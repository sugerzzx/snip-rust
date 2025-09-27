#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
use anyhow::Result;
use env_logger;
use image::ImageReader;
use log::info;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, TrayIconBuilder,
};
use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::CursorIcon,
};

use snip_rust::capture::capture_fullscreen_raw_with_origin;
use snip_rust::hotkey::subscribe_f4;
use snip_rust::overlay::{OverlayAction, OverlayState};
use snip_rust::paste_window::PasteWindow;
mod single_instance;

#[allow(deprecated)]
fn main() -> Result<()> {
    // 单实例：若已存在实例则安静退出
    let _instance_guard = match single_instance::acquire_single_instance() {
        Some(g) => g,
        None => {
            println!("snip_rust: 已有实例在运行，退出");
            return Ok(());
        }
    };
    env_logger::init();
    info!("starting snip_rust (overlay + paste mode + tray)");
    let event_loop = EventLoop::new()?;

    // 从嵌入的 PNG 构建托盘图标（assets/app_icon.png）
    fn build_tray_icon() -> Icon {
        const BYTES: &[u8] = include_bytes!("../assets/app_icon.png");
        let reader = ImageReader::new(std::io::Cursor::new(BYTES))
            .with_guessed_format()
            .unwrap();
        let img = reader.decode().expect("decode icon").to_rgba8();
        let (w, h) = img.dimensions();
        Icon::from_rgba(img.into_raw(), w, h).expect("icon rgba")
    }

    // 托盘菜单（仅退出）
    let tray_menu = Menu::new();
    let quit_item = MenuItem::new("退出(&Q)", true, None);
    tray_menu.append(&quit_item).ok();
    let _tray = TrayIconBuilder::new()
        .with_tooltip("Snip Rust")
        .with_icon(build_tray_icon())
        .with_menu(Box::new(tray_menu))
        .build()
        .ok();
    // 仅需一个接收器（tray_icon::menu 与 muda::MenuEvent 实际共用同一全局通道）
    let menu_event_rx = MenuEvent::receiver();
    let mut paste_windows: Vec<PasteWindow> = Vec::new(); // 多 PasteWindow
    let mut hotkey_rx = subscribe_f4().ok();
    let mut overlay: Option<OverlayState> = None;
    let _ = event_loop.run(|event, elwt| match event {
        Event::AboutToWait => {
            while let Ok(ev) = menu_event_rx.try_recv() {
                // 1) 托盘退出
                if ev.id == quit_item.id() {
                    log::debug!("quit menu selected");
                    elwt.exit();
                    return;
                }
                // 2) 单次线性扫描：同时识别 copy / destroy（窗口数量一般很少，O(n) 足够）
                let mut remove_index: Option<usize> = None;
                for (i, pw) in paste_windows.iter().enumerate() {
                    if ev.id == pw.ctx_copy_id {
                        log::debug!("context copy placeholder triggered id={:?}", ev.id);
                        // TODO: 实现剪贴板复制
                        break; // 复制不需要继续找
                    }
                    if ev.id == pw.ctx_destroy_id {
                        remove_index = Some(i);
                        break;
                    }
                }
                if let Some(idx) = remove_index {
                    log::debug!(
                        "context destroy triggered id={:?} removing window #{}",
                        ev.id,
                        idx
                    );
                    let mut pw = paste_windows.remove(idx);
                    pw.destroy();
                }
            }
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
            // 回收 ESC 标记待销毁窗口（倒序遍历避免索引错位）
            for i in (0..paste_windows.len()).rev() {
                if paste_windows[i].is_pending_destroy() {
                    let mut pw = paste_windows.remove(i);
                    pw.destroy();
                }
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
