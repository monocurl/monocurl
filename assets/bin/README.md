Place the release Tectonic executable here when building a bundled Monocurl app.

Expected names:

- macOS/Linux: `tectonic`
- Windows: `tectonic.exe`

The file is intentionally gitignored because it is platform-specific release
payload. Local development can also point Monocurl at a binary with
`MONOCURL_TECTONIC_BIN`, or point the whole app at an alternate assets tree with
`MONOCURL_ASSETS_DIR`.
