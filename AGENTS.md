# AGENTS.md

## Environment & Commands
Host runs NixOS. Standard tools require `devenv` (configured in `devenv.nix` with dynamic graphics libs and Wiimms ISO tools, or automatic via `direnv`):
- Run GUI: `devenv shell -- cargo run -p skylands-app`
- Run Headless: `devenv shell -- cargo run -p skylands-app -- --headless --ticks 120`
- Build: `devenv shell -- cargo build`
- Test: `devenv shell -- cargo test`
- Single test: `devenv shell -- cargo test <test_name>`
- Format & Lint: `devenv shell -- cargo fmt --all && devenv shell -- cargo clippy --all-targets --all-features`
- ISO extract tool: `devenv shell -- wit extract <rom.wbfs> <dest>`
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

## Documentation
Engineering notes and reverse engineering documentation live in `agent/` and `tools/rea-bridge/`. No external documentation site is maintained.

## Maintenance Matrix
| Area | Also update |
|---|---|
| `crates/skylands-formats/` | `crates/skylands-render/` (mesh/texture conversion) and `crates/skylands-core/` (asset loading/collision) |
| `crates/skylands-portal/` (protocol/tags) | `crates/skylands-core/src/portal_gameplay.rs` and `crates/skylands-portal/src/mock.rs` |
| `crates/skylands-core/` (components/events/resources) | `crates/skylands-app/` and system dispatch in `crates/skylands-core/src/lib.rs` |
| `crates/skylands-render/` | `crates/skylands-app/src/main.rs` (render sync & Bevy plugins) |
| `packaging/flatpak/` | `packaging/flatpak/cargo-sources.json` and `.github/workflows/ci.yml` |

## Done means
- Format check passes: `devenv shell -- cargo fmt --all -- --check`
- Lint passes with no warnings: `devenv shell -- cargo clippy --workspace --all-targets -- -D warnings`
- Test suite passes: `devenv shell -- cargo test --workspace`
- Headless verification passes: `devenv shell -- cargo run -p skylands-app -- --headless --ticks 120`
- Any behavior change ships with a test that fails without the change.

## Never merges without a human

A person has to have **read this diff** before it lands. Telling an agent "merge it when you're done" is approving a goal, not this change — so it does not count for anything on this list. Everywhere else it counts fine, which is the point of having a list.

- Any commit containing copyrighted assets, extracted ROM data, or binary assets (`*.arc`, `*.bld`, `*.wbfs`, `*.iso`, audio/texture dumps)
- Anything touching USB HID packet protocols or raw hardware interaction in `crates/skylands-portal/`
- Anything introducing or modifying `unsafe` blocks or raw pointer dereferencing
- Anything modifying release or CI workflows in `.github/workflows/`
