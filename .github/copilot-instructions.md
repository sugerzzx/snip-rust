# Copilot Instructions for `snip_rust`

These guidelines describe the CURRENT reality of the repository. Do not assume features that are only mentioned in older history or prior README versions.

## Project Overview
- Goal: Lightweight cross‑platform screenshot (snipping) utility (prototype stage).
- Current pipeline: global hotkey (F4) → fullscreen capture (single screen) → overlay dim layer + mouse drag selection → crop → optional Pin → multiple paste windows.
- Rendering is pure CPU (no GPU): `screenshots` capture + `tiny-skia` canvas + `softbuffer` present.

## Source Layout (Actual Files)
- `src/main.rs`: Event loop, overlay orchestration, tray icon (Quit), paste window management.
- `src/capture.rs`: Capture APIs (`fullscreen`, `raw`, `with_origin`, `area`).
- `src/renderer.rs`: Pixmap holder for future annotation pipeline.
- `src/paste_window.rs`: Pinned windows (pre-rendered dual border buffer, draggable, always-on-top).
- `src/hotkey.rs`: Global F4 registration + channel subscription.
- `src/overlay/`: Selection overlay modules (state, toolbar, handles, drawing) with dim cache.
- `build.rs`: Multi-size ICO (16..256) generation from `assets/app_icon.png` via `ico` + embedding with `winres`.
- `assets/app_icon.png`: Source PNG icon.
- `examples/capture_demo.rs`: Minimal capture example.
- `lib.rs`: Re-exports.

## Key Public / Semi-Public APIs
- Capture:
	- `capture_fullscreen() -> Result<Vec<u8>>`
	- `capture_fullscreen_raw() -> Result<(u32,u32,Vec<u8>)>`
	- `capture_fullscreen_raw_with_origin() -> Result<(i32,i32,u32,u32,Vec<u8>)>`
	- `capture_area(Rect) -> Result<Vec<u8>>`
- Renderer:
	- `Renderer::new(w,h)` / `load_png_bytes(&[u8])` / `as_bgra_u32()`
- Overlay:
	- `OverlayState::show_with_image(w,h,pixels, origin)`
	- `handle_event(&WindowEvent) -> OverlayAction`
	- `OverlayAction::{None, Canceled, SelectionFinished(Vec<u8>), PasteSelection { png, width, height, screen_x, screen_y }}`
- Paste:
	- `PasteWindow::new_from_png(event_loop, png_bytes, Some((screen_x, screen_y)))`

## Patterns & Conventions
- Lifetimes: Softbuffer surfaces require `'static`; current interim solution uses `Box::leak`. Do not refactor away unless replacing with a safe owner pattern across the event loop.
- Color channels: Internal processing keeps RGBA. Presentation to softbuffer expects BGRA ordering packed in `u32`. Conversion is explicit (`renderer.as_bgra_u32`). Do not silently reorder outside these helpers.
- Dim background: Overlay precomputes `dim_cache` once per capture; bright selection region uses original buffer.
- Paste windows: Pre-render focused/unfocused frame buffers (constant-time redraw during drag).
- Event loop: Using deprecated `EventLoop::run` (winit 0.30) intentionally. Do not migrate to `run_app` unless we re-architect handler pattern.
- Selection logic: Only update/redraw on cursor moved while dragging. Keep overlay responsive by minimizing allocations in that path.

## Capture Module Guidelines
- Add new capture outputs as separate functions; do NOT change existing return signatures without explicit approval.
- If introducing multi-monitor stitched capture, create new API (e.g., `capture_virtual_desktop_raw`) rather than mutating current single-screen semantics.
- BGRA conversion only triggered by `SNIP_FORCE_BGRA`. Do not add heuristic detection.

## Error Handling
- Use `anyhow::{Result, anyhow!}` with concise, lower-case contextual messages. No `thiserror` unless a broad error taxonomy becomes necessary.

## Logging
- Initialize via `env_logger::init()` in `main` (already present). Use `debug!` for verbose pixel/math details only if diagnosing; keep default code quiet.

## Testing
- Colocate fast unit tests (see `capture.rs`). Avoid fragile GUI-dependent tests. If adding scenario tests that require a display, consider gating with env var (future: `SNIP_SKIP_RUNTIME_TEST`).

## Adding Functionality (Scoped Guidance)
- Hotkeys: Extend existing `subscribe_*` pattern returning a channel; keep registration centralized (avoid multiple managers/thread leaks).
- Overlay Enhancements: Add new visual effects (mask, interior highlight) by layering additional write passes in `redraw`; reuse cached buffers when possible.
- Annotations / Editing: Prefer operating on raw RGBA within `renderer.rs` or a new `annotate` module, only encoding to PNG at external boundaries.
- Avoid adding GUI frameworks (egui/wgpu/iced) unless the maintainer explicitly requests a UI layer.

## Example (Selection + Pin Flow)
```rust
let (ox, oy, w, h, rgba) = capture_fullscreen_raw_with_origin()?;
overlay.show_with_image(w, h, rgba, (ox, oy))?;
match overlay.handle_event(&event) {
	OverlayAction::PasteSelection { png, screen_x, screen_y, .. } => {
		PasteWindow::new_from_png(&event_loop, &png, Some((screen_x, screen_y)))?;
	}
	_ => {}
}
```

## Build & Run
- Dev: `cargo run` (console visible, logs)
- Release: `cargo build --release` (no console window, embedded icon)
- Example: `cargo run --example capture_demo`
- Debug logging (Windows CMD): `set RUST_LOG=debug && cargo run`

## Environment Variables
- `SNIP_FORCE_BGRA`: Forces BGRA→RGBA conversion on captured buffer (diagnostics / platform quirks).

## Style Guidelines
- Functions stay small & focused. Keep pixel math explicit (index derivations, row-major assumptions).
- Pre-size buffers via `Vec::with_capacity` where size is known.
- Avoid allocation inside high-frequency redraw loops; precompute caches.
- Public value types (e.g., `Rect`) derive `Debug, Clone, Copy`.

## PR / Change Guidance (For Agents)
1. Verify README + instructions reflect actual files before referencing a module.
2. When adding new public API (function, env var), update BOTH README and this file.
3. Keep overlay interaction contract stable (`OverlayAction`) unless explicitly asked to change UX.
4. Do not remove `capture_fullscreen()` even if unused—external code may rely on it in future.
5. If optimizing performance, measure (log at debug) before altering algorithms.

## Out-of-Scope (Without Explicit Request)
- Async runtime introduction
- GPU backend migration
- Full annotation suite beyond initial primitives (pending request)
- Settings persistence / advanced configuration UI
- Multi-monitor mosaic capture (will be added as new API variant)

## Future Roadmap (Do Not Preempt)
- Esc / right-click cancel in overlay
- Multi-monitor (current screen / stitched virtual desktop)
- Clipboard copy (PNG + RGBA)
- Annotation primitives (rectangle, arrow, text)
- Replace `Box::leak` with managed lifetime container
- Tray additions: quick capture, theme toggle
- Dark/Light icon variants

---
If something is ambiguous, ask rather than guessing. Keep changes incremental and observable.
