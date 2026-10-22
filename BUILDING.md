# Build Instructions

Monocurl is a Rust workspace. The GUI currently targets macOS, Windows, and
Linux. Use a Rust toolchain with Edition 2024 support, 

Monocurl currently relies on Tectonic for bundled LaTeX support. Tectonic
discovers native dependencies through `pkg-config`/`pkgconf` on Unix systems
and either `pkg-config` or `vcpkg` on Windows. During the build process, it may be the case that tectonic cannot find a
dependency, in which case you should install the relevant package and rebuild.

Prefer putting local build environment in `.cargo/config.toml` instead of
exporting variables in your shell. 

## Running

For all platforms, once you have done the required environment setup (see below), you can build and run the editor with:

```sh
cargo run --package monocurl
```

## Fast optimized builds

`--release` is what ships: fat LTO with one codegen unit, so after editing a
workspace crate a rebuild relinks everything (about 7 minutes on an M-series
laptop at `-j 3`). For iterating on performance or rendering videos locally, the
`release-fast` profile keeps release's optimization level but turns LTO off,
uses 16 codegen units and builds incrementally:

```sh
cargo build --profile release-fast -p monocurl
# the binary is target/release-fast/monocurl
```

Measured on the same machine: a rebuild after a one-line change in `executor` or
`exporter` takes 12-29 seconds instead of 6m45s, and render CPU time is within
about 7% of `--release` (same output). A first build from scratch takes about as
long as `--release`. Benchmarks and anything you publish should still use
`--release`.

## macOS

Install Xcode's command-line tools, Rust, and the native package. 

```sh
xcode-select --install
brew install pkg-config icu4c
```

Homebrew's `icu4c` is keg-only, so make its `.pc` files visible to
`pkg-config`. On Apple Silicon Homebrew normally lives under `/opt/homebrew`; on
Intel macOS it normally lives under `/usr/local`.

Template `.cargo/config.toml` for Apple Silicon:

```toml
[env]
MONOCURL_ASSETS_DIR = { value = "assets", relative = true }
PKG_CONFIG_PATH = "/opt/homebrew/opt/icu4c/lib/pkgconfig"
CFLAGS = "-Wno-int-conversion"
CXXFLAGS = "-std=c++17"
```

For Intel Homebrew, you may need to instead use `/usr/local`:

```toml
[env]
MONOCURL_ASSETS_DIR = { value = "assets", relative = true }
PKG_CONFIG_PATH = "/usr/local/opt/icu4c/lib/pkgconfig"
CFLAGS = "-Wno-int-conversion"
CXXFLAGS = "-std=c++17"
```

## Linux

Install Rust, a C/C++ toolchain, `pkg-config`,  ICU,
OpenSSL, fontconfig, and the Linux window-system development packages. 

Debian/Ubuntu:

```sh
sudo apt update
sudo apt install build-essential pkg-config libicu-dev libssl-dev libfontconfig1-dev libx11-dev libxcb1-dev libwayland-dev libxkbcommon-dev
```

Fedora:

```sh
sudo dnf install gcc gcc-c++ make pkgconf-pkg-config libicu-devel openssl-devel fontconfig-devel libX11-devel libxcb-devel wayland-devel libxkbcommon-devel
```

Arch:

```sh
sudo pacman -S base-devel pkgconf clang icu openssl fontconfig libx11 libxcb wayland libxkbcommon
```

Template `.cargo/config.toml`. You may need to adjust `PKG_CONFIG_PATH`.

```toml
[env]
MONOCURL_ASSETS_DIR = { value = "assets", relative = true }
# you may have to adjust this
PKG_CONFIG_PATH = "/usr/bin/pkgconfig"
CXXFLAGS = "-std=c++17"
CFLAGS = "-Wno-int-conversion"
```

## Windows

There are two supported Windows setups. The developers so far have only used the MSYS2 way, but the MSVC way should work in theory.

- MSVC Rust toolchain with `vcpkg`
- MSYS2/UCRT64 Rust toolchain with MSYS2 packages and `pkgconf`


### MSYS2 with UCRT64

Install MSYS2, open the `UCRT64` shell, update packages, and install the UCRT64
toolchain and native dependencies:

```sh
pacman -Syu
pacman -S --needed \
  git base-devel \
  mingw-w64-ucrt-x86_64-toolchain \
  mingw-w64-ucrt-x86_64-rust \
  mingw-w64-ucrt-x86_64-pkgconf \
  mingw-w64-ucrt-x86_64-clang \
  mingw-w64-ucrt-x86_64-icu \
  mingw-w64-ucrt-x86_64-openssl \
  mingw-w64-ucrt-x86_64-fontconfig \
  mingw-w64-ucrt-x86_64-freetype \
  mingw-w64-ucrt-x86_64-graphite2 \
  mingw-w64-ucrt-x86_64-harfbuzz \
  mingw-w64-ucrt-x86_64-libpng
```

Template `.cargo/config.toml` for MSYS2/UCRT64:

```toml
[env]
MONOCURL_ASSETS_DIR = { value = "assets", relative = true }
PKG_CONFIG_PATH = "C:\\msys64\\ucrt64\\lib\\pkgconfig"
CFLAGS = "-Wno-int-conversion"
CXXFLAGS = "-std=c++17"
```

### MSVC with vcpkg

Install the Visual Studio Build Tools with the `Desktop development with C++`
workload, the Windows SDK, LLVM, Rust's MSVC toolchain, and `vcpkg`.

Install the native packages with `vcpkg`. The Tectonic bridge crates probe ICU,
fontconfig, freetype, graphite2, harfbuzz, and libpng through vcpkg; OpenSSL is
needed by the TLS stack.

```powershell
git clone https://github.com/microsoft/vcpkg C:\src\vcpkg
C:\src\vcpkg\bootstrap-vcpkg.bat
C:\src\vcpkg\vcpkg install `
    icu:x64-windows `
    openssl:x64-windows `
    fontconfig:x64-windows `
    freetype:x64-windows `
    graphite2:x64-windows `
    harfbuzz:x64-windows `
    libpng:x64-windows
```

Tectonic's build helper defaults to `pkg-config`, even on Windows. Tell it to
use vcpkg explicitly in `.cargo/config.toml`.

Template `.cargo/config.toml`:

```toml
[env]
MONOCURL_ASSETS_DIR = { value = "assets", relative = true }
VCPKG_ROOT = "C:\\src\\vcpkg"
VCPKGRS_TRIPLET = "x64-windows"
TECTONIC_DEP_BACKEND = "vcpkg"
CXXFLAGS = "/std:c++17"
```
