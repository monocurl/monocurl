# Monocurl Binary and CLI

The `monocurl` executable is both the desktop application and the command-line
interface.

- `monocurl` with no arguments launches the GUI.
- `monocurl ...` with command-line arguments runs the CLI path from the same
  binary.

The MCP server is documentation-only. It does not validate, execute, seek, or
render scenes. Use the `monocurl` binary for those operations.

## Finding The Binary

If `monocurl` is on `PATH`, invoke it directly. Packaged installs commonly put
the GUI executable in one of these locations:

```sh
/Applications/Monocurl.app/Contents/MacOS/monocurl
/Applications/Monocurl.app/Contents/MacOS/Monocurl
```

On Windows, common packaged install locations are:

```text
C:\Program Files\Monocurl\monocurl.exe
C:\Program Files\Monocurl\Monocurl.exe
%LOCALAPPDATA%\Programs\Monocurl\monocurl.exe
```

Use the same executable for GUI and CLI. Launching it with no arguments opens
the desktop app; launching it with arguments runs the CLI command.

## General Usage

```sh
monocurl
monocurl help
monocurl help image
monocurl help video
monocurl help transcript
```

## Image Export

```sh
monocurl image <scene path> [options]
monocurl image lesson.mcs
monocurl image lesson.mcs --slide 2 --time 1.25 --resolution large
```

Options:

- `-o, --output <path>`: output path; extension is forced to `.png`.
- `-r, --resolution <preset>`: `small`, `medium`, or `large`.
- `--slide <index>`: zero-based visible slide index to capture.
- `--time <seconds>`: time within the selected slide.

If neither timestamp option is provided, image export captures the final frame.
If only one timestamp option is provided, the missing slide or time defaults to
`0`.

## Video Export

```sh
monocurl video <scene path> [options]
monocurl video lesson.mcs --resolution medium --fps 30
```

Options:

- `-o, --output <path>`: output path; extension is forced to `.mp4`.
- `-r, --resolution <preset>`: `small`, `medium`, or `large`.
- `--fps <number>`: frames per second.

## Transcript Inspection

```sh
monocurl transcript <scene path> [options]
monocurl transcript lesson.mcs
monocurl transcript lesson.mcs --slide 0 --time 0.5
```

The transcript command parses, compiles, seeks the scene, and prints only
Monocurl `print` output to stdout. Progress and runtime errors are written to
stderr.

Options:

- `--slide <index>`: zero-based visible slide index to seek.
- `--time <seconds>`: time within the selected slide.

If neither timestamp option is provided, transcript inspection seeks to the
scene end. If only one timestamp option is provided, the missing slide or time
defaults to `0`.
