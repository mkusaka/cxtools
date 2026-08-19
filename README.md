# cxtools

An MCP (Model Context Protocol) server that exposes [OpenAI Codex](https://github.com/openai/codex) tools by calling the codex-rs crates **in-process** — not a reimplementation.

cxtools depends on the codex-rs workspace crates as git dependencies (pinned to [mkusaka/codex](https://github.com/mkusaka/codex), a fork that only adjusts the workspace version for downstream consumption). `shell_command` and `apply_patch` run the exact functions Codex itself runs. Tool names, descriptions, and parameters follow the specs Codex presents to its model (`codex-rs/core/src/tools/handlers/*_spec.rs`).

## Tools

| Tool | Backed by | Fidelity |
| --- | --- | --- |
| `shell_command` | `codex_core::exec::process_exec_tool_call` | Codex's own execution path: default 10s timeout, output caps/truncation |
| `apply_patch` | `codex_apply_patch::apply_patch` | Codex's own patch parser/applier |
| `view_image` | `tokio::fs` + base64, returned as MCP image content | Equivalent (Codex reads the file and attaches it the same way) |
| `subagent` | `codex exec --ephemeral` (Codex CLI on PATH) | Delegation — Codex's internal multi-agent tooling is not extractable |

### Tool inputs

- `shell_command`: `command` (required), `workdir` (optional), `timeout_ms` (optional, defaults to 10000 ms)
- `apply_patch`: `input` (required, `*** Begin Patch` ... `*** End Patch` format), `cwd` (optional, cxtools extension)
- `view_image`: `path` (required, png/jpg/gif/webp)
- `subagent`: `prompt` (required), `cwd` (optional)

### Output schemas

`shell_command`, `apply_patch`, and `subagent` declare an `outputSchema` and return matching `structuredContent` (in addition to the usual text content, for clients that don't read structured output):

- `shell_command` → `{ output: string, exit_code: number, timed_out: boolean }`
- `apply_patch` → `{ success: boolean, output: string }`
- `subagent` → `{ success: boolean, output: string }`

These shapes are cxtools additions, not copied from Codex — the specs these tools otherwise mirror (`shell_spec.rs`, `apply_patch_spec.rs`) leave `output_schema: None` for them. `view_image` has no output schema: it returns binary image content, which `structuredContent` (JSON-only) can't carry.

### Deliberately not exposed

- **`web_search`** — in Codex this is a hosted tool: the Responses API executes it server-side. There is no local implementation to export.
- **Sandboxing / approvals** — `shell_command` runs unsandboxed (`PermissionProfile::Disabled`); command approval is the MCP client's responsibility.

## Requirements

- Rust (see `codex-rs`'s minimum; 1.95+)
- For `subagent`: a logged-in Codex CLI (`codex login`) on PATH

## Setup

Install via Homebrew:

```bash
brew tap mkusaka/tap
brew install mkusaka/tap/cxtools
```

Tagged releases publish Homebrew bottles for Apple Silicon and Intel Macs on
macOS Sequoia 15 and Tahoe 26. Until the first tagged release is published,
or on unsupported platforms, install from `HEAD`:

```bash
brew install --HEAD mkusaka/tap/cxtools
```

Or build from source:

```bash
cargo build --release
# or install to ~/.cargo/bin (--locked is required: a fresh resolution
# drifts rama-* crates away from the versions codex's lockfile pins)
cargo install --path . --locked
```

Register as a stdio MCP server. For Claude Code (if installed via Homebrew or
`cargo install`, `cxtools` is already on `PATH`; otherwise use the full path
to `target/release/cxtools`):

```bash
claude mcp add cxtools -- cxtools
```

Or in an `mcpServers` config:

```json
{
  "mcpServers": {
    "cxtools": {
      "command": "/path/to/cxtools/target/release/cxtools"
    }
  }
}
```

## Development

```bash
cargo test
cargo clippy --all-targets
cargo fmt
```

### Updating the pinned codex revision

Bump the `rev` on every `codex-*` dependency in `Cargo.toml` (all must point at the same commit), and mirror any changes to the `[patch.crates-io]` section from `codex-rs/Cargo.toml` at that revision. These are unpublished internal crates — expect API churn on upgrade.

## License

MIT
