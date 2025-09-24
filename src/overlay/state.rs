use anyhow::{anyhow, Result};
use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use winit::{
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes},
};

use crate::overlay::drawing::draw_handle;
use crate::overlay::handles::{hit_test_handle, ResizeHandle};
use crate::overlay::toolbar::{compute_toolbar_rect, draw_toolbar};

// OverlayAction: 外部事件结果（当前仍只返回 None；按钮交互未来扩展）
pub enum OverlayAction {
    None,
    SelectionFinished(Vec<u8>),
    Canceled,
}

// OverlayMode: 内部状态机
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayMode {
    Idle,
    Dragging,
    MovingSelection,
    Resizing,
    IdleWithSelection,
    Annotating,
}

// OverlayState: 全屏覆盖层，基于预先截取的原始 RGBA 图像进行交互式选区
pub struct OverlayState {
    pub window: &'static Window,
    context: Context<&'static Window>,
    surface: Surface<&'static Window, &'static Window>,
    pub visible: bool,
    pub screenshot: Option<(u32, u32, Vec<u8>)>, // 原始 RGBA
    origin: (i32, i32),                          // 截图对应显示器原点
    dim_cache: Option<Vec<u32>>,                 // 变暗 BGRA 缓存
    bright_cache: Option<Vec<u32>>,              // 原亮度 BGRA 缓存
    drag_start: Option<(f64, f64)>,
    last_cursor: (f64, f64),
    pub selection: Option<(u32, u32, u32, u32)>, // x,y,w,h
    move_offset: Option<(i32, i32)>,
    mode: OverlayMode,
    resize_handle: Option<ResizeHandle>,
}

impl OverlayState {
    pub fn new(active: &ActiveEventLoop) -> Result<Self> {
        let size = active
            .available_monitors()
            .next()
            .map(|m| m.size())
            .unwrap_or(winit::dpi::PhysicalSize::new(800, 600));
        let attrs = WindowAttributes::default()
            .with_decorations(false)
            .with_resizable(false)
            .with_transparent(true)
            .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
            .with_visible(false)
            .with_title("Snip Overlay")
            .with_inner_size(size); // 物理像素避免 DPI 放大二次缩放
        let window = active.create_window(attrs)?;
        let window: &'static Window = Box::leak(Box::new(window));
        let context = Context::new(window).map_err(|e| anyhow!("overlay ctx: {e}"))?;
        let surface =
            Surface::new(&context, window).map_err(|e| anyhow!("overlay surface: {e}"))?;
        Ok(Self {
            window,
            context,
            surface,
            visible: false,
            screenshot: None,
            origin: (0, 0),
            dim_cache: None,
            bright_cache: None,
            drag_start: None,
            last_cursor: (0.0, 0.0),
            selection: None,
            move_offset: None,
            mode: OverlayMode::Idle,
            resize_handle: None,
        })
    }

    pub fn show_with_image(
        &mut self,
        w: u32,
        h: u32,
        pixels: Vec<u8>,
        origin: (i32, i32),
    ) -> Result<()> {
        self.screenshot = Some((w, h, pixels));
        self.origin = origin;
        self.selection = None;
        self.drag_start = None;
        self.visible = true;
        self.mode = OverlayMode::Idle;
        self.window.set_visible(true);
        self.window
            .set_outer_position(winit::dpi::PhysicalPosition::new(origin.0, origin.1));
        self.build_caches();
        self.window.request_redraw();
        Ok(())
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.window.set_visible(false);
        self.selection = None;
        self.drag_start = None;
        self.dim_cache = None;
        self.bright_cache = None;
    }

