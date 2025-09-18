use anyhow::{anyhow, Result};
use image::{codecs::png::PngEncoder, ColorType, ImageEncoder};
use screenshots::Screen;
use std::env;
use std::io::Cursor;

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// 全屏截图，返回 PNG 字节
pub fn capture_fullscreen() -> Result<Vec<u8>> {
    let screen = Screen::from_point(0, 0).map_err(|e| anyhow!("detect screen failed: {e}"))?;
    let img = screen
        .capture()
        .map_err(|e| anyhow!("capture failed: {e}"))?; // RgbaImage
    let raw = img.as_raw();
    // 经验：screenshots 0.8 在当前平台实际返回 RGBA，之前误当 BGRA 导致偏色。
    // 如果用户设置 SNIP_FORCE_BGRA=1 则执行 BGRA->RGBA 转换。
    let rgba = maybe_convert_bgra(raw, img.width(), img.height());
    encode_png(&rgba, img.width(), img.height())
}

/// 区域截图（跨屏时暂以包含左上角的屏幕为准）
pub fn capture_area(rect: Rect) -> Result<Vec<u8>> {
    let screen = Screen::from_point(rect.x, rect.y)
        .map_err(|e| anyhow!("find screen for point ({}, {}) failed: {e}", rect.x, rect.y))?;
    let img = screen
        .capture()
        .map_err(|e| anyhow!("capture failed: {e}"))?; // RgbaImage

    // 屏幕坐标原点
    let origin_x = screen.display_info.x;
    let origin_y = screen.display_info.y;
    let rel_x = (rect.x - origin_x).max(0) as u32;
    let rel_y = (rect.y - origin_y).max(0) as u32;
    let max_w = img.width().saturating_sub(rel_x);
    let max_h = img.height().saturating_sub(rel_y);
    let crop_w = rect.width.min(max_w);
    let crop_h = rect.height.min(max_h);

    let rgba_full = maybe_convert_bgra(img.as_raw(), img.width(), img.height());
    let mut cropped: Vec<u8> = Vec::with_capacity((crop_w * crop_h * 4) as usize);
    for row in 0..crop_h {
        let start = (((rel_y + row) * img.width()) + rel_x) as usize * 4;
        let end = start + crop_w as usize * 4;
        cropped.extend_from_slice(&rgba_full[start..end]);
    }
    encode_png(&cropped, crop_w, crop_h)
}

fn bgra_to_rgba(bgra: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(bgra.len());
    for chunk in bgra.chunks_exact(4) {
        if chunk.len() == 4 {
            out.push(chunk[2]); // R
            out.push(chunk[1]); // G
            out.push(chunk[0]); // B
            out.push(chunk[3]); // A
        }
    }
    if out.len() != (w * h * 4) as usize {
        out.resize((w * h * 4) as usize, 0);
    }
    out
}

fn maybe_convert_bgra(raw: &[u8], w: u32, h: u32) -> Vec<u8> {
    if env::var("SNIP_FORCE_BGRA").is_ok() {
        bgra_to_rgba(raw, w, h)
    } else {
        // 直接克隆为 RGBA
        raw.to_vec()
    }
}

fn encode_png(rgba: &[u8], w: u32, h: u32) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    let cursor = Cursor::new(&mut data);
    let encoder = PngEncoder::new(cursor);
    encoder.write_image(rgba, w, h, ColorType::Rgba8)?;
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bgra_to_rgba_conversion() {
        // 单像素 BGRA: Blue=10, Green=20, Red=30, Alpha=255 -> RGBA: 30,20,10,255
        let bgra = [10u8, 20u8, 30u8, 255u8];
        let rgba = bgra_to_rgba(&bgra, 1, 1);
        assert_eq!(rgba, vec![30, 20, 10, 255]);
    }

    #[test]
    fn test_encode_png_signature() {
        // 2x1 像素: 红, 绿
        let mut data = Vec::new();
        data.extend_from_slice(&[255, 0, 0, 255]);
        data.extend_from_slice(&[0, 255, 0, 255]);
        let png = encode_png(&data, 2, 1).unwrap();
        assert!(png.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]));
    }

    #[test]
    fn test_fullscreen_runtime_capture() {
        let png = capture_fullscreen().unwrap();
        assert!(png.len() > 100); // 粗略检查
    }
}
