mod capture;
mod hotkey;
mod overlay;
mod renderer;
mod window_manager;

use anyhow::Result;
use env_logger;
use log::{error, info};
use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoop,
};

use crate::capture::capture_fullscreen_raw_with_origin;
use crate::hotkey::subscribe_f4;
use crate::overlay::{OverlayAction, OverlayState};
use crate::renderer::Renderer;
use crate::window_manager::WindowState;

#[allow(deprecated)]
fn main() -> Result<()> {
    env_logger::init();
    info!("starting snip_rust preview window (legacy run)");
    let event_loop = EventLoop::new()?;
    let mut maybe_state: Option<WindowState> = None;
    let mut maybe_renderer: Option<Renderer> = None;
    let mut hotkey_rx = subscribe_f4().ok();
    let mut overlay: Option<OverlayState> = None;
    let _ = event_loop.run(|event, elwt| match event {
        Event::AboutToWait => {
            if maybe_state.is_none() {
                if let Ok(st) = WindowState::new(elwt, 800, 600) {
                    maybe_state = Some(st);
                }
                if let Ok(r) = Renderer::new(800, 600) {
                    maybe_renderer = Some(r);
                }
                // 初始不加载固定图片，等待按 F4 截图
            }
            // 轮询热键事件：进入 overlay 选区模式
            if let Some(rx) = &mut hotkey_rx {
                while let Ok(()) = rx.try_recv() {
                    if overlay.is_none() {
                        if let Ok(ov) = OverlayState::new(elwt) {
                            overlay = Some(ov);
                        }
                    }
                    if let Some(ov) = &mut overlay {
                        if let Ok((ox, oy, w, h, raw)) = capture_fullscreen_raw_with_origin() {
                            let _ = ov.show_with_image(w, h, raw, (ox, oy));
                        }
                    }
                    if let Some(st) = &maybe_state {
                        st.window.set_visible(false);
                    }
                }
            }
            if let Some(st) = &maybe_state {
                st.window.request_redraw();
            }
            if let Some(ov) = &overlay {
                if ov.visible {
                    ov.window.request_redraw();
                }
            }
        }
        Event::WindowEvent {
            event: WindowEvent::RedrawRequested,
            window_id,
        } => {
            // 主窗口 redraw
            if let (Some(st), Some(r)) = (&mut maybe_state, &maybe_renderer) {
                if window_id == st.window.id() {
                    if let Err(e) = submit_pixels(st, r) {
                        error!("redraw failed: {e}");
                    }
                }
            }
            // overlay redraw
            if let Some(ov) = &mut overlay {
                if window_id == ov.window.id() {
                    ov.redraw();
                }
            }
        }
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            window_id,
        } => {
            if let Some(st) = &maybe_state {
                if window_id == st.window.id() {
                    elwt.exit();
                }
            }
            if let Some(ov) = &overlay {
                if window_id == ov.window.id() {
                    // 取消选区
                    if let Some(st) = &maybe_state {
                        st.window.set_visible(true);
                    }
                    // drop overlay keeps screenshot data ephemeral
                }
            }
        }
        Event::WindowEvent {
            event: WindowEvent::Resized(_),
            window_id,
        } => {
            if let Some(st) = &maybe_state {
                if window_id == st.window.id() {
                    st.window.request_redraw();
                }
            }
            if let Some(ov) = &overlay {
                if window_id == ov.window.id() {
                    ov.window.request_redraw();
                }
            }
        }
        Event::WindowEvent { event, window_id } => {
            if let Some(ov) = &mut overlay {
                if window_id == ov.window.id() {
                    match ov.handle_event(&event) {
                        OverlayAction::SelectionFinished(png) => {
                            if let (Some(r), Some(st)) = (&mut maybe_renderer, &maybe_state) {
                                let _ = r.load_png_bytes(&png);
                                st.window.set_visible(true);
                                st.window.request_redraw();
                            }
                        }
                        OverlayAction::Canceled => {
                            if let Some(st) = &maybe_state {
                                st.window.set_visible(true);
                            }
                        }
                        OverlayAction::None => {}
                    }
                }
            }
        }
        _ => {}
    });
    Ok(())
}

fn submit_pixels(state: &mut WindowState, renderer: &Renderer) -> Result<()> {
    let width = state.window.inner_size().width.max(1) as u32;
    let height = state.window.inner_size().height.max(1) as u32;
    let buffer_len = (width * height) as usize;
    // 确保 surface 尺寸匹配窗口
    use std::num::NonZeroU32;
    let w_nz = NonZeroU32::new(width).unwrap();
    let h_nz = NonZeroU32::new(height).unwrap();
    state
        .surface
        .resize(w_nz, h_nz)
        .map_err(|e| anyhow::anyhow!("surface resize: {e}"))?;
    // 如果窗口被缩放，简单缩放策略：当前直接居左上，不做重采样
    let mut surface_buffer = state
        .surface
        .buffer_mut()
        .map_err(|e| anyhow::anyhow!("surface buffer: {e}"))?;
    let converted = renderer.as_bgra_u32();
    let src = &converted[..];
    let copy_w = width.min(renderer.pixmap.width());
    let copy_h = height.min(renderer.pixmap.height());
    for y in 0..copy_h {
        let dst_start = (y * width) as usize;
        let src_start = (y * renderer.pixmap.width()) as usize;
        surface_buffer[dst_start..dst_start + copy_w as usize]
            .copy_from_slice(&src[src_start..src_start + copy_w as usize]);
    }
    // 填充剩余区域（如果窗口大于图片）为背景色
    if width * height > copy_w * copy_h {
        for pix in &mut surface_buffer[(copy_h * width) as usize..buffer_len] {
            *pix = 0xFFF0F0F0;
        }
    }
    surface_buffer
        .present()
        .map_err(|e| anyhow::anyhow!("present failed: {e}"))?;
    Ok(())
}
