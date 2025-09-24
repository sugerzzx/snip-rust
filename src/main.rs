use anyhow::Result;
use env_logger;
use log::info;
use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::CursorIcon,
};

use snip_rust::capture::capture_fullscreen_raw_with_origin;
use snip_rust::hotkey::subscribe_f4;
use snip_rust::overlay::{OverlayAction, OverlayState};
use snip_rust::paste_window::PasteWindow;

#[allow(deprecated)]
fn main() -> Result<()> {
    env_logger::init();
    info!("starting snip_rust (overlay + paste mode)");
    let event_loop = EventLoop::new()?;
    let mut paste_windows: Vec<PasteWindow> = Vec::new(); // 多 PasteWindow
    let mut hotkey_rx = subscribe_f4().ok();
    let mut overlay: Option<OverlayState> = None;
    let _ = event_loop.run(|event, elwt| match event {
        Event::AboutToWait => {
            // 轮询热键事件：进入 overlay 选区模式
            if let Some(rx) = &mut hotkey_rx {
                while let Ok(()) = rx.try_recv() {
                    // 若 overlay 已存在且当前可见，则忽略重复 F4，避免多实例 / 叠加创建
                    let already_visible = overlay.as_ref().map(|o| o.visible).unwrap_or(false);
                    if already_visible {
                        continue;
                    }
                    if overlay.is_none() {
                        if let Ok(ov) = OverlayState::new(elwt) {
                            overlay = Some(ov);
                        }
                    }
                    if let Some(ov) = &mut overlay {
                        if let Ok((ox, oy, w, h, raw)) = capture_fullscreen_raw_with_origin() {
                            if ov.show_with_image(w, h, raw, (ox, oy)).is_ok() {
                                ov.window.set_cursor(CursorIcon::Crosshair);
                            }
                        }
                    }
                }
            }
            if let Some(ov) = &overlay {
                if ov.visible {
                    ov.window.request_redraw();
                }
            }
            for pw in paste_windows.iter_mut() {
                let id = pw.window.id();
                pw.redraw(id);
            }
        }
        Event::WindowEvent {
            event: WindowEvent::RedrawRequested,
            window_id,
        } => {
            // overlay redraw
            if let Some(ov) = &mut overlay {
                if window_id == ov.window.id() {
                    ov.redraw();
                }
            }
            for pw in paste_windows.iter_mut() {
                pw.redraw(window_id);
            }
        }
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            window_id,
        } => {
            // 关闭 paste window
            let before = paste_windows.len();
            paste_windows.retain(|pw| pw.window.id() != window_id);
            if before != 0
                && paste_windows.is_empty()
                && overlay.as_ref().map(|o| !o.visible).unwrap_or(true)
            {
                // 若无窗口可见，可考虑退出（暂不自动 exit）
            }
            if let Some(ov) = &overlay {
                if window_id == ov.window.id() {
                    // 取消选区
                    // drop overlay keeps screenshot data ephemeral
                }
            }
        }
        Event::WindowEvent {
            event: WindowEvent::Resized(_),
            window_id,
        } => {
            if let Some(ov) = &overlay {
                if window_id == ov.window.id() {
                    ov.window.request_redraw();
                }
            }
            for pw in &mut paste_windows {
                if pw.window.id() == window_id {
                    pw.redraw(window_id);
                }
            }
        }
        Event::WindowEvent { event, window_id } => {
            if let Some(ov) = &mut overlay {
                if window_id == ov.window.id() {
                    match ov.handle_event(&event) {
                        OverlayAction::SelectionFinished(_png) => { /* 废弃预览逻辑 */ }
                        OverlayAction::Canceled => { /* overlay 已隐藏 不做处理 */ }
                        OverlayAction::PasteSelection {
                            png,
                            width: _w,
                            height: _h,
                            screen_x,
                            screen_y,
                        } => {
                            if let Ok(pw) =
                                PasteWindow::new_from_png(elwt, &png, Some((screen_x, screen_y)))
                            {
                                paste_windows.push(pw);
                            }
                        }
                        OverlayAction::None => {}
                    }
                }
            }
            for pw in &mut paste_windows {
                if pw.window.id() == window_id {
                    pw.handle_event(&event);
                }
            }
        }
        _ => {}
    });
    Ok(())
}
