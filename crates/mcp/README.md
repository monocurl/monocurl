# Monocurl MCP Server

`monocurl-mcp` is a small stdio MCP server that exposes Monocurl authoring
documentation as MCP resources and exposes read-only validation/execution tools.
It does not edit or render scenes yet.

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
- `monocurl://docs/stdlib`
- `monocurl://stdlib/util`
- `monocurl://stdlib/math`
- `monocurl://stdlib/color`
- `monocurl://stdlib/mesh`
- `monocurl://stdlib/anim`
- `monocurl://stdlib/scene`

## Tools

- `monocurl_check`: parses and compiles a root scene, returning structured
  parse/compiler diagnostics, warnings, slide count, and slide names.
- `monocurl_seek`: parses, compiles, executes up to a requested timestamp, and
  returns diagnostics, runtime errors, the reached timestamp, and transcript
  entries from `print` statements.

Both tools accept:

```json
{
  "source": "slide\nprint 1 + 2\n",
  "rootPath": "scene.mcl",
  "openDocuments": {
    "lib/helpers.mcl": "let helper = |x| x + 1\n"
  }
}
```

`monocurl_seek` also requires `slide` and optionally accepts `time` or `atEnd`:

```json
{
  "source": "slide\nprint 1 + 2\n",
  "slide": 1,
  "atEnd": true
}
```

Visible slides are addressed as `1..=slideCount`. `slide = 0` with
`atEnd = true` refers to the pre-scene boundary.

## Publishing Through GitHub Later

The MCP Registry stores metadata, not the binary itself. A GitHub-based flow
would be:

1. Create a GitHub release containing a packaged `monocurl-mcp` artifact.
2. Fill in `server.github.example.json` with the release URL and SHA-256.
3. Rename or copy it to `server.json`.
4. Authenticate with `mcp-publisher login github`.
5. Publish with `mcp-publisher publish`.

This crate is intentionally usable before that publishing work exists.
