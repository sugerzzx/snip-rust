use anyhow::{anyhow, Result};
use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use winit::{
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes},
};

// OverlayState: 全屏覆盖窗口，显示已截图图像的变暗版本并允许拖拽矩形选区
pub struct OverlayState {
    pub window: &'static Window,
    context: Context<&'static Window>,
    surface: Surface<&'static Window, &'static Window>,
    pub visible: bool,
    pub screenshot: Option<(u32, u32, Vec<u8>)>, // 原始 RGBA 像素
    origin: (i32, i32),                          // 此截图对应显示器的原点（全局坐标）
    dim_cache: Option<Vec<u32>>,                 // 预计算的 BGRA+变暗 缓冲，提升重绘性能
    bright_cache: Option<Vec<u32>>,              // 原始亮度 BGRA 缓冲（行拷贝加速选区恢复）
    drag_start: Option<(f64, f64)>,
    last_cursor: (f64, f64),
    pub selection: Option<(u32, u32, u32, u32)>, // 选区：x,y,w,h（相对窗口左上）
    move_offset: Option<(i32, i32)>,             // 移动现有选区时，光标相对选区左上角偏移
    mode: OverlayMode,
    resize_handle: Option<ResizeHandle>, // 当前正在拖拽的缩放手柄
                                         // 工具栏无需复杂状态；只在 IdleWithSelection 显示，由 redraw 计算位置
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
            // 直接使用物理像素尺寸，避免 High-DPI 下再被 scale 放大
            .with_inner_size(size);
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

