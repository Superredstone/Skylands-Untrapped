## Description
<!-- Summary of the changes and the related issue/feature -->

## Changes
- 

## How Has This Been Tested?
<!-- Describe the tests you ran to verify your changes. Include commands used. -->
- `nix-shell --run "cargo test --workspace"`
- `nix-shell --run "cargo run -p skylands-app -- --headless --ticks 120"`

## Checklist
- [ ] No copyrighted game assets or ROM files committed
- [ ] Any behavior change ships with a test that fails without the change
- [ ] Code formatted: `cargo fmt --all -- --check`
- [ ] Clippy passes with no warnings: `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] Tests pass: `cargo test --workspace`
- [ ] Maintenance matrix updated if dependencies changed
