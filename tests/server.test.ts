import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { afterAll, beforeAll, expect, test } from "vitest";
import { createServer } from "../src/server.js";

let client: Client;
let dir: string;

beforeAll(async () => {
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  await createServer().connect(serverTransport);
  client = new Client({ name: "test", version: "0.0.0" });
  await client.connect(clientTransport);
  dir = await mkdtemp(path.join(tmpdir(), "cxtools-test-"));
});

afterAll(async () => {
  await rm(dir, { recursive: true, force: true });
});

test("lists all tools", async () => {
  const { tools } = await client.listTools();
  expect(tools.map((t) => t.name).sort()).toEqual([
    "apply_patch",
    "read_file",
    "shell",
    "subagent",
    "view_image",
    "web_search",
  ]);
});

test("shell runs a command", async () => {
  const result = await client.callTool({
    name: "shell",
    arguments: { command: "echo hello" },
  });
  expect(result.isError).toBe(false);
  expect((result.content as { text: string }[])[0].text).toContain("hello");
});

test("shell reports failure", async () => {
  const result = await client.callTool({
    name: "shell",
    arguments: { command: "exit 3" },
  });
  expect(result.isError).toBe(true);
});

test("apply_patch creates a file via codex", async () => {
  const patch = [
    "*** Begin Patch",
    "*** Add File: hello.txt",
    "+hello from apply_patch",
    "*** End Patch",
  ].join("\n");
  const result = await client.callTool({
    name: "apply_patch",
    arguments: { patch, cwd: dir },
  });
  expect(result.isError).toBe(false);
  expect(await readFile(path.join(dir, "hello.txt"), "utf8")).toBe("hello from apply_patch\n");
}, 30000);

test("read_file returns file contents", async () => {
  const file = path.join(dir, "read-me.txt");
  await writeFile(file, "file body");
  const result = await client.callTool({
    name: "read_file",
    arguments: { path: file },
  });
  expect((result.content as { text: string }[])[0].text).toBe("file body");
});

test("view_image returns image content", async () => {
  const png = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==",
    "base64",
  );
  const file = path.join(dir, "dot.png");
  await writeFile(file, png);
  const result = await client.callTool({
    name: "view_image",
    arguments: { path: file },
  });
  const content = (result.content as { type: string; mimeType: string }[])[0];
  expect(content.type).toBe("image");
  expect(content.mimeType).toBe("image/png");
});
