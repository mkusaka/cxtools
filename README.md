# cxtools

An MCP (Model Context Protocol) server that exposes Codex-style tools, backed by the [OpenAI Codex CLI](https://github.com/openai/codex) (`@openai/codex`).

Instead of reimplementing agent tooling, cxtools delegates to the Codex binary bundled with the npm package: patches are applied with Codex's own `apply_patch` implementation, and web search / subagent tasks run through `codex exec`.

## Tools

| Tool | Description | Backed by |
| --- | --- | --- |
| `shell` | Run a shell command and return stdout/stderr and the exit code | `bash -lc` |
| `apply_patch` | Create, edit, or delete files by applying a patch in Codex `apply_patch` format | `codex --codex-run-as-apply-patch` |
| `read_file` | Read a text file and return its contents | `node:fs` |
| `view_image` | Return a local image (png/jpg/gif/webp) as MCP image content | `node:fs` + base64 |
| `web_search` | Search the web via a Codex agent and return findings with source URLs | `codex exec -c web_search=live` |
| `subagent` | Delegate a task to a non-interactive Codex agent and return its final message | `codex exec --ephemeral` |

### Tool inputs

- `shell`: `command` (required), `cwd` (optional)
- `apply_patch`: `patch` (required, `*** Begin Patch` ... `*** End Patch` format), `cwd` (optional)
- `read_file`: `path` (required)
- `view_image`: `path` (required)
- `web_search`: `query` (required)
- `subagent`: `prompt` (required), `cwd` (optional)

## Requirements

- Node.js >= 20 (>= 22.18 to run the TypeScript sources directly)
- A Codex CLI login (`codex login`) for `web_search` and `subagent` — they consume your Codex usage

## Setup

```bash
pnpm install
pnpm build
```

Register as a stdio MCP server. For Claude Code:

```bash
claude mcp add cxtools -- node /path/to/cxtools/dist/index.js
```

Or in an `mcpServers` config:

```json
{
  "mcpServers": {
    "cxtools": {
      "command": "node",
      "args": ["/path/to/cxtools/dist/index.js"]
    }
  }
}
```

On Node.js >= 22.18 you can skip the build and point at the TypeScript entry directly: `node /path/to/cxtools/src/index.ts`.

## Development

```bash
pnpm test       # vitest
pnpm lint       # oxlint
pnpm fmt        # oxfmt
pnpm fmt:check  # oxfmt --check
pnpm build      # tsc -> dist/
```

Tests exercise the server end-to-end over an in-memory MCP transport, including a real `apply_patch` round-trip through the Codex binary. `web_search` and `subagent` are not covered by tests because they require a Codex login.

## License

MIT
