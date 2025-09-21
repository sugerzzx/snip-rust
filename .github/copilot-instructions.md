# Copilot Instructions for `snip_rust`

These instructions help AI coding agents work effectively in this repository. Focus on the concrete patterns that already exist; do not invent large new abstractions unless explicitly requested.

## Project Overview
- Goal: Cross‑platform screenshot (snipping) utility written in Rust. Current implemented core: fullscreen + rectangular area capture producing PNG bytes.
- Status: Early scaffolding. Many modules listed in `README.md` (renderer, window_manager, tray, settings, paste) are not yet present in `src/`; only `capture.rs`, `hotkey.rs`, `main.rs`, and example code are implemented.
- Primary external crates actually in use so far: `screenshots`, `image` (encoder only), `anyhow`, `env_logger`.

## Source Layout (Current Reality)
- `src/main.rs`: Initializes logging only. Keep `main` minimal; add orchestration logic here gradually.
- `src/capture.rs`: Core screenshot logic. Converts raw RGBA (optionally BGRA) into PNG bytes. Contains pure helpers and runtime tests.
- `src/hotkey.rs`: Stub for future global hotkey management (uses `anyhow::Result`).
- `examples/capture_demo.rs`: Example binary demonstrating saving fullscreen and area captures.

## Capture Module Patterns
- Public API surface: `capture_fullscreen() -> Result<Vec<u8>>`, `capture_area(Rect) -> Result<Vec<u8>>` returning PNG-encoded bytes (not raw pixels). Preserve this contract unless a deliberate API change is approved.
- Color channel handling: By default treat buffer as RGBA. If env var `SNIP_FORCE_BGRA` is set (any value), convert BGRA→RGBA via `bgra_to_rgba`. Do NOT auto-detect format in other ways.
- PNG encoding: Centralized in `encode_png`; if adding formats, add new helper (e.g. `encode_jpeg`) rather than branching `encode_png` heavily.
- Cropping logic in `capture_area`: Computes relative coordinates against the screen origin; uses safe saturation and bounds clamping. When modifying, keep memory layout assumptions (4 bytes per pixel, row-major) explicit.

## Error Handling
- Use `anyhow::{Result, anyhow!}` for fallible operations. Provide contextual messages mirroring existing style: lower-case start, concise (`"capture failed: {e}"`). Do not introduce `thiserror` unless broad consensus.

## Testing Conventions
- Unit tests colocated in the same file under `#[cfg(test)]` with small, fast checks. Runtime-dependent tests (like `test_fullscreen_runtime_capture`) presently perform a real capture; keep them lightweight and guarded only if they become flaky (consider env gate later, e.g. `SNIP_SKIP_RUNTIME_TEST`).

## Logging & Diagnostics
- `env_logger::init()` is called in `main`. Prefer `log::{debug,info,warn,error}` macros in new code. Avoid printing via `println!` outside example binaries.

## Adding New Functionality
- Future modules (renderer, window_manager, tray, settings, paste) listed in README are placeholders. Create files only when implementing actual logic; update README to reflect reality if architectural direction changes.
- For global hotkeys: extend `HotkeyManager` methods rather than creating free functions. Keep a thin abstraction that can later wrap platform-specific backends.
- For image post-processing or annotations: return raw RGBA buffers internally, only encoding at final boundary (reuse or extend `encode_png`).

## Example Usage Pattern
```rust
use capture::{capture_area, Rect};
let png_bytes = capture_area(Rect { x: 0, y: 0, width: 200, height: 150 })?;
// persist or send over channel
```

## Build & Run
- Standard build: `cargo build` / `cargo run`.
- Release build: `cargo build --release`.
- Example: run capture demo (ensure module path / example structure): `cargo run --bin capture_demo` if promoted to `examples/` binary, otherwise integrate via a proper `[[bin]]` entry. Currently the file lives under `src/examples/`; to expose it as a cargo example, move it to top-level `examples/`.
- Logging: `RUST_LOG=debug cargo run` (Windows cmd: `set RUST_LOG=debug && cargo run`).

## Platform Considerations
- Screen detection uses first screen containing the provided point (`Screen::from_point`). Multi-monitor cropping relies on coordinate translation via `display_info.x/y`. Preserve this approach; if expanding to multi-screen composites, add a new API instead of mutating existing semantics.
- Environment toggle `SNIP_FORCE_BGRA` is the only supported format switch; document any new env flags here when added.

## Style & Conventions
- Keep functions small and single-purpose (see `encode_png`, `maybe_convert_bgra`).
- Avoid premature async abstractions; remain synchronous until a clear blocking need (e.g. heavy IO) emerges.
- Prefer explicit allocation sizing (`Vec::with_capacity`) where predictable.
- Public structs use `#[derive(Debug, Clone, Copy)]` when value-semantic (e.g., `Rect`).

## PR / Change Guidance for Agents
1. Confirm no divergence between README planned modules and actual files before referencing them.
2. When introducing a new module, add a succinct comment header mirroring existing file comments (purpose summary in Chinese is acceptable—match repo bilingual style).
3. Maintain backward compatibility of existing public functions unless user explicitly asks for breaking change.
4. Keep instruction file (`.github/copilot-instructions.md`) updated when adding env vars, new public APIs, or altering capture pipeline.

## Out-of-Scope for Autonomous Changes
- Implementing full UI, tray, or hotkey backend without explicit request.
- Reorganizing crate into workspace or multi-crate layout.
- Adding heavy dependencies (e.g. GPU frameworks) without approval.

---
If anything here seems incomplete (e.g., you want conventions for future renderer code), ask the maintainer before extrapolating.
