use cxtools::ApplyPatchParams;
use cxtools::CxTools;
use cxtools::ShellCommandParams;
use cxtools::ViewImageParams;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::ContentBlock;

fn text_of(result: &rmcp::model::CallToolResult) -> String {
    match &result.content[0] {
        ContentBlock::Text(t) => t.text.clone(),
        other => panic!("expected text content, got {other:?}"),
    }
}

#[tokio::test]
async fn shell_command_runs_and_captures_output() {
    let server = CxTools::new();
    let result = server
        .shell_command(Parameters(ShellCommandParams {
            command: "echo hello".to_string(),
            workdir: None,
            timeout_ms: None,
        }))
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(false));
    assert!(text_of(&result).contains("hello"));

    let structured = result.structured_content.expect("structured_content set");
    assert_eq!(structured["exit_code"], 0);
    assert_eq!(structured["timed_out"], false);
    assert!(structured["output"].as_str().unwrap().contains("hello"));
}

#[tokio::test]
async fn shell_command_advertises_output_schema() {
    let router = CxTools::tool_router();
    let tool = router
        .list_all()
        .into_iter()
        .find(|t| t.name == "shell_command")
        .expect("shell_command tool registered");
    assert!(tool.output_schema.is_some());
}

#[tokio::test]
async fn shell_command_reports_failure() {
    let server = CxTools::new();
    let result = server
        .shell_command(Parameters(ShellCommandParams {
            command: "exit 3".to_string(),
            workdir: None,
            timeout_ms: None,
        }))
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(true));
    assert!(text_of(&result).contains("exit code 3"));
}

#[tokio::test]
async fn shell_command_times_out() {
    let server = CxTools::new();
    let result = server
        .shell_command(Parameters(ShellCommandParams {
            command: "sleep 5".to_string(),
            workdir: None,
            timeout_ms: Some(200),
        }))
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(true));
    assert!(text_of(&result).contains("timed out"));
}

#[tokio::test]
async fn apply_patch_creates_file() {
    let dir = tempdir();
    let server = CxTools::new();
    let patch = "*** Begin Patch\n*** Add File: hello.txt\n+hello from apply_patch\n*** End Patch";
    let result = server
        .apply_patch(Parameters(ApplyPatchParams {
            input: patch.to_string(),
            cwd: Some(dir.to_string_lossy().into_owned()),
        }))
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(false), "{}", text_of(&result));
    let contents = std::fs::read_to_string(dir.join("hello.txt")).unwrap();
    assert_eq!(contents, "hello from apply_patch\n");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn apply_patch_rejects_invalid_patch() {
    let server = CxTools::new();
    let result = server
        .apply_patch(Parameters(ApplyPatchParams {
            input: "not a patch".to_string(),
            cwd: None,
        }))
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(true));
}

#[tokio::test]
async fn view_image_returns_image_content() {
    let dir = tempdir();
    // 1x1 transparent PNG.
    let png: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x64,
        0x60, 0xF8, 0x5F, 0x0F, 0x00, 0x02, 0x87, 0x01, 0x80, 0xEB, 0x47, 0xBA, 0x92, 0x00, 0x00,
        0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    let path = dir.join("dot.png");
    std::fs::write(&path, png).unwrap();
    let server = CxTools::new();
    let result = server
        .view_image(Parameters(ViewImageParams {
            path: path.to_string_lossy().into_owned(),
        }))
        .await
        .unwrap();
    match &result.content[0] {
        ContentBlock::Image(image) => assert_eq!(image.mime_type, "image/png"),
        other => panic!("expected image content, got {other:?}"),
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

fn tempdir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cxtools-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}
