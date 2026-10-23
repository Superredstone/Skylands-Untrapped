# Character Model Selection & Rendering Fix - Implementation Plan

## Problem Statement
When selecting a character (via `--character`, `--viewer`, or mock portal), users observe the 3D model of a "random thing" instead of the expected character (e.g., giant flat 2D billboard cards, floating light spheres, ground rings, minigame musical instruments, or villain trap focus).

---

## Root Causes

1. **Geometry Aggregation in `igb_model_to_bevy_mesh` / `parse_igz`**:
   - `crates/skylands-formats/src/igb.rs:2847-2965`: `parse_igz` collects every `igWiiGeometry` in the `.bld` into `geometries` and creates a node for each.
   - Character `.bld` archives contain character submeshes alongside:
     - 2D upgrade cards / UI billboards (`z_depth < 0.1`, dimensions 30×35 units).
     - VFX domes / background geometry (e.g., Snap Shot `igWiiGeometry_8055` has dimensions 598×585 units).
     - Ground decal rings / selection indicators (e.g., Kaos ground rings of diameter 11.6 units).
     - Light/particle trigger boxes.
   - `crates/skylands-render/src/mesh_conversion.rs:759`: `igb_model_to_bevy_mesh` merges all submeshes and rescales the aggregate bounding box to `target_height` (1.8m). The massive outlier or flat billboard geometry squashes the true character body to near-zero scale (0.003 - 0.05 units), rendering the character unrecognizable.

2. **Minigame Archive Hijacking in `resolve_character_model_archive`**:
   - `crates/skylands-core/src/portal_gameplay.rs:141-155`: Directory scanning matches `name.starts_with(prefix1) || name.starts_with(prefix2)` with `.bld`.
   - For non-hardcoded character IDs (e.g., 1 Sonic Boom, 2 Warnado, 5 Terrafin, 7 Prism Break, 8 Sunburn, 9 Eruptor, 17 Chop Chop, 20 Drobot), directory order resolves `001_musicmini.bld`, `005_musicmini.bld`, etc., which are minigame musical instruments rather than character models.

3. **Mock Portal Slot 0 Trap Priority & Camera Hijack**:
   - `crates/skylands-app/src/main.rs:71-88`: Default mock portal inserts slot 0 (Kaos Trap 3010 / 1010) and slot 1 (Hero).
   - On startup, `portal_gameplay_system` triggers `GameState::VillainCapture`.
   - `crates/skylands-render/src/camera.rs:107-116`: In `VillainCapture`, camera hard-locks onto pedestal coordinates `(2.5, 0.8, 0.0)`, focusing on the villain trap rather than the selected character.

---

## Step-by-Step Implementation Tasks

### Phase 1: Fix Character Archive Resolution (`crates/skylands-core/src/portal_gameplay.rs`)
- [x] In `resolve_character_model_archive`:
  - Filter out auxiliary files: ignore filenames containing `musicmini`, `outro`, `stream`, `alt`, and `legendary` during standard resolution (unless explicitly requested).
  - Prefer the primary character archive name matching `{id}_{Name}.bld` or `{id:03}_{Name}.bld`.
  - Add missing canonical character mappings for standard Skylander IDs to prevent relying on raw directory order.

### Phase 2: Character Submesh Filtering in Render Conversion (`crates/skylands-render/src/mesh_conversion.rs`)
- [x] Enhance `igb_model_to_bevy_mesh`:
  - Compute individual submesh bounding box extents and dimensionality.
  - Filter out flat 2D billboard quads/cards: discard submeshes where one axis thickness is near zero (`< 0.05`) while other axes exceed 2.0 units, or where vertex count is trivial (`<= 4`) with large planar spans.
  - Filter out oversized outlier submeshes: ignore submeshes whose maximum dimension exceeds `15.0` units (e.g., 600-unit background domes, 50-unit VFX cards).
  - Filter out horizontal ground rings: discard submeshes where `y_span < 0.15` and `x_span > 3.0` (ground decals).
  - Retain true 3D character geometries and normalise based on the filtered character geometry aggregate.

### Phase 3: Viewer Mode & Startup Flow Adjustments (`crates/skylands-app/src/main.rs`)
- [x] In `--viewer` mode:
  - Configure mock portal or gameplay state initialization so viewer mode starts focused on the hero without triggering `GameState::VillainCapture`.
  - Keep camera centered on `(0.0, 0.8, 0.0)` targeting the showcase hero.

### Phase 4: Unit & Regression Tests
- [x] Add unit test in `crates/skylands-core` verifying `resolve_character_model_archive` resolves proper character archives and ignores `musicmini` for IDs 1, 2, 5, 7, 8, 9, 17, 20.
- [x] Add unit test in `crates/skylands-render` verifying `igb_model_to_bevy_mesh` strips oversized and 2D billboard geometries from Spyro (`016_Spyro.bld`), Snap Shot (`462_Water_Crocky.bld`), and Kaos (`1010_Kaos.bld`), returning a grounded mesh within expected human/creature proportions (height ~1.8m, realistic vertex count).
- [x] Update `agent/PROGRESS.md` with results and benchmarks.

---

## Verification Commands
- `devenv shell -- cargo fmt --all -- --check`
- `devenv shell -- cargo clippy --workspace --all-targets -- -D warnings`
- `devenv shell -- cargo test --workspace`
- `devenv shell -- cargo run -p skylands-app -- --headless --ticks 120`
- `devenv shell -- cargo run -p skylands-app -- --viewer --character spyro`
- `devenv shell -- cargo run -p skylands-app -- --viewer --character snapshot`
