# AGENTS.md

## Environment & Commands
Host runs NixOS. Standard tools require `nix-shell` (configured in `shell.nix` with dynamic graphics libs):
- Run GUI: `nix-shell --run "cargo run -p skylands-app"`
- Run Headless: `nix-shell --run "cargo run -p skylands-app -- --headless --ticks 120"`
- Build: `nix-shell --run "cargo build"`
- Test: `nix-shell --run "cargo test"`
- Single test: `nix-shell --run "cargo test <test_name>"`
- Format & Lint: `nix-shell --run "cargo fmt --all && cargo clippy --all-targets --all-features"`
- ISO extract tool: `nix-shell -p wiimms-iso-tools --run "wit extract <rom.wbfs> <dest>"`
- Ghidra headless: `/run/current-system/sw/bin/ghidra` or REA CLI (`npx rea-agents@6.7.0`)

## Constraints & Rules
- NO copyrighted game assets committed to repository. Assets must be extracted on first run from disc/wbfs image.
- `.gitignore` ignores all ROMs, extracted assets, build artifacts (`assets/`, `*.wbfs`, `*.iso`).
- Idiomatic, safe Rust for core logic. Unsafe limited to FFI / raw pointer interop where strictly required.
- Cross-platform support: Linux (`udev`/`hidapi-rs`) and Windows (`hidapi-winapi`).
- Always format code with `cargo fmt` and lint with `cargo clippy` at the end of a prompt.
- The game must be deterministic.
- Must implement everything that is described in README.md as objective.
- All the files for the project are in this folder, don't look outside.
- For extracting game files take ispiration from "https://github.com/NefariousTechSupport/igArchiveExtractor".
- igArchive format is described here "https://skylanderswiki.miraheze.org/wiki/IgArchive".
- Write down progress inside of markdown files in the agent/ folder.

## Architecture
- `./crates/skylands-formats`: Alchemy v8.1.0 file formats (`igArchive` `.arc`/`.bld`, `igIGBFile`, Big/Little Endian serialization).
- `./crates/skylands-portal`: USB HID Traptanium portal driver (`VID: 0x1430`, `PID: 0x0150`), tag protocol, color/audio commands, mock portal mode.
- `./crates/skylands-extractor`: ROM extraction (`rom.eu.wbfs`), asset unpacker, validation.
- `./crates/skylands-render`: Windowing (winit) and rendering (wgpu) pipeline, vertex buffers from IGB models.
- `./crates/skylands-core`: 60Hz fixed tick rate game loop, state machine, input translation (gilrs + keyboard), actor spawning, dual audio sink (system + Traptanium USB speaker).
- `./crates/skylands-app`: CLI & native runtime entry point. Runs first-time extraction before launch.
- `./tools/rea-bridge`: REA / Ghidra PowerPC decompiler harness for `sys/main.dol`.
- `./tools/igArchiveExtractor`: Archive extractor for Skylanders format.
