// renderer: 使用 tiny-skia 在 CPU 上绘制图像并输出到 softbuffer
// 提供加载 PNG/JPEG 等（通过 image crate）并拷贝到 tiny-skia Pixmap，然后附加简单形状绘制

use anyhow::{anyhow, Result};
use image::GenericImageView;
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, Stroke, Transform};

pub struct Renderer {
    pub pixmap: Pixmap,
}

impl Renderer {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        let pixmap = Pixmap::new(width, height).ok_or_else(|| anyhow!("create pixmap failed"))?;
        Ok(Self { pixmap })
    }

    pub fn load_image_to_canvas(&mut self, path: &str) -> Result<()> {
        let img = image::open(path)?; // DynamicImage
        let (w, h) = img.dimensions();
        let target_w = self.pixmap.width().min(w);
        let target_h = self.pixmap.height().min(h);

        // 清空背景为浅灰
        self.pixmap.fill(Color::from_rgba8(245, 245, 245, 255));

        // 拷贝像素（假设 image crate 解码后是 RGBA8 或可转换）
        let rgba_img = img.to_rgba8();
        let raw = rgba_img.as_raw();
        for y in 0..target_h {
            let src_start = (y * w * 4) as usize;
            let dst_start = (y * self.pixmap.width() * 4) as usize;
            let bytes = (target_w * 4) as usize;
            let src_slice = &raw[src_start..src_start + bytes];
            let dst_slice = &mut self.pixmap.data_mut()[dst_start..dst_start + bytes];
            dst_slice.copy_from_slice(src_slice);
        }

        // 绘制一个红色矩形边框以示例 tiny-skia 矢量绘制
        let mut pb = PathBuilder::new();
        pb.move_to(10.0, 10.0);
        pb.line_to((target_w as f32 - 10.0).max(10.0), 10.0);
        pb.line_to(
            (target_w as f32 - 10.0).max(10.0),
            (target_h as f32 - 10.0).max(10.0),
        );
        pb.line_to(10.0, (target_h as f32 - 10.0).max(10.0));
        pb.close();
        let path = pb.finish().ok_or_else(|| anyhow!("path build failed"))?;
        let mut paint = Paint::default();
        paint.set_color(Color::from_rgba8(220, 20, 60, 255));
        let stroke = Stroke {
            width: 3.0,
            ..Stroke::default()
        };
        self.pixmap
            .stroke_path(&path, &paint, &stroke, Transform::identity(), None);

        Ok(())
    }

    pub fn as_u32_slice(&self) -> &[u32] {
        // softbuffer 需要 BGRA 预乘或直通？目前直接把 RGBA8 按小端视为 u32。
        // tiny-skia pixmap data 默认是 RGBA premultiplied. 这里直接转即可。
        bytemuck::cast_slice(self.pixmap.data())
    }

    pub fn as_bgra_u32(&self) -> Vec<u32> {
        // 如果平台需要 BGRA 顺序（Windows 常见），转换一次
        // 数据为预乘 RGBA, 这里只调换 R 与 B 通道
        if std::env::var("SNIP_ASSUME_BGRA").is_ok() {
            return bytemuck::cast_slice(self.pixmap.data()).to_vec();
        }
        let data = self.pixmap.data();
        let mut out: Vec<u32> =
            Vec::with_capacity(self.pixmap.width() as usize * self.pixmap.height() as usize);
        for px in data.chunks_exact(4) {
            // RGBA -> BGRA (little endian u32: lowest addr is B when we reorder manually)
            let b = px[2];
            let g = px[1];
            let r = px[0];
            let a = px[3];
            out.push(u32::from_le_bytes([b, g, r, a]));
        }
        out
    }

    pub fn load_png_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        let img = image::load_from_memory(bytes)?; // DynamicImage
        let (w, h) = img.dimensions();
        let target_w = self.pixmap.width().min(w);
        let target_h = self.pixmap.height().min(h);
        self.pixmap.fill(Color::from_rgba8(34, 34, 34, 255));
        let rgba = img.to_rgba8();
        let raw = rgba.as_raw();
        for y in 0..target_h {
            let src_start = (y * w * 4) as usize;
            let dst_start = (y * self.pixmap.width() * 4) as usize;
            let bytes = (target_w * 4) as usize;
            let src_slice = &raw[src_start..src_start + bytes];
            let dst_slice = &mut self.pixmap.data_mut()[dst_start..dst_start + bytes];
            dst_slice.copy_from_slice(src_slice);
        }
        Ok(())
    }
}
