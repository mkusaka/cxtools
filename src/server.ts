import { execFile } from "node:child_process";
import { randomUUID } from "node:crypto";
import { readFile, unlink } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";

const execFileAsync = promisify(execFile);
const require = createRequire(import.meta.url);
const codexJs = require.resolve("@openai/codex/bin/codex.js");

const MAX_BUFFER = 10 * 1024 * 1024;

interface RunResult {
  stdout: string;
  stderr: string;
  exitCode: number;
}

async function run(file: string, args: string[], cwd?: string): Promise<RunResult> {
  try {
    const promise = execFileAsync(file, args, {
      cwd,
      maxBuffer: MAX_BUFFER,
    });
    // codex exec reads stdin until EOF when it is a pipe; close it so it never hangs.
    promise.child.stdin?.end();
    const { stdout, stderr } = await promise;
    return { stdout, stderr, exitCode: 0 };
  } catch (error) {
    const e = error as NodeJS.ErrnoException & RunResult & { code?: unknown };
    return {
      stdout: e.stdout ?? "",
      stderr: e.stderr ?? String(error),
      exitCode: typeof e.code === "number" ? e.code : 1,
    };
  }
}

function runCodex(args: string[], cwd?: string): Promise<RunResult> {
  return run(process.execPath, [codexJs, ...args], cwd);
}

function toResult(result: RunResult) {
  const text =
    [result.stdout, result.stderr].filter(Boolean).join("\n") ||
    `(no output, exit code ${result.exitCode})`;
  return {
    content: [{ type: "text" as const, text }],
    isError: result.exitCode !== 0,
  };
}

/// Run `codex exec` non-interactively and return the agent's last message.
async function codexExec(
  prompt: string,
  extraArgs: string[],
  cwd?: string,
): Promise<ReturnType<typeof toResult>> {
  const outFile = path.join(tmpdir(), `cxtools-${randomUUID()}.txt`);
  try {
    const result = await runCodex(
      [
        "exec",
        "--ephemeral",
        "--skip-git-repo-check",
        "--output-last-message",
        outFile,
        ...extraArgs,
        prompt,
      ],
      cwd,
    );
    if (result.exitCode !== 0) {
      return toResult(result);
    }
    const lastMessage = await readFile(outFile, "utf8").catch(() => "");
    return {
      content: [{ type: "text" as const, text: lastMessage || result.stdout }],
      isError: false,
    };
  } finally {
    await unlink(outFile).catch(() => {});
  }
}

const IMAGE_MIME: Record<string, string> = {
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".gif": "image/gif",
  ".webp": "image/webp",
};

export function createServer(): McpServer {
  const server = new McpServer({ name: "cxtools", version: "0.1.0" });

  server.registerTool(
    "shell",
    {
      description: "Run a shell command (bash -lc) and return stdout/stderr and the exit code.",
      inputSchema: {
        command: z.string().describe("Shell command to run"),
        cwd: z.string().optional().describe("Working directory (default: server cwd)"),
      },
    },
    async ({ command, cwd }) => toResult(await run("bash", ["-lc", command], cwd)),
  );

  server.registerTool(
    "apply_patch",
    {
      description:
        "Create, edit, or delete files by applying a patch in Codex apply_patch format " +
        "(*** Begin Patch / *** Add File: / *** Update File: / *** Delete File: / *** End Patch).",
      inputSchema: {
        patch: z.string().describe("Patch in Codex apply_patch format"),
        cwd: z
          .string()
          .optional()
          .describe("Directory to apply the patch in (default: server cwd)"),
      },
    },
    async ({ patch, cwd }) => toResult(await runCodex(["--codex-run-as-apply-patch", patch], cwd)),
  );

  server.registerTool(
    "read_file",
    {
      description: "Read a text file and return its contents.",
      inputSchema: {
        path: z.string().describe("Path to the file"),
      },
    },
    async ({ path: filePath }) => {
      try {
        const text = await readFile(filePath, "utf8");
        return { content: [{ type: "text" as const, text }] };
      } catch (error) {
        return {
          content: [{ type: "text" as const, text: String(error) }],
          isError: true,
        };
      }
    },
  );

  server.registerTool(
    "view_image",
    {
      description: "Read a local image file (png/jpg/gif/webp) and return it as image content.",
      inputSchema: {
        path: z.string().describe("Path to the image file"),
      },
    },
    async ({ path: filePath }) => {
      const mimeType = IMAGE_MIME[path.extname(filePath).toLowerCase()];
      if (!mimeType) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Unsupported image extension: ${filePath}`,
            },
          ],
          isError: true,
        };
      }
      try {
        const data = await readFile(filePath);
        return {
          content: [{ type: "image" as const, data: data.toString("base64"), mimeType }],
        };
      } catch (error) {
        return {
          content: [{ type: "text" as const, text: String(error) }],
          isError: true,
        };
      }
    },
  );

  server.registerTool(
    "web_search",
    {
      description:
        "Search the web via a Codex agent and return a summary of findings with source URLs.",
      inputSchema: {
        query: z.string().describe("What to search for"),
      },
    },
    async ({ query }) =>
      codexExec(
        `Search the web for the following and report your findings concisely, citing source URLs:\n\n${query}`,
        ["-c", "web_search=live"],
      ),
  );

  server.registerTool(
    "subagent",
    {
      description:
        "Delegate a task (research, investigation, implementation, ...) to a non-interactive Codex agent and return its final message.",
      inputSchema: {
        prompt: z.string().describe("Task instructions for the subagent"),
        cwd: z.string().optional().describe("Working directory for the subagent"),
      },
    },
    async ({ prompt, cwd }) => codexExec(prompt, [], cwd),
  );

  return server;
}
