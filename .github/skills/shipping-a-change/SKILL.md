---
name: shipping-a-change
description: What to update when you change something in this repo, and what "done" requires. Use before opening a pull request.
---

# Shipping a change

## Add a new core game system or component
1. `crates/skylands-core/src/components.rs` or `crates/skylands-core/src/systems.rs` — implement component or system
2. `crates/skylands-core/src/lib.rs` — register system in App schedule
3. `crates/skylands-core/src/events.rs` or `resources.rs` — declare associated events or resources
4. `nix-shell --run "cargo test -p skylands-core"` — verify

## When you change this, also change that
| Change | Also update |
|---|---|
| `crates/skylands-formats/` | `crates/skylands-render/` (mesh/texture conversion) and `crates/skylands-core/` (asset loading/collision) |
| `crates/skylands-portal/` (protocol/tags) | `crates/skylands-core/src/portal_gameplay.rs` and `crates/skylands-portal/src/mock.rs` |
| `crates/skylands-core/` (components/events/resources) | `crates/skylands-app/` and system dispatch in `crates/skylands-core/src/lib.rs` |
| `crates/skylands-render/` | `crates/skylands-app/src/main.rs` (render sync & Bevy plugins) |
| `packaging/flatpak/` | `packaging/flatpak/cargo-sources.json` and `.github/workflows/ci.yml` |

## Done
- Format check passes: `nix-shell --run "cargo fmt --all -- --check"`
- Lint passes with no warnings: `nix-shell --run "cargo clippy --workspace --all-targets -- -D warnings"`
- Test suite passes: `nix-shell --run "cargo test --workspace"`
- Headless verification passes: `nix-shell --run "cargo run -p skylands-app -- --headless --ticks 120"`
- Any behavior change ships with a test that fails without the change.
