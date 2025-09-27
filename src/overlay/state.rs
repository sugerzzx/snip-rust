use anyhow::{anyhow, Result};
use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use winit::{
    event::{ElementState, KeyEvent, MouseButton, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    platform::windows::WindowAttributesExtWindows,
    window::{
        CursorIcon::{self, *},
        Window, WindowAttributes,
    },
};

use crate::overlay::drawing::draw_handle;
use crate::overlay::handles::{hit_test_handle, ResizeHandle};
use crate::overlay::toolbar::{compute_toolbar_rect, draw_toolbar, hit_test_toolbar_button};

// OverlayAction: 外部事件结果（当前仍只返回 None；按钮交互未来扩展）
pub enum OverlayAction {
    None,
    Canceled,
    PasteSelection {
        png: Vec<u8>,
        width: u32,
        height: u32,
        screen_x: i32,
        screen_y: i32,
    },
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
    drag_start: Option<(f64, f64)>,
    last_cursor: (f64, f64),
    pub selection: Option<(u32, u32, u32, u32)>, // x,y,w,h
    move_offset: Option<(i32, i32)>,
    mode: OverlayMode,
    resize_handle: Option<ResizeHandle>,
    toolbar_rect: Option<(i32, i32, i32, i32)>, // 缓存当前工具栏矩形（屏幕内坐标）
    toolbar_hover: Option<usize>,               // 当前悬停按钮
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
            .with_inner_size(size) // 物理像素避免 DPI 放大二次缩放
            .with_skip_taskbar(true);
        let window = active.create_window(attrs)?;
        let window: &'static Window = Box::leak(Box::new(window));

        // 禁用窗口淡入淡出动画，提升显隐响应（Windows）
        crate::windows_util::disable_window_transitions(window);

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
            drag_start: None,
            last_cursor: (0.0, 0.0),
            selection: None,
            move_offset: None,
            mode: OverlayMode::Idle,
            resize_handle: None,
            toolbar_rect: None,
            toolbar_hover: None,
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
        self.window.focus_window();
        Ok(())
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.window.set_visible(false);
        // 释放截图与缓存，避免在取消后仍长期占用显著内存（全屏 RGBA + 缓存数十 MB）
        // 重新显示时会通过 show_with_image 重新构建
        self.screenshot = None;
        self.selection = None;
        self.drag_start = None;
        self.dim_cache = None;
        // 主动收缩可能的临时 Vec 容量（注意 allocator 可能仍保留，但可提示归还）
        // 由于我们把 Option<Vec<_>> 设为 None，这里暂无直接 shrink；若后续改为复用缓冲则可调用 shrink_to_fit。
    }

    pub fn handle_event(&mut self, event: &WindowEvent) -> OverlayAction {
        if !self.visible {
            return OverlayAction::None;
        }
        let mut immediate_action = OverlayAction::None;
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
                ElementState::Released => {
                    // 工具栏点击优先
                    if matches!(self.mode, OverlayMode::IdleWithSelection) {
                        if let Some((bx, by, bw, bh)) = self.toolbar_rect {
                            let cx = self.last_cursor.0 as i32;
                            let cy = self.last_cursor.1 as i32;
                            if let Some(btn) = hit_test_toolbar_button(cx, cy, bx, by, bw, bh) {
                                immediate_action = self.execute_toolbar_button(btn);
                            }
                        }
                    }
                    match self.mode {
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
                    }
                }
            },
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Right,
                ..
            } => match state {
                ElementState::Pressed => match self.mode {
                    OverlayMode::Idle => {
                        self.hide();
                    }
                    OverlayMode::IdleWithSelection => {
                        self.selection = None;
                        self.mode = OverlayMode::Idle;
                        self.window.set_cursor(CursorIcon::Crosshair);
                        self.window.request_redraw();
                    }
                    OverlayMode::Dragging
                    | OverlayMode::MovingSelection
                    | OverlayMode::Resizing
                    | OverlayMode::Annotating => {}
                },
                ElementState::Released => {}
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
                            // 1. 工具栏 hover 检测（若命中则直接使用 Pointer，不再继续后续手柄/区域判定）
                            let mut over_toolbar = false;
                            if let Some((bx, by, bw, bh)) = self.toolbar_rect {
                                if let Some(btn) = hit_test_toolbar_button(cx, cy, bx, by, bw, bh) {
                                    self.toolbar_hover = Some(btn);
                                    self.window.set_cursor(CursorIcon::Pointer);
                                    over_toolbar = true;
                                } else {
                                    self.toolbar_hover = None;
                                }
                            } else {
                                self.toolbar_hover = None;
                            }

                            if !over_toolbar {
                                // 2. 手柄与选区区域判定
                                if let Some(handle) = hit_test_handle(cx, cy, x, y, w, h) {
                                    let icon = match handle {
                                        ResizeHandle::Top | ResizeHandle::Bottom => NsResize,
                                        ResizeHandle::Left | ResizeHandle::Right => EwResize,
                                        ResizeHandle::TopLeft | ResizeHandle::BottomRight => {
                                            NwseResize
                                        }
                                        ResizeHandle::TopRight | ResizeHandle::BottomLeft => {
                                            NeswResize
                                        }
                                    };
                                    self.window.set_cursor(icon);
                                } else if cx >= x as i32
                                    && cy >= y as i32
                                    && cx < (x + w) as i32
                                    && cy < (y + h) as i32
                                {
                                    self.window.set_cursor(CursorIcon::Move);
                                } else {
                                    if self.toolbar_hover.is_none() {
                                        self.window.set_cursor(CursorIcon::Default);
                                    }
                                    // 已被 toolbar hover 设置，不处理
                                }
                            }
                        }
                    }
                    _ => {}
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
                self.hide();
            }
            _ => {}
        }
        // 返回可能的按钮动作（若未触发仍为 None）
        immediate_action
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
                            if let Some((sw, sh, buf)) = &self.screenshot {
                                let copy_w = w.min(*sw - x).min(width - x);
                                let copy_h = h.min(*sh - y).min(height - y);
                                // 按需转换选区 RGBA -> BGRA，避免存整幅亮度缓存占用额外内存
                                for row in 0..copy_h {
                                    let src_row_start = (((y + row) * *sw) + x) as usize * 4;
                                    let dst_row_start = ((y + row) * width + x) as usize;
                                    for col in 0..copy_w {
                                        let si = src_row_start + col as usize * 4;
                                        let r = buf[si];
                                        let g = buf[si + 1];
                                        let b = buf[si + 2];
                                        let a = buf[si + 3];
                                        frame[dst_row_start + col as usize] =
                                            u32::from_le_bytes([b, g, r, a]);
                                    }
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
                            self.toolbar_rect = compute_toolbar_rect(x, y, w, h, sw, sh);
                            if let Some((bar_x, bar_y, bar_w, bar_h)) = self.toolbar_rect {
                                draw_toolbar(
                                    &mut frame,
                                    width,
                                    height,
                                    bar_x,
                                    bar_y,
                                    bar_w,
                                    bar_h,
                                    self.toolbar_hover,
                                );
                            }
                        } else {
                            self.toolbar_rect = None;
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
            for px in buf.chunks_exact(4) {
                let r = px[0];
                let g = px[1];
                let b = px[2];
                let a = px[3];
                let packed = u32::from_le_bytes([b, g, r, a]);
                dim.push(mix_dim(packed));
            }
            self.dim_cache = Some(dim);
        } else {
            self.dim_cache = None;
        }
    }
}

