#!/usr/bin/env node

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  ErrorCode,
  ListPromptsRequestSchema,
  ListResourceTemplatesRequestSchema,
  ListResourcesRequestSchema,
  ListToolsRequestSchema,
  McpError,
  ReadResourceRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";

import {
  listResources,
  readResource,
  SERVER_INSTRUCTIONS,
  SERVER_NAME,
  SERVER_TITLE,
  SERVER_VERSION,
} from "./resources.js";

const server = new Server(
  {
    name: SERVER_NAME,
    title: SERVER_TITLE,
    version: SERVER_VERSION,
  },
  {
    capabilities: {
      prompts: {},
      resources: {},
      tools: {},
    },
    instructions: SERVER_INSTRUCTIONS,
  },
);

server.setRequestHandler(ListResourcesRequestSchema, async () => ({
  resources: listResources(),
  nextCursor: undefined,
}));

server.setRequestHandler(ReadResourceRequestSchema, async (request) => {
  try {
    const resource = readResource(request.params.uri);

    return {
      contents: [
        {
          uri: resource.uri,
          mimeType: resource.mimeType,
          text: resource.text,
        },
      ],
    };
  } catch {
    throw new McpError(ErrorCode.InvalidRequest, "resource_not_found", {
      uri: request.params.uri,
    });
  }
});

server.setRequestHandler(ListResourceTemplatesRequestSchema, async () => ({
  resourceTemplates: [],
  nextCursor: undefined,
}));

server.setRequestHandler(ListPromptsRequestSchema, async () => ({
  prompts: [],
  nextCursor: undefined,
}));

server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: [],
  nextCursor: undefined,
}));

async function main(): Promise<void> {
  const transport = new StdioServerTransport();
  await server.connect(transport);
}

main().catch((error: unknown) => {
  console.error(error);
  process.exit(1);
});
