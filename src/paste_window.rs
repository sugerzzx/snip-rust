use anyhow::{anyhow, Result};
use image::GenericImageView;
use softbuffer::{Context, Surface};
use winit::{
    dpi::PhysicalSize,
    event::{ElementState, KeyEvent, MouseButton, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    platform::windows::WindowAttributesExtWindows,
    window::{Window, WindowAttributes, WindowLevel},
};

// PasteWindow: 钉住的图片窗口（无边框 / 可拖动 / 置顶 / 预渲染边框提升性能）
pub struct PasteWindow {
    pub window: &'static Window,
    surface: Surface<&'static Window, &'static Window>,
    _context: Context<&'static Window>,
    pub width: u32,  // 原始图像宽
    pub height: u32, // 原始图像高
    #[allow(dead_code)]
    margin: u32, // 边框/阴影 margin（左右上下各 margin 像素）
    total_w: u32,    // 含 margin 的窗口像素宽
    total_h: u32,    // 含 margin 的窗口像素高
    // 拖动状态
    dragging: bool,
    drag_offset: (i32, i32),
    // 焦点状态
    focused: bool,
    // 原始图像像素（BGRA u32）
    #[allow(dead_code)]
    pixels: Vec<u32>,
    // 预渲染帧（含边框+图像）
    frame_focus: Vec<u32>,
    frame_unfocus: Vec<u32>,
    // 上一次窗口内光标位置（用于确定 press 时的拖动 offset）
    last_local_cursor: (f64, f64),
}

impl PasteWindow {
    pub fn new_from_png(
        active: &ActiveEventLoop,
        png_bytes: &[u8],
        desired_pos: Option<(i32, i32)>,
    ) -> Result<Self> {
        let img = image::load_from_memory(png_bytes)?;
        let (w, h) = img.dimensions();
        let margin: u32 = 2; // 外 1 像素暗线 + 内 1 像素彩色/灰线
        let total_w = w + margin * 2;
        let total_h = h + margin * 2;
        let mut pixels: Vec<u32> = Vec::with_capacity((w * h) as usize);
        let rgba = img.to_rgba8();
        for px in rgba.as_raw().chunks_exact(4) {
            // RGBA -> BGRA
            let b = px[2];
            let g = px[1];
            let r = px[0];
            let a = px[3];
            pixels.push(u32::from_le_bytes([b, g, r, a]));
        }
        // 使用物理像素尺寸（含 margin）
        let attrs = WindowAttributes::default()
            .with_title("Snip Paste")
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(false) // 先隐藏创建，避免“闪一下”或内容空白再填充的视觉差
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_inner_size(PhysicalSize::new(total_w, total_h))
            .with_skip_taskbar(true);
        let win = active.create_window(attrs)?;
        if let Some((x, y)) = desired_pos {
            // 目标位置应与选区左上对齐，窗口包含 margin 需向左上偏移 margin
            let px = x - margin as i32;
            let py = y - margin as i32;
            win.set_outer_position(winit::dpi::PhysicalPosition::new(px, py));
        }
        let win: &'static Window = Box::leak(Box::new(win));

        // 禁用淡入淡出动画确保显示/隐藏即时反馈（Windows 平台）
        crate::windows_util::disable_window_transitions(win);

        let context = Context::new(win).map_err(|e| anyhow!("paste ctx: {e}"))?;
        let mut surface = Surface::new(&context, win).map_err(|e| anyhow!("paste surface: {e}"))?;
        use std::num::NonZeroU32;
        surface
            .resize(
                NonZeroU32::new(total_w.max(1)).unwrap(),
                NonZeroU32::new(total_h.max(1)).unwrap(),
            )
            .map_err(|e| anyhow!("paste resize: {e}"))?;
        let (frame_focus, frame_unfocus) = build_frames(&pixels, w, h, margin);
        win.set_visible(true);
        Ok(Self {
            window: win,
            surface,
            _context: context,
            width: w,
            height: h,
            margin,
            total_w,
            total_h,
            dragging: false,
            drag_offset: (0, 0),
            focused: true,
            pixels,
            frame_focus,
            frame_unfocus,
            last_local_cursor: (0.0, 0.0),
        })
    }

    pub fn handle_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                // 记录窗口内局部坐标（逻辑像素）
                self.last_local_cursor = (position.x, position.y);
                if self.dragging {
                    if let Some((gx, gy)) = global_cursor_position() {
                        let x = gx - self.drag_offset.0;
                        let y = gy - self.drag_offset.1;
                        self.window
                            .set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                match state {
                    ElementState::Pressed => {
                        match button {
                            MouseButton::Left => {
                                self.dragging = true;
                                self.focused = true;
                                self.drag_offset = (
                                    self.last_local_cursor.0 as i32,
                                    self.last_local_cursor.1 as i32,
                                );
                            }
                            MouseButton::Right => {
                                // 右键隐藏（主循环会在 CloseRequested 中清理，这里直接 set_visible false 并发出请求）
                                self.window.set_visible(false);
                                self.window.request_redraw();
                            }
                            _ => {}
                        }
                    }
                    ElementState::Released => {
                        if *button == MouseButton::Left {
                            self.dragging = false;
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::Escape),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                self.window.set_visible(false);
                self.window.request_redraw();
            }
            WindowEvent::Focused(f) => {
                self.focused = *f;
            }
            _ => {}
        }
    }

    pub fn redraw(&mut self, window_id: winit::window::WindowId) {
        if window_id != self.window.id() {
            return;
        }

        let actual_size = self.window.inner_size();
        if actual_size.width != self.total_w || actual_size.height != self.total_h {
            // 尺寸不符则调整
            let _ = self
                .window
                .request_inner_size(PhysicalSize::new(self.total_w, self.total_h));
        }

        if let Ok(mut buf) = self.surface.buffer_mut() {
            let src = if self.focused {
                &self.frame_focus
            } else {
                &self.frame_unfocus
            };
            let need = (self.total_w * self.total_h) as usize;
            if buf.len() >= need && src.len() == need {
                buf[..need].copy_from_slice(src);
            }
            let _ = buf.present();
        }
    }
}

// 预构建含边框帧：外 1px 暗色 + 内 1px (聚焦高亮 / 非聚焦灰) + 原图像
fn build_frames(image: &[u32], w: u32, h: u32, margin: u32) -> (Vec<u32>, Vec<u32>) {
    let total_w = w + margin * 2;
    let total_h = h + margin * 2;
    let len = (total_w * total_h) as usize;
    let mut focus = vec![0xFF1E1E1E; len];
    let mut unfocus = focus.clone();
    // 拷贝图像
    for row in 0..h {
        let src_start = (row * w) as usize;
        let dst_base = ((row + margin) * total_w + margin) as usize;
        focus[dst_base..dst_base + w as usize]
            .copy_from_slice(&image[src_start..src_start + w as usize]);
        unfocus[dst_base..dst_base + w as usize]
            .copy_from_slice(&image[src_start..src_start + w as usize]);
    }
    let outer = 0xFF202020u32;
    let inner_focus = 0xFF3DA5F4u32;
    let inner_unfocus = 0xFF888888u32;
    let tw = total_w as usize;
    let th = total_h as usize;
    // 外圈
    for x in 0..tw {
        focus[x] = outer;
        unfocus[x] = outer;
        focus[(th - 1) * tw + x] = outer;
        unfocus[(th - 1) * tw + x] = outer;
    }
    for y in 0..th {
        let row = y * tw;
        focus[row] = outer;
        unfocus[row] = outer;
        focus[row + (tw - 1)] = outer;
        unfocus[row + (tw - 1)] = outer;
    }
    if margin >= 2 {
        let top = tw;
        let bottom = (th - 2) * tw;
        for x in 1..tw - 1 {
            focus[top + x] = inner_focus;
            focus[bottom + x] = inner_focus;
            unfocus[top + x] = inner_unfocus;
            unfocus[bottom + x] = inner_unfocus;
        }
        for y in 1..th - 1 {
            let row = y * tw;
            focus[row + 1] = inner_focus;
            focus[row + tw - 2] = inner_focus;
            unfocus[row + 1] = inner_unfocus;
            unfocus[row + tw - 2] = inner_unfocus;
        }
    }
    (focus, unfocus)
}

// 获取全局屏幕坐标（Windows 平台）。其他平台暂未实现。
#[cfg(target_os = "windows")]
fn global_cursor_position() -> Option<(i32, i32)> {
    use std::mem::MaybeUninit;
    #[repr(C)]
    struct POINT {
        x: i32,
        y: i32,
    }
    extern "system" {
        fn GetCursorPos(lpPoint: *mut POINT) -> i32;
    }
    let mut pt = MaybeUninit::<POINT>::uninit();
    let ok = unsafe { GetCursorPos(pt.as_mut_ptr()) };
    if ok != 0 {
        unsafe {
            let p = pt.assume_init();
            Some((p.x, p.y))
        }
    } else {
        None
    }
}

#[cfg(not(target_os = "windows"))]
fn global_cursor_position() -> Option<(i32, i32)> {
    None
}