impl OverlayState {
    fn execute_toolbar_button(&mut self, index: usize) -> OverlayAction {
        match index {
            0 => {
                // Exit
                self.hide();
                OverlayAction::Canceled
            }
            1 => {
                // Pin -> 生成贴图窗口，携带屏幕绝对坐标
                if let Some(png) = self.take_selection_png() {
                    if let Some((sx, sy, w, h)) = self.selection {
                        let screen_x = self.origin.0 + sx as i32;
                        let screen_y = self.origin.1 + sy as i32;
                        self.hide();
                        return OverlayAction::PasteSelection {
                            png,
                            width: w,
                            height: h,
                            screen_x,
                            screen_y,
                        };
                    }
                    self.hide();
                }
                OverlayAction::None
            }
            2 => {
                // Save to file (简单写入当前工作目录 snip_YYYYMMDD_HHMMSS.png)
                if let Some(png) = self.take_selection_png() {
                    if let Err(e) = save_png_auto(&png) {
                        eprintln!("save failed: {e}");
                    }
                }
                OverlayAction::None
            }
            3 => {
                // Copy (占位：暂未实现剪贴板集成)
                // TODO: 后续可引入 arboard / copypasta 以支持 RGBA + PNG
                OverlayAction::None
            }
            4 => {
                // Annotate 模式切换
                self.mode = OverlayMode::Annotating; // 目前仅状态标记
                OverlayAction::None
            }
            _ => OverlayAction::None,
        }
    }
}

fn save_png_auto(data: &[u8]) -> Result<()> {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let path = format!("snip_{ts}.png");
    fs::write(&path, data).map_err(|e| anyhow!("write png: {e}"))
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
