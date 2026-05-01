# Monocurl MCP Server

`monocurl-mcp` is a small stdio MCP server that exposes Monocurl authoring
documentation as MCP resources. It does not edit, validate, execute, or render
scenes.

## Run From Source

```sh
cargo run -p mcp --bin monocurl-mcp
```

Example local MCP client configuration:

```json
{
  "mcpServers": {
    "monocurl": {
      "command": "cargo",
      "args": ["run", "-p", "mcp", "--bin", "monocurl-mcp"]
    }
  }
}
```

For a release build:

```sh
cargo build --release -p mcp --bin monocurl-mcp
```

Then point the client at `target/release/monocurl-mcp`.

## Resources

- `monocurl://docs/language-semantics`
- `monocurl://docs/ai-overview`
- `monocurl://docs/language-basics`
- `monocurl://docs/meshes`
- `monocurl://docs/animations`
- `monocurl://docs/params-camera`
- `monocurl://docs/debugging-patterns`
- `monocurl://docs/cheat-sheet`
- `monocurl://docs/stdlib`
- `monocurl://docs/cli`
- `monocurl://examples/riemann-rectangles`
- `monocurl://stdlib/util`
- `monocurl://stdlib/math`
- `monocurl://stdlib/color`
- `monocurl://stdlib/mesh`
- `monocurl://stdlib/anim`
- `monocurl://stdlib/scene`

## Tools

This server intentionally exposes no tools. Compile checks, execution, seeking,
transcript output, and rendering belong in the Monocurl CLI/application layer.
See `monocurl://docs/cli` for the shared GUI/CLI binary invocation details.

## Publishing Through GitHub Later

The MCP Registry stores metadata, not the binary itself. A GitHub-based flow
would be:

1. Create a GitHub release containing a packaged `monocurl-mcp` artifact.
2. Fill in `server.github.example.json` with the release URL and SHA-256.
3. Rename or copy it to `server.json`.
4. Authenticate with `mcp-publisher login github`.
5. Publish with `mcp-publisher publish`.

This crate is intentionally usable before that publishing work exists.
