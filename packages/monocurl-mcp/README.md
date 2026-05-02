# monocurl-mcp

Cross-platform TypeScript MCP server for Monocurl documentation resources.

The server exposes the Monocurl documentation resource set:

- split Monocurl authoring docs
- standard library overview
- raw stdlib wrapper sources
- CLI usage notes
- the Riemann rectangles example scene

## Development

```sh
npm install
npm test
```

Recreate the package symlinks to the asset-backed docs and icon:

```sh
npm run sync:docs
```

## Client Config

After building with `npm run build`, use:

```json
{
  "mcpServers": {
    "monocurl-mcp": {
      "command": "node",
      "args": ["/absolute/path/to/packages/monocurl-mcp/dist/index.js"]
    }
  }
}
```

For MCPB packaging:

```sh
npm run prepare:mcpb
```
