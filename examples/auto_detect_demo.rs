use anyhow::{anyhow, Context, Result};
use image::{ImageBuffer, Rgba};
use snip_rust::capture::capture_fullscreen_raw;
use snip_rust::overlay::auto_detect::{detect_rectangles, DetectedRect};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> Result<()> {
    env_logger::init();

    let (width, height, pixels) =
        capture_fullscreen_raw().context("failed to capture fullscreen RGBA buffer")?;
    let rectangles = detect_rectangles(width, height, &pixels).context("auto-detection failed")?;

    println!("Detected {} rectangles", rectangles.len());
    for rect in rectangles.iter().take(10) {
        println!(
            "Rect: x={}, y={}, w={}, h={}, score={:.3}",
            rect.x, rect.y, rect.width, rect.height, rect.score
        );
    }

    let mut image = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_vec(width, height, pixels)
        .ok_or_else(|| anyhow!("failed to create image buffer from capture"))?;

    let border_color = Rgba([255, 0, 0, 128]); // 半透明红色 (alpha=128)
    for rect in &rectangles {
        draw_border(&mut image, rect, border_color);
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let output_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("temp");
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create temp output directory at {:?}", output_dir))?;
    let output_path = output_dir.join(format!("snip_auto_detect_{timestamp}.png"));
    image
        .save(&output_path)
        .with_context(|| format!("failed to save annotated image to {:?}", output_path))?;

    println!("Saved annotated capture to {:?}", output_path);
    Ok(())
}

fn draw_border(img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, rect: &DetectedRect, color: Rgba<u8>) {
    if rect.width <= 0 || rect.height <= 0 {
        return;
    }

    let img_w = img.width() as i32;
    let img_h = img.height() as i32;
    if img_w == 0 || img_h == 0 {
        return;
    }

    let mut x0 = rect.x.max(0);
    let mut y0 = rect.y.max(0);
    if x0 >= img_w || y0 >= img_h {
        return;
    }

    let mut x1 = rect.x + rect.width - 1;
    let mut y1 = rect.y + rect.height - 1;
    if x1 < 0 || y1 < 0 {
        return;
    }

    x1 = x1.min(img_w - 1);
    y1 = y1.min(img_h - 1);
    x0 = x0.min(img_w - 1);
    y0 = y0.min(img_h - 1);
    if x0 > x1 || y0 > y1 {
        return;
    }

    for x in x0..=x1 {
        blend_pixel(img, x as u32, y0 as u32, color);
        blend_pixel(img, x as u32, y1 as u32, color);
    }
    for y in y0..=y1 {
        blend_pixel(img, x0 as u32, y as u32, color);
        blend_pixel(img, x1 as u32, y as u32, color);
    }
}

fn blend_pixel(img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, x: u32, y: u32, color: Rgba<u8>) {
    let bg = img.get_pixel(x, y);
    let alpha = color[3] as f32 / 255.0;
    let inv_alpha = 1.0 - alpha;

    let r = (color[0] as f32 * alpha + bg[0] as f32 * inv_alpha) as u8;
    let g = (color[1] as f32 * alpha + bg[1] as f32 * inv_alpha) as u8;
    let b = (color[2] as f32 * alpha + bg[2] as f32 * inv_alpha) as u8;
    let a = 255u8;

    img.put_pixel(x, y, Rgba([r, g, b, a]));
}
