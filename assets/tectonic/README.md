The bundled LaTeX backend uses the `tectonic` Rust crate directly (no external binary required).
Monocurl first checks the local seed bundle in this directory. If a requested TeX support
file is not present, it falls back to Tectonic's default bundle, downloads that one file on
demand, and caches it in the system cache directory (e.g. `~/.cache/Tectonic/`).

Supported local seed bundle locations:

- `assets/tectonic/bundle/`    — a directory bundle (contains TeX files directly)
- `assets/tectonic/bundle.zip` — a ZIP bundle

`bundle.zip` is generated from a warmed Tectonic cache and includes `SHA256SUM` for the
matching upstream default bundle, so Tectonic can reuse its normal format cache.
`default_bundle_v33.index` is the upstream indexed-tar file list; Monocurl uses it to avoid
network fallback for TeX probes that are not support files.

The pinned `tectonic = 0.15` backend uses the legacy indexed-tar default bundle:
  https://relay.fullyjustified.net/default_bundle_v33.tar

Newer Tectonic releases can create/read TTBv1 bundles with:
  tectonic -X bundle create --build-dir ./build texlive2023/bundle.toml v1

The bundle tooling lives in the upstream Tectonic repository under `bundles/`, but the
pre-generated default bundle is the simpler source when you just want common LaTeX support.

For local development you can also override the bundle path with the env var:
  MONOCURL_TECTONIC_BUNDLE=/path/to/bundle

Or point the whole app at an alternate assets tree with:
  MONOCURL_ASSETS_DIR=/path/to/assets
