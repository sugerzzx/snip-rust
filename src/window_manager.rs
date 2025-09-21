// 窗口与 softbuffer 初始化模块
// 负责创建 winit 窗口并建立 softbuffer Surface，供渲染器写入像素

use anyhow::{anyhow, Result};
use softbuffer::{Context, Surface};
use winit::{
    dpi::LogicalSize,
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes},
};

pub struct WindowState {
    pub window: &'static Window,
    pub surface: Surface<&'static Window, &'static Window>,
    _context: Context<&'static Window>,
}

impl WindowState {
    pub fn new(active: &ActiveEventLoop, width: u32, height: u32) -> Result<Self> {
        let attrs = WindowAttributes::default()
            .with_title("Snip Rust - 预览窗口")
            .with_inner_size(LogicalSize::new(width as f64, height as f64));
        let window = active.create_window(attrs)?;
        let window: &'static Window = Box::leak(Box::new(window));
        let context = Context::new(window).map_err(|e| anyhow!("context create failed: {e}"))?;
        let mut surface =
            Surface::new(&context, window).map_err(|e| anyhow!("surface create failed: {e}"))?;
        // 初始化 surface 尺寸（softbuffer 要先 resize 再获取缓冲）
        use std::num::NonZeroU32;
        let w = NonZeroU32::new(window.inner_size().width.max(1)).unwrap();
        let h = NonZeroU32::new(window.inner_size().height.max(1)).unwrap();
        surface
            .resize(w, h)
            .map_err(|e| anyhow!("surface resize failed: {e}"))?;
        Ok(Self {
            window,
            surface,
            _context: context,
        })
    }
}
