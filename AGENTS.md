# AGENTS.md

## Environment & Commands
Host runs NixOS. Standard tools require `nix-shell`:
- Build: `nix-shell -p cargo rustc pkg-config udev --run "cargo build"`
- Test: `nix-shell -p cargo rustc pkg-config udev --run "cargo test"`
- Single test: `nix-shell -p cargo rustc pkg-config udev --run "cargo test <test_name>"`
- ISO extract tool: `nix-shell -p wiimms-iso-tools --run "wit extract <rom.wbfs> <dest>"`
- Ghidra headless: `/run/current-system/sw/bin/ghidra` or REA CLI (`npx rea-agents@6.7.0`)

## Constraints & Rules
- NO copyrighted game assets committed to repository. Assets must be extracted on first run from disc/wbfs image.
- `.gitignore` ignores all ROMs, extracted assets, build artifacts (`assets/`, `*.wbfs`, `*.iso`).
- Idiomatic, safe Rust for core logic. Unsafe limited to FFI / raw pointer interop where strictly required.
- Cross-platform support: Linux (`udev`/`hidapi-rs`) and Windows (`hidapi-winapi`).

## Architecture
- `crates/skylands-formats`: Alchemy v8.1.0 file formats (`igArchive` `.arc`/`.bld`, `igIGBFile`, Big/Little Endian serialization).
- `crates/skylands-portal`: USB HID Traptanium portal driver (`VID: 0x1430`, `PID: 0x0150`), tag protocol, color/audio commands, mock portal mode.
- `crates/skylands-extractor`: ROM extraction (`rom.eu.wbfs`), asset unpacker, validation.
- `crates/skylands-app`: CLI & native runtime entry point. Runs first-time extraction before launch.
- `tools/rea-bridge`: REA / Ghidra PowerPC decompiler harness for `sys/main.dol`.
