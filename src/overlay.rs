use anyhow::{anyhow, Result};
use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use winit::{
    dpi::{LogicalSize, PhysicalPosition},
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
    origin: (i32, i32),                         // 此截图对应显示器的原点（全局坐标）
    dim_cache: Option<Vec<u32>>,                // 预计算的 BGRA+变暗 缓冲，提升重绘性能
    drag_start: Option<(f64, f64)>,
    last_cursor: (f64, f64),
    pub selection: Option<(u32, u32, u32, u32)>, // 选区：x,y,w,h（相对窗口左上）
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
            .with_inner_size(LogicalSize::new(size.width as f64, size.height as f64));
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
            drag_start: None,
            last_cursor: (0.0, 0.0),
            selection: None,
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
        self.window.set_visible(true);
        // 定位窗口，使其左上与显示器原点对齐
        self.window
            .set_outer_position(PhysicalPosition::new(origin.0, origin.1));
        self.build_dim_cache();
        self.window.request_redraw();
        Ok(())
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.window.set_visible(false);
        self.selection = None;
        self.drag_start = None;
        self.dim_cache = None;
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
                    self.drag_start = Some(self.last_cursor);
                    self.selection = None;
                }
                ElementState::Released => {
                    let png = self.take_selection_png();
                    if png.as_ref().map(|v| v.len()).unwrap_or(0) > 0 {
                        let out = png.unwrap();
                        self.hide();
                        return OverlayAction::SelectionFinished(out);
                    } else {
                        self.hide();
                        return OverlayAction::Canceled;
                    }
                }
            },
            WindowEvent::CursorMoved { position, .. } => {
                self.last_cursor = (position.x, position.y);
                if let Some((sx, sy)) = self.drag_start {
                    let x0 = sx.min(position.x);
                    let y0 = sy.min(position.y);
                    let w = (sx - position.x).abs();
                    let h = (sy - position.y).abs();
                    self.selection = Some((x0 as u32, y0 as u32, w as u32, h as u32));
                    self.window.request_redraw();
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
            let _ = self
                .surface
                .resize(NonZeroU32::new(width).unwrap(), NonZeroU32::new(height).unwrap());
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

    fn build_dim_cache(&mut self) {
        if let Some((w, h, ref buf)) = self.screenshot {
            let mut out: Vec<u32> = Vec::with_capacity((w * h) as usize);
            for px in buf.chunks_exact(4) {
                let r = px[0];
                let g = px[1];
                let b = px[2];
                let a = px[3];
                let packed = u32::from_le_bytes([b, g, r, a]);
                out.push(mix_dim(packed));
            }
            self.dim_cache = Some(out);
        } else {
            self.dim_cache = None;
        }
    }
}

pub enum OverlayAction {
    None,
    SelectionFinished(Vec<u8>),
    Canceled,
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