    // 显示 overlay 并载入截图（RGBA）及其全局原点，预计算变暗缓存
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
        self.mode = OverlayMode::Idle; // 等待第一次按下
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
                ElementState::Pressed => {
                    match self.mode {
                        OverlayMode::Idle => {
                            // 启动新建选区
                            self.drag_start = Some(self.last_cursor);
                            self.selection = None;
                            self.mode = OverlayMode::Dragging;
                            self.window.request_redraw();
                        }
                        OverlayMode::IdleWithSelection => {
                            if let Some((x, y, w, h)) = self.selection {
                                let (cx, cy) =
                                    (self.last_cursor.0 as i32, self.last_cursor.1 as i32);
                                // 先检测手柄
                                if let Some(handle) = hit_test_handle(cx, cy, x, y, w, h) {
                                    self.resize_handle = Some(handle);
                                    self.mode = OverlayMode::Resizing;
                                } else if cx >= x as i32
                                    && cy >= y as i32
                                    && cx < (x + w) as i32
                                    && cy < (y + h) as i32
                                {
                                    // 在选区内部 -> 进入移动模式
                                    self.move_offset = Some((cx - x as i32, cy - y as i32));
                                    self.mode = OverlayMode::MovingSelection;
                                } else {
                                    // Outside: 忽略
                                }
                            }
                        }
                        OverlayMode::Dragging
                        | OverlayMode::MovingSelection
                        | OverlayMode::Resizing
                        | OverlayMode::Annotating => {}
                    }
                }
                ElementState::Released => {
                    match self.mode {
                        OverlayMode::Dragging => {
                            // 结束新建选区
                            self.drag_start = None;
                            if self.selection.is_some() {
                                self.mode = OverlayMode::IdleWithSelection;
                            } else {
                                self.mode = OverlayMode::Idle;
                            }
                        }
                        OverlayMode::MovingSelection => {
                            // 完成移动
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
                            // Clamp within screenshot bounds
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
                            // Clamp最终
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
                        // 更新光标形状：在选区内部 -> Move, 否则 Default
                        if let Some((x, y, w, h)) = self.selection {
                            let (cx, cy) = (position.x as i32, position.y as i32);
                            if let Some(handle) = hit_test_handle(cx, cy, x, y, w, h) {
                                // 根据手柄类型换不同光标
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
                    // fallback: 填充半透明黑
                    frame.fill(0x88000000);
                }
                if let Some((x, y, w, h)) = self.selection {
                    // 边框（1px）
                    let x2 = (x + w).saturating_sub(1);
                    let y2 = (y + h).saturating_sub(1);
                    if w > 0 && h > 0 {
                        // 恢复选区内部为原始亮度（使用原始 screenshot）
                        if matches!(
                            self.mode,
                            OverlayMode::Dragging
                                | OverlayMode::IdleWithSelection
                                | OverlayMode::MovingSelection
                                | OverlayMode::Resizing
                        ) {
                            if let (Some((sw, sh, _)), Some(bright)) =
                                (&self.screenshot, &self.bright_cache)
                            {
                                let copy_w = w.min(sw - x).min(width - x);
                                let copy_h = h.min(sh - y).min(height - y);
                                for row in 0..copy_h {
                                    let src_row = (y + row) * *sw;
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
                        // 水平线
                        for i in x..=x2.min(width - 1) {
                            let top = (y.min(height - 1) * width + i) as usize;
                            frame[top] = 0xFFFFFFFF;
                            let bottom_y = y2.min(height - 1);
                            let bottom = (bottom_y * width + i) as usize;
                            frame[bottom] = 0xFFFFFFFF;
                        }
                        // 垂直线
                        for j in y..=y2.min(height - 1) {
                            let left = (j * width + x.min(width - 1)) as usize;
                            frame[left] = 0xFFFFFFFF;
                            let right_x = x2.min(width - 1);
                            let right = (j * width + right_x) as usize;
                            frame[right] = 0xFFFFFFFF;
                        }
                        // 绘制 8 个缩放手柄 (小方块 6x6)
                        let handle_size: i32 = 6;
                        let hs2 = handle_size / 2;
                        let centers = [
                            (x as i32, y as i32),                     // TL
                            ((x + w / 2) as i32, y as i32),           // T
                            ((x + w) as i32 - 1, y as i32),           // TR
                            ((x + w) as i32 - 1, (y + h / 2) as i32), // R
                            ((x + w) as i32 - 1, (y + h) as i32 - 1), // BR
                            ((x + w / 2) as i32, (y + h) as i32 - 1), // B
                            (x as i32, (y + h) as i32 - 1),           // BL
                            (x as i32, (y + h / 2) as i32),           // L
                        ];
                        for (cx, cy) in centers {
                            draw_handle(&mut frame, width, height, cx, cy, hs2);
                        }
                        // --- 工具栏绘制（仅在 IdleWithSelection 模式显示） ---
                        if matches!(self.mode, OverlayMode::IdleWithSelection) {
                            // 工具栏翻转逻辑应基于截图实际像素尺寸 (sw, sh)，而不是窗口当前物理缓冲尺寸（避免 DPI 放大导致判断失真）
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

pub enum OverlayAction {
    None,
    SelectionFinished(Vec<u8>),
    Canceled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OverlayMode {
    Idle,
    Dragging,
    MovingSelection,
    Resizing,
    IdleWithSelection,
    Annotating,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResizeHandle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

fn hit_test_handle(cx: i32, cy: i32, x: u32, y: u32, w: u32, h: u32) -> Option<ResizeHandle> {
    if w == 0 || h == 0 {
        return None;
    }
    let x = x as i32;
    let y = y as i32;
    let w = w as i32;
    let h = h as i32;
    let points = [
        (x, y, ResizeHandle::TopLeft),
        (x + w / 2, y, ResizeHandle::Top),
        (x + w - 1, y, ResizeHandle::TopRight),
        (x + w - 1, y + h / 2, ResizeHandle::Right),
        (x + w - 1, y + h - 1, ResizeHandle::BottomRight),
        (x + w / 2, y + h - 1, ResizeHandle::Bottom),
        (x, y + h - 1, ResizeHandle::BottomLeft),
        (x, y + h / 2, ResizeHandle::Left),
    ];
    const R: i32 = 5; // 命中半径
    for (px, py, id) in points {
        if (cx - px).abs() <= R && (cy - py).abs() <= R {
            return Some(id);
        }
    }
    None
}

fn draw_handle(frame: &mut [u32], width: u32, height: u32, cx: i32, cy: i32, half: i32) {
    let (w, h) = (width as i32, height as i32);
    for yy in (cy - half)..=(cy + half) {
        if yy < 0 || yy >= h {
            continue;
        }
        for xx in (cx - half)..=(cx + half) {
            if xx < 0 || xx >= w {
                continue;
            }
            let idx = (yy as u32 * width + xx as u32) as usize;
            frame[idx] = 0xFFFFFFFF;
        }
    }
}

// ------------------ 工具栏绘制 ------------------
const TB_BUTTONS: usize = 5; // Exit / Pin / Save / Copy / Mark(Annotate)
const TB_BTN_W: i32 = 48;
const TB_BTN_H: i32 = 26;
const TB_BTN_PAD_X: i32 = 6; // 左右内边距（整体边框与按钮之间）
const TB_BTN_GAP: i32 = 4; // 按钮之间水平间距
const TB_MARGIN: i32 = 6; // 与选区之间的垂直距离
const TB_OUTER_RAD: i32 = 0; // 圆角(暂不实现，可留为后续)

fn compute_toolbar_rect(
    sel_x: u32,
    sel_y: u32,
    sel_w: u32,
    sel_h: u32,
    screen_w: u32,
    screen_h: u32,
) -> Option<(i32, i32, i32, i32)> {
    if sel_w == 0 || sel_h == 0 {
        return None;
    }
    let total_w =
        TB_BTN_PAD_X * 2 + (TB_BUTTONS as i32) * TB_BTN_W + (TB_BUTTONS as i32 - 1) * TB_BTN_GAP;
    let total_h = TB_BTN_H + 2; // 上下边框各 1 像素
    let (screen_w_i, screen_h_i) = (screen_w as i32, screen_h as i32);
    if screen_w_i <= 0 || screen_h_i <= 0 {
        return None;
    }
    let sel_bottom = sel_y as i32 + sel_h as i32;
    let space_below = (screen_h_i - sel_bottom).max(0);
    let space_above = sel_y as i32;

    // 第一层：下方或上方（含 margin）正常放置
    if space_below >= total_h + TB_MARGIN {
        let bar_y = sel_bottom + TB_MARGIN;
        let mut bar_x = sel_x as i32 + (sel_w as i32 / 2) - total_w / 2;
        if bar_x < 0 {
            bar_x = 0;
        }
        let max_x = screen_w_i - total_w;
        if max_x < 0 {
            bar_x = 0;
        } else if bar_x > max_x {
            bar_x = max_x;
        }
        return Some((bar_x, bar_y, total_w, total_h));
    }
    if space_above >= total_h + TB_MARGIN {
        let bar_y = sel_y as i32 - TB_MARGIN - total_h;
        if bar_y >= 0 {
            // 防御
            let mut bar_x = sel_x as i32 + (sel_w as i32 / 2) - total_w / 2;
            if bar_x < 0 {
                bar_x = 0;
            }
            let max_x = screen_w_i - total_w;
            if max_x < 0 {
                bar_x = 0;
            } else if bar_x > max_x {
                bar_x = max_x;
            }
            return Some((bar_x, bar_y, total_w, total_h));
        }
    }

    // 第二层：上下都不足 margin，转为选区内嵌（右下角优先）
    let inset_pad = 4;
    let sel_w_i = sel_w as i32;
    let sel_h_i = sel_h as i32;
    // 强制右下角对齐，即使选区比工具栏窄/矮：向左/上溢出时裁剪
    let mut bar_x = sel_x as i32 + sel_w_i - total_w - inset_pad;
    let mut bar_y = sel_y as i32 + sel_h_i - total_h - inset_pad;
    // 如果选区太小导致 bar_x 左移超过选区左边也接受，但不要小于 0
    if bar_x < 0 {
        bar_x = 0;
    }
    if bar_y < 0 {
        bar_y = 0;
    }
    // 仍需保持不超出屏幕（极端：选区接近屏幕右下角时）
    let max_x = screen_w_i - total_w;
    let max_y = screen_h_i - total_h;
    if bar_x > max_x {
        bar_x = max_x.max(0);
    }
    if bar_y > max_y {
        bar_y = max_y.max(0);
    }
    Some((bar_x, bar_y, total_w, total_h))
}

fn draw_toolbar(frame: &mut [u32], width: u32, height: u32, x: i32, y: i32, w: i32, h: i32) {
    // 背景：半透明深色 (0xAA202020)
    fill_rect(frame, width, height, x, y, w, h, 0xAA202020);
    // 边框白色
    stroke_rect(frame, width, height, x, y, w, h, 0xFFFFFFFF);
    // 按钮区域
    let mut cursor_x = x + TB_BTN_PAD_X;
    let center_y = y + h / 2;
    let icon_color = 0xFFFFFFFF;
    for idx in 0..TB_BUTTONS {
        let bx = cursor_x;
        let by = center_y - TB_BTN_H / 2;
        draw_button(
            frame, width, height, bx, by, TB_BTN_W, TB_BTN_H, idx, icon_color,
        );
        cursor_x += TB_BTN_W + TB_BTN_GAP;
    }
}

fn fill_rect(
    frame: &mut [u32],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u32,
) {
    let (sw, sh) = (width as i32, height as i32);
    for yy in y.max(0)..(y + h).min(sh) {
        let row = yy as u32 * width;
        for xx in x.max(0)..(x + w).min(sw) {
            let idx = (row + xx as u32) as usize;
            frame[idx] = color;
        }
    }
}

fn stroke_rect(
    frame: &mut [u32],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u32,
) {
    if w <= 1 || h <= 1 {
        return;
    }
    let (sw, sh) = (width as i32, height as i32);
    // top & bottom
    for xx in x.max(0)..(x + w).min(sw) {
        if y >= 0 && y < sh {
            frame[(y as u32 * width + xx as u32) as usize] = color;
        }
        let by = y + h - 1;
        if by >= 0 && by < sh {
            frame[(by as u32 * width + xx as u32) as usize] = color;
        }
    }
    // left & right
    let right = x + w - 1;
    for yy in y.max(0)..(y + h).min(sh) {
        if x >= 0 && x < sw {
            frame[(yy as u32 * width + x as u32) as usize] = color;
        }
        if right >= 0 && right < sw {
            frame[(yy as u32 * width + right as u32) as usize] = color;
        }
    }
}

fn draw_button(
    frame: &mut [u32],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    index: usize,
    color: u32,
) {
    // 按钮背景（稍浅）
    fill_rect(frame, width, height, x, y, w, h, 0x66333333);
    stroke_rect(frame, width, height, x, y, w, h, 0xFFCCCCCC);
    // 绘制图标 (简易像素图)
    let icon_w = 12;
    let icon_h = 12;
    let ix = x + (w - icon_w) / 2;
    let iy = y + (h - icon_h) / 2;
    match index {
        0 => draw_icon_exit(frame, width, height, ix, iy, icon_w, icon_h, color),
        1 => draw_icon_pin(frame, width, height, ix, iy, icon_w, icon_h, color),
        2 => draw_icon_save(frame, width, height, ix, iy, icon_w, icon_h, color),
        3 => draw_icon_copy(frame, width, height, ix, iy, icon_w, icon_h, color),
        4 => draw_icon_annotate(frame, width, height, ix, iy, icon_w, icon_h, color),
        _ => {}
    }
}

fn set_px(frame: &mut [u32], width: u32, height: u32, x: i32, y: i32, color: u32) {
    if x < 0 || y < 0 {
        return;
    }
    let (sw, sh) = (width as i32, height as i32);
    if x >= sw || y >= sh {
        return;
    }
    frame[(y as u32 * width + x as u32) as usize] = color;
}

fn draw_icon_exit(
    frame: &mut [u32],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u32,
) {
    for i in 0..w {
        set_px(frame, width, height, x + i, y + i, color);
        set_px(frame, width, height, x + (w - 1 - i), y + i, color);
    }
}
fn draw_icon_pin(
    frame: &mut [u32],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u32,
) {
    // 顶部横线
    for xx in x..x + w {
        set_px(frame, width, height, xx, y, color);
    }
    // 中心竖线
    for yy in y..y + h {
        set_px(frame, width, height, x + w / 2, yy, color);
    }
    // 底部尖
    for i in 0..w / 2 {
        set_px(frame, width, height, x + w / 2 - i, y + h - 1 - i, color);
        set_px(frame, width, height, x + w / 2 + i, y + h - 1 - i, color);
    }
}
fn draw_icon_save(
    frame: &mut [u32],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u32,
) {
    // 外框
    for xx in x..x + w {
        set_px(frame, width, height, xx, y, color);
        set_px(frame, width, height, xx, y + h - 1, color);
    }
    for yy in y..y + h {
        set_px(frame, width, height, x, yy, color);
        set_px(frame, width, height, x + w - 1, yy, color);
    }
    // 顶部槽
    for xx in x + 2..x + w - 2 {
        set_px(frame, width, height, xx, y + 2, color);
    }
    // 底部块
    for yy in y + h / 2..y + h - 2 {
        set_px(frame, width, height, x + 2, yy, color);
        set_px(frame, width, height, x + w - 3, yy, color);
    }
}
fn draw_icon_copy(
    frame: &mut [u32],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u32,
) {
    // 后面的大框
    for xx in x + 2..x + w {
        set_px(frame, width, height, xx, y + 2, color);
        set_px(frame, width, height, xx, y + h - 1, color);
    }
    for yy in y + 2..y + h {
        set_px(frame, width, height, x + 2, yy, color);
        set_px(frame, width, height, x + w - 1, yy, color);
    }
    // 前面的框
    for xx in x..x + w - 2 {
        set_px(frame, width, height, xx, y, color);
        set_px(frame, width, height, xx, y + h - 3, color);
    }
    for yy in y..y + h - 2 {
        set_px(frame, width, height, x, yy, color);
        set_px(frame, width, height, x + w - 3, yy, color);
    }
}
fn draw_icon_annotate(
    frame: &mut [u32],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u32,
) {
    // 画一支“斜线铅笔” + 尾部方块
    let len = w.min(h);
    for i in 0..len {
        set_px(frame, width, height, x + i, y + h - 1 - i, color);
    }
    // 尾部块
    for yy in y + h - 4..y + h - 1 {
        for xx in x + 1..x + 4 {
            set_px(frame, width, height, xx, yy, color);
        }
    }
}

fn mix_dim(src: u32) -> u32 {
    // 简单 0.6 亮度
    let b = (src & 0xFF) as u8;
    let g = ((src >> 8) & 0xFF) as u8;
    let r = ((src >> 16) & 0xFF) as u8;
    let a = ((src >> 24) & 0xFF) as u8;
    let dr = ((r as f32) * 0.6) as u8;
    let dg = ((g as f32) * 0.6) as u8;
    let db = ((b as f32) * 0.6) as u8;
    u32::from_le_bytes([db, dg, dr, a])
}
