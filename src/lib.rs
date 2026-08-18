use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;

use base64::Engine;
use codex_core::exec::ExecCapturePolicy;
use codex_core::exec::ExecExpiration;
use codex_core::exec::ExecParams;
use codex_core::exec::process_exec_tool_call;
use codex_core::sandboxing::SandboxPermissions;
use codex_protocol::models::PermissionProfile;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_path_uri::PathUri;
use rmcp::ErrorData as McpError;
use rmcp::ServerHandler;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::model::ContentBlock;
use rmcp::model::Implementation;
use rmcp::model::ServerCapabilities;
use rmcp::model::ServerInfo;
use rmcp::tool;
use rmcp::tool_handler;
use rmcp::tool_router;
use serde::Deserialize;

fn text_result(text: impl Into<String>, is_error: bool) -> CallToolResult {
    let content = vec![ContentBlock::text(text.into())];
    if is_error {
        CallToolResult::error(content)
    } else {
        CallToolResult::success(content)
    }
}

fn resolve_cwd(dir: Option<&str>) -> std::io::Result<AbsolutePathBuf> {
    match dir {
        Some(dir) => AbsolutePathBuf::from_absolute_path(dir),
        None => AbsolutePathBuf::current_dir(),
    }
}

// Parameter names and descriptions match the tool specs codex presents to the
// model (codex-rs/core/src/tools/handlers/*_spec.rs at the pinned revision).

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ShellCommandParams {
    /// Shell script to run in the user's default shell.
    pub command: String,
    /// Working directory for the command. Defaults to the server process cwd.
    pub workdir: Option<String>,
    /// Maximum command runtime. Defaults to 10000 ms.
    pub timeout_ms: Option<u64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ApplyPatchParams {
    /// The entire contents of the apply_patch command.
    pub input: String,
    /// Directory relative paths in the patch are resolved against. Defaults to
    /// the server process cwd. (cxtools extension; codex resolves against the
    /// turn cwd.)
    pub cwd: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ViewImageParams {
    /// Local filesystem path to an image file.
    pub path: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SubagentParams {
    /// Task instructions for the non-interactive Codex agent.
    pub prompt: String,
    /// Working directory for the agent. Defaults to the server process cwd.
    pub cwd: Option<String>,
}

// #[tool_handler] routes through Self::tool_router(), so no router field is needed.
#[derive(Default)]
pub struct CxTools;

#[tool_router]
impl CxTools {
    pub fn new() -> Self {
        Self
    }

    #[tool(
        name = "shell_command",
        description = "Runs a shell command and returns its output.\n- Always set the `workdir` param when using the shell_command function. Do not use `cd` unless absolutely necessary."
    )]
    pub async fn shell_command(
        &self,
        Parameters(params): Parameters<ShellCommandParams>,
    ) -> Result<CallToolResult, McpError> {
        let cwd = match resolve_cwd(params.workdir.as_deref()) {
            Ok(cwd) => cwd,
            Err(e) => return Ok(text_result(format!("invalid workdir: {e}"), true)),
        };
        let exec_params = ExecParams {
            command: vec!["bash".to_string(), "-lc".to_string(), params.command],
            cwd: cwd.clone(),
            expiration: ExecExpiration::from(params.timeout_ms),
            capture_policy: ExecCapturePolicy::ShellTool,
            env: std::env::vars().collect::<HashMap<String, String>>(),
            network: None,
            network_environment_id: None,
            sandbox_permissions: SandboxPermissions::UseDefault,
            windows_sandbox_level: Default::default(),
            windows_sandbox_private_desktop: false,
            justification: None,
            arg0: None,
        };
        match process_exec_tool_call(
            exec_params,
            &PermissionProfile::Disabled,
            &cwd,
            &[],
            &None,
            false,
            None,
        )
        .await
        {
            Ok(output) => {
                let mut text = output.aggregated_output.text.clone();
                if output.timed_out {
                    text = format!("command timed out\n{text}");
                }
                if output.exit_code != 0 {
                    text = format!("{text}\n(exit code {})", output.exit_code);
                }
                Ok(text_result(text, output.exit_code != 0))
            }
            Err(e) => Ok(text_result(format!("exec error: {e}"), true)),
        }
    }

    #[tool(
        name = "apply_patch",
        description = "The `apply_patch` tool can be used to edit files. Provide the patch in the apply_patch format (*** Begin Patch / *** Add File: / *** Update File: / *** Delete File: / *** End Patch)."
    )]
    pub async fn apply_patch(
        &self,
        Parameters(params): Parameters<ApplyPatchParams>,
    ) -> Result<CallToolResult, McpError> {
        let cwd = match resolve_cwd(params.cwd.as_deref()) {
            Ok(cwd) => cwd,
            Err(e) => return Ok(text_result(format!("invalid cwd: {e}"), true)),
        };
        let cwd: PathUri = cwd.into();
        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();
        let result = codex_apply_patch::apply_patch(
            &params.input,
            &cwd,
            &mut stdout,
            &mut stderr,
            codex_exec_server::LOCAL_FS.as_ref(),
            None,
        )
        .await;
        let stdout = String::from_utf8_lossy(&stdout);
        let stderr = String::from_utf8_lossy(&stderr);
        match result {
            Ok(_) => Ok(text_result(stdout, false)),
            Err(_) => Ok(text_result(
                [stdout.as_ref(), stderr.as_ref()]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .copied()
                    .collect::<Vec<_>>()
                    .join("\n"),
                true,
            )),
        }
    }

    #[tool(
        name = "view_image",
        description = "View a local image file from the filesystem when visual inspection is needed. Use this for images already available on disk."
    )]
    pub async fn view_image(
        &self,
        Parameters(params): Parameters<ViewImageParams>,
    ) -> Result<CallToolResult, McpError> {
        let mime = match Path::new(&params.path)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("png") => "image/png",
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("webp") => "image/webp",
            _ => {
                return Ok(text_result(
                    format!("unsupported image extension: {}", params.path),
                    true,
                ));
            }
        };
        match tokio::fs::read(&params.path).await {
            Ok(bytes) => {
                let data = base64::engine::general_purpose::STANDARD.encode(bytes);
                Ok(CallToolResult::success(vec![ContentBlock::image(
                    data, mime,
                )]))
            }
            Err(e) => Ok(text_result(format!("failed to read image: {e}"), true)),
        }
    }

    #[tool(
        name = "subagent",
        description = "Delegate a task (research, investigation, implementation, ...) to a non-interactive Codex agent (`codex exec`) and return its final message. Requires a logged-in Codex CLI on PATH."
    )]
    pub async fn subagent(
        &self,
        Parameters(params): Parameters<SubagentParams>,
    ) -> Result<CallToolResult, McpError> {
        let out_file = std::env::temp_dir().join(format!(
            "cxtools-last-message-{}.txt",
            std::process::id() as u64
                ^ std::time::UNIX_EPOCH
                    .elapsed()
                    .unwrap_or_default()
                    .as_nanos() as u64
        ));
        let mut cmd = tokio::process::Command::new("codex");
        cmd.arg("exec")
            .arg("--ephemeral")
            .arg("--skip-git-repo-check")
            .arg("--output-last-message")
            .arg(&out_file)
            .stdin(Stdio::null());
        if let Some(cwd) = &params.cwd {
            cmd.arg("-C").arg(cwd);
        }
        cmd.arg(&params.prompt);
        let result = cmd.output().await;
        let last_message = tokio::fs::read_to_string(&out_file)
            .await
            .unwrap_or_default();
        let _ = tokio::fs::remove_file(&out_file).await;
        match result {
            Ok(output) if output.status.success() => {
                let text = if last_message.is_empty() {
                    String::from_utf8_lossy(&output.stdout).into_owned()
                } else {
                    last_message
                };
                Ok(text_result(text, false))
            }
            Ok(output) => Ok(text_result(
                format!(
                    "codex exec failed (exit code {:?}):\n{}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stderr)
                ),
                true,
            )),
            Err(e) => Ok(text_result(format!("failed to launch codex: {e}"), true)),
        }
    }
}

#[tool_handler]
impl ServerHandler for CxTools {
    fn get_info(&self) -> ServerInfo {
        let mut server_info = Implementation::default();
        server_info.name = "cxtools".to_string();
        server_info.version = env!("CARGO_PKG_VERSION").to_string();
        let mut info = ServerInfo::default();
        info.server_info = server_info;
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.instructions = Some(
            "Codex tools served over MCP: shell_command and apply_patch run the actual \
             codex-rs implementations in-process; subagent delegates to `codex exec`."
                .to_string(),
        );
        info
    }
}
