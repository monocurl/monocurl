import test from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";

type JsonRpcResponse = {
  id?: number;
  result?: Record<string, unknown>;
  error?: {
    code: number;
    message: string;
  };
};

test("answers standard list probes during MCP startup", async () => {
  const child = spawn(process.execPath, ["dist/index.js"], {
    stdio: ["pipe", "pipe", "pipe"],
  });

  let stdout = "";
  let stderr = "";
  const responses: JsonRpcResponse[] = [];

  child.stdout.on("data", (chunk: Buffer) => {
    stdout += chunk.toString("utf8");

    for (;;) {
      const newline = stdout.indexOf("\n");
      if (newline === -1) {
        break;
      }

      const line = stdout.slice(0, newline).trim();
      stdout = stdout.slice(newline + 1);

      if (line) {
        responses.push(JSON.parse(line) as JsonRpcResponse);
      }
    }
  });

  child.stderr.on("data", (chunk: Buffer) => {
    stderr += chunk.toString("utf8");
  });

  const send = (message: unknown) => {
    child.stdin.write(`${JSON.stringify(message)}\n`);
  };

  const waitFor = async (id: number): Promise<JsonRpcResponse> => {
    const deadline = Date.now() + 10_000;

    while (Date.now() < deadline) {
      const response = responses.find((candidate) => candidate.id === id);
      if (response) {
        return response;
      }

      await new Promise((resolve) => setTimeout(resolve, 25));
    }

    throw new Error(`timed out waiting for response ${id}: ${stderr}`);
  };

  try {
    send({
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: {
        protocolVersion: "2025-06-18",
        capabilities: {},
        clientInfo: {
          name: "monocurl-mcp-test",
          version: "0.0.0",
        },
      },
    });

    const initialized = await waitFor(1);
    assert.deepEqual(initialized.error, undefined);

    send({
      jsonrpc: "2.0",
      method: "notifications/initialized",
      params: {},
    });
    send({ jsonrpc: "2.0", id: 2, method: "tools/list", params: {} });
    send({ jsonrpc: "2.0", id: 3, method: "prompts/list", params: {} });
    send({ jsonrpc: "2.0", id: 4, method: "resources/list", params: {} });

    const tools = await waitFor(2);
    const prompts = await waitFor(3);
    const resources = await waitFor(4);

    assert.deepEqual(tools.error, undefined);
    assert.deepEqual(prompts.error, undefined);
    assert.deepEqual(resources.error, undefined);
    assert.deepEqual(tools.result?.tools, []);
    assert.deepEqual(prompts.result?.prompts, []);
    assert.equal(
      (resources.result?.resources as unknown[] | undefined)?.length,
      17,
    );
  } finally {
    child.kill();
  }
});