    pub fn handle_event(&mut self, event: &WindowEvent) -> OverlayAction {
        if !self.visible {
            return OverlayAction::None;
        }
        match event {
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => match self.mode {
                    OverlayMode::Idle => {
                        self.drag_start = Some(self.last_cursor);
                        self.selection = None;
                        self.mode = OverlayMode::Dragging;
                        self.window.request_redraw();
                    }
                    OverlayMode::IdleWithSelection => {
                        if let Some((x, y, w, h)) = self.selection {
                            let (cx, cy) = (self.last_cursor.0 as i32, self.last_cursor.1 as i32);
                            if let Some(handle) = hit_test_handle(cx, cy, x, y, w, h) {
                                self.resize_handle = Some(handle);
                                self.mode = OverlayMode::Resizing;
                            } else if cx >= x as i32
                                && cy >= y as i32
                                && cx < (x + w) as i32
                                && cy < (y + h) as i32
                            {
                                self.move_offset = Some((cx - x as i32, cy - y as i32));
                                self.mode = OverlayMode::MovingSelection;
                            }
                        }
                    }
                    OverlayMode::Dragging
                    | OverlayMode::MovingSelection
                    | OverlayMode::Resizing
                    | OverlayMode::Annotating => {}
                },
                ElementState::Released => match self.mode {
                    OverlayMode::Dragging => {
                        self.drag_start = None;
                        if self.selection.is_some() {
                            self.mode = OverlayMode::IdleWithSelection;
                        } else {
                            self.mode = OverlayMode::Idle;
                        }
                    }
                    OverlayMode::MovingSelection => {
                        self.move_offset = None;
                        self.mode = OverlayMode::IdleWithSelection;
                    }
                    OverlayMode::Resizing => {
                        self.resize_handle = None;
                        self.mode = OverlayMode::IdleWithSelection;
                    }
                    _ => {}
                },
            },
            WindowEvent::CursorMoved { position, .. } => {
                self.last_cursor = (position.x, position.y);
                match self.mode {
                    OverlayMode::Dragging => {
                        if let Some((sx, sy)) = self.drag_start {
                            let x0 = sx.min(position.x);
                            let y0 = sy.min(position.y);
                            let w = (sx - position.x).abs();
                            let h = (sy - position.y).abs();
                            self.selection = Some((x0 as u32, y0 as u32, w as u32, h as u32));
                            self.window.request_redraw();
                        }
                    }
                    OverlayMode::MovingSelection => {
                        if let (Some((sw, sh, _)), Some((_x, _y, w, h)), Some((ox, oy))) =
                            (self.screenshot.as_ref(), self.selection, self.move_offset)
                        {
                            let cx = position.x as i32;
                            let cy = position.y as i32;
                            let mut new_x = cx - ox;
                            let mut new_y = cy - oy;
                            if new_x < 0 {
                                new_x = 0;
                            }
                            if new_y < 0 {
                                new_y = 0;
                            }
                            let max_x = (*sw as i32 - w as i32).max(0);
                            let max_y = (*sh as i32 - h as i32).max(0);
                            if new_x > max_x {
                                new_x = max_x;
                            }
                            if new_y > max_y {
                                new_y = max_y;
                            }
                            self.selection = Some((new_x as u32, new_y as u32, w, h));
                            self.window.request_redraw();
                        }
                    }
                    OverlayMode::Resizing => {
                        if let (Some((sw, sh, _)), Some((sx, sy, w, h)), Some(handle)) =
                            (self.screenshot.as_ref(), self.selection, self.resize_handle)
                        {
                            let cx = position.x as i32;
                            let cy = position.y as i32;
                            let mut x = sx as i32;
                            let mut y = sy as i32;
                            let mut rw = w as i32;
                            let mut rh = h as i32;
                            const MIN: i32 = 4;
                            match handle {
                                ResizeHandle::TopLeft => {
                                    let nx = cx.clamp(0, (sx + w) as i32 - MIN);
                                    let ny = cy.clamp(0, (sy + h) as i32 - MIN);
                                    rw = (x + rw - nx).max(MIN);
                                    rh = (y + rh - ny).max(MIN);
                                    x = nx;
                                    y = ny;
                                }
                                ResizeHandle::Top => {
                                    let ny = cy.clamp(0, (sy + h) as i32 - MIN);
                                    rh = (y + rh - ny).max(MIN);
                                    y = ny;
                                }
                                ResizeHandle::TopRight => {
                                    let ny = cy.clamp(0, (sy + h) as i32 - MIN);
                                    let nx2 = cx.clamp(sx as i32 + MIN, *sw as i32 - 1);
                                    rw = (nx2 - x + 1).max(MIN);
                                    rh = (y + rh - ny).max(MIN);
                                    y = ny;
                                }
                                ResizeHandle::Right => {
                                    let nx2 = cx.clamp(sx as i32 + MIN, *sw as i32 - 1);
                                    rw = (nx2 - x + 1).max(MIN);
                                }
                                ResizeHandle::BottomRight => {
                                    let nx2 = cx.clamp(sx as i32 + MIN, *sw as i32 - 1);
                                    let ny2 = cy.clamp(sy as i32 + MIN, *sh as i32 - 1);
                                    rw = (nx2 - x + 1).max(MIN);
                                    rh = (ny2 - y + 1).max(MIN);
                                }
                                ResizeHandle::Bottom => {
                                    let ny2 = cy.clamp(sy as i32 + MIN, *sh as i32 - 1);
                                    rh = (ny2 - y + 1).max(MIN);
                                }
                                ResizeHandle::BottomLeft => {
                                    let nx = cx.clamp(0, (sx + w) as i32 - MIN);
                                    let ny2 = cy.clamp(sy as i32 + MIN, *sh as i32 - 1);
                                    rw = (x + rw - nx).max(MIN);
                                    rh = (ny2 - y + 1).max(MIN);
                                    x = nx;
                                }
                                ResizeHandle::Left => {
                                    let nx = cx.clamp(0, (sx + w) as i32 - MIN);
                                    rw = (x + rw - nx).max(MIN);
                                    x = nx;
                                }
                            }
                            if x < 0 {
                                x = 0;
                            }
                            if y < 0 {
                                y = 0;
                            }
                            if x + rw > *sw as i32 {
                                rw = *sw as i32 - x;
                            }
                            if y + rh > *sh as i32 {
                                rh = *sh as i32 - y;
                            }
                            self.selection = Some((x as u32, y as u32, rw as u32, rh as u32));
                            self.window.request_redraw();
                        }
                    }
                    OverlayMode::IdleWithSelection => {
                        if let Some((x, y, w, h)) = self.selection {
                            let (cx, cy) = (position.x as i32, position.y as i32);
                            if let Some(handle) = hit_test_handle(cx, cy, x, y, w, h) {
                                use winit::window::CursorIcon::*;
                                let icon = match handle {
                                    ResizeHandle::Top | ResizeHandle::Bottom => NsResize,
                                    ResizeHandle::Left | ResizeHandle::Right => EwResize,
                                    ResizeHandle::TopLeft | ResizeHandle::BottomRight => NwseResize,
                                    ResizeHandle::TopRight | ResizeHandle::BottomLeft => NeswResize,
                                };
                                self.window.set_cursor(icon);
                            } else if cx >= x as i32
                                && cy >= y as i32
                                && cx < (x + w) as i32
                                && cy < (y + h) as i32
                            {
                                self.window.set_cursor(winit::window::CursorIcon::Move);
                            } else {
                                self.window.set_cursor(winit::window::CursorIcon::Default);
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        OverlayAction::None
    }

    pub fn redraw(&mut self) {
        if !self.visible {
            return;
        }
        if let Some((sw, sh, _)) = self.screenshot {
            let size = self.window.inner_size();
            let width = size.width.max(1);
            let height = size.height.max(1);
            let _ = self.surface.resize(
                NonZeroU32::new(width).unwrap(),
                NonZeroU32::new(height).unwrap(),
            );
            if let Ok(mut frame) = self.surface.buffer_mut() {
                if let Some(cache) = &self.dim_cache {
                    let copy_w = sw.min(width);
                    let copy_h = sh.min(height);
                    for y in 0..copy_h {
                        let dst_row = (y * width) as usize;
                        let src_row = (y * sw) as usize;
                        frame[dst_row..dst_row + copy_w as usize]
                            .copy_from_slice(&cache[src_row..src_row + copy_w as usize]);
                    }
                } else {
                    frame.fill(0x88000000);
                }
                if let Some((x, y, w, h)) = self.selection {
                    let x2 = (x + w).saturating_sub(1);
                    let y2 = (y + h).saturating_sub(1);
                    if w > 0 && h > 0 {
                        if matches!(
                            self.mode,
                            OverlayMode::Dragging
                                | OverlayMode::IdleWithSelection
                                | OverlayMode::MovingSelection
                                | OverlayMode::Resizing
                        ) {
                            if let (Some((_sw, _sh, _)), Some(bright)) =
                                (&self.screenshot, &self.bright_cache)
                            {
                                let copy_w = w.min(sw - x).min(width - x);
                                let copy_h = h.min(sh - y).min(height - y);
                                for row in 0..copy_h {
                                    let src_row = (y + row) * sw;
                                    let dst_row = (y + row) * width;
                                    let src_start = (src_row + x) as usize;
                                    let dst_start = (dst_row + x) as usize;
                                    let src_slice = &bright[src_start..src_start + copy_w as usize];
                                    let dst_slice =
                                        &mut frame[dst_start..dst_start + copy_w as usize];
                                    dst_slice.copy_from_slice(src_slice);
                                }
                            }
                        }
                        for i in x..=x2.min(width - 1) {
                            let top = (y.min(height - 1) * width + i) as usize;
                            frame[top] = 0xFFFFFFFF;
                            let bottom_y = y2.min(height - 1);
                            let bottom = (bottom_y * width + i) as usize;
                            frame[bottom] = 0xFFFFFFFF;
                        }
                        for j in y..=y2.min(height - 1) {
                            let left = (j * width + x.min(width - 1)) as usize;
                            frame[left] = 0xFFFFFFFF;
                            let right_x = x2.min(width - 1);
                            let right = (j * width + right_x) as usize;
                            frame[right] = 0xFFFFFFFF;
                        }
                        let handle_size: i32 = 6;
                        let hs2 = handle_size / 2;
                        let centers = [
                            (x as i32, y as i32),
                            ((x + w / 2) as i32, y as i32),
                            ((x + w) as i32 - 1, y as i32),
                            ((x + w) as i32 - 1, (y + h / 2) as i32),
                            ((x + w) as i32 - 1, (y + h) as i32 - 1),
                            ((x + w / 2) as i32, (y + h) as i32 - 1),
                            (x as i32, (y + h) as i32 - 1),
                            (x as i32, (y + h / 2) as i32),
                        ];
                        for (cx, cy) in centers {
                            draw_handle(&mut frame, width, height, cx, cy, hs2);
                        }
                        if matches!(self.mode, OverlayMode::IdleWithSelection) {
                            if let Some((bar_x, bar_y, bar_w, bar_h)) =
                                compute_toolbar_rect(x, y, w, h, sw, sh)
                            {
                                draw_toolbar(&mut frame, width, height, bar_x, bar_y, bar_w, bar_h);
                            }
                        }
                    }
                }
                let _ = frame.present();
            }
        }
    }

    pub fn take_selection_png(&self) -> Option<Vec<u8>> {
        use image::{ImageBuffer, Rgba};
        let (sw, sh, ref buf) = self.screenshot.as_ref()?;
        let (x, y, w, h) = self.selection?;
        if w == 0 || h == 0 {
            return None;
        }
        if x >= *sw || y >= *sh {
            return None;
        }
        let rw = w.min(sw - x);
        let rh = h.min(sh - y);
        let mut out: Vec<u8> = Vec::with_capacity((rw * rh * 4) as usize);
        for row in 0..rh {
            let start = (((y + row) * sw) + x) * 4;
            let end = start + rw * 4;
            out.extend_from_slice(&buf[start as usize..end as usize]);
        }
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_raw(rw, rh, out)?;
        let mut png_data = Vec::new();
        {
            use image::{codecs::png::PngEncoder, ExtendedColorType, ImageEncoder};
            let encoder = PngEncoder::new(&mut png_data);
            if encoder
                .write_image(img.as_raw(), rw, rh, ExtendedColorType::Rgba8)
                .is_err()
            {
                return None;
            }
        }
        Some(png_data)
    }

    fn build_caches(&mut self) {
        if let Some((w, h, ref buf)) = self.screenshot {
            let total = (w * h) as usize;
            let mut dim: Vec<u32> = Vec::with_capacity(total);
            let mut bright: Vec<u32> = Vec::with_capacity(total);
            for px in buf.chunks_exact(4) {
                let r = px[0];
                let g = px[1];
                let b = px[2];
                let a = px[3];
                let packed = u32::from_le_bytes([b, g, r, a]);
                bright.push(packed);
                dim.push(mix_dim(packed));
            }
            self.dim_cache = Some(dim);
            self.bright_cache = Some(bright);
        } else {
            self.dim_cache = None;
            self.bright_cache = None;
        }
    }
}

fn mix_dim(src: u32) -> u32 {
    let b = (src & 0xFF) as u8;
    let g = ((src >> 8) & 0xFF) as u8;
    let r = ((src >> 16) & 0xFF) as u8;
    let a = ((src >> 24) & 0xFF) as u8;
    let dr = ((r as f32) * 0.6) as u8;
    let dg = ((g as f32) * 0.6) as u8;
    let db = ((b as f32) * 0.6) as u8;
    u32::from_le_bytes([db, dg, dr, a])
}
