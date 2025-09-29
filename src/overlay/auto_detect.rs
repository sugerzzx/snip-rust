use std::collections::HashSet;

use anyhow::{anyhow, Result};
use opencv::{
    core::{self, AlgorithmHint, Mat, Point, Rect, Scalar, Size, Vector},
    imgproc,
    prelude::*,
};

#[derive(Clone, Debug)]
pub struct DetectedRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub score: f32,
}

impl DetectedRect {
    pub fn new(x: i32, y: i32, width: i32, height: i32, score: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            score,
        }
    }

    #[inline]
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }

    #[inline]
    pub fn area(&self) -> i64 {
        (self.width as i64) * (self.height as i64)
    }

    pub fn composite_score(&self, cursor_x: i32, cursor_y: i32) -> f32 {
        let center_x = self.x as f32 + self.width as f32 * 0.5;
        let center_y = self.y as f32 + self.height as f32 * 0.5;
        let dx = center_x - cursor_x as f32;
        let dy = center_y - cursor_y as f32;
        let dist = (dx * dx + dy * dy).sqrt();
        let diag = ((self.width.pow(2) + self.height.pow(2)) as f32).sqrt() + 1.0;
        let dist_term = dist / diag; // 0..~1
        let area = (self.width.max(1) * self.height.max(1)) as f32;
        let area_term = area.ln().max(1.0); // 权重抑制超大矩形
        self.score * 3.0 - dist_term * 1.5 - area_term * 0.08
    }
}

pub fn detect_rectangles(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<DetectedRect>> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|v| v.checked_mul(4))
        .ok_or_else(|| anyhow!("rgba buffer too large"))?;
    if rgba.len() < expected {
        return Err(anyhow!(
            "rgba buffer too small: {} < {}",
            rgba.len(),
            expected
        ));
    }

    let rgba_mat = Mat::from_slice(rgba)?;
    let rgba_mat = rgba_mat.reshape(4, height as i32)?;

    let mut gray = Mat::default();
    imgproc::cvt_color(
        &rgba_mat,
        &mut gray,
        imgproc::COLOR_RGBA2GRAY,
        0,
        AlgorithmHint::ALGO_HINT_DEFAULT,
    )?;

    let mut blurred = Mat::default();
    imgproc::gaussian_blur(
        &gray,
        &mut blurred,
        Size::new(5, 5),
        0.0,
        0.0,
        core::BORDER_DEFAULT,
        AlgorithmHint::ALGO_HINT_DEFAULT,
    )?;

    let mut edges = Mat::default();
    imgproc::canny(&blurred, &mut edges, 32.0, 96.0, 3, false)?;

    let kernel =
        imgproc::get_structuring_element(imgproc::MORPH_RECT, Size::new(3, 3), Point::new(-1, -1))?;
    let mut dilated = Mat::default();
    imgproc::dilate(
        &edges,
        &mut dilated,
        &kernel,
        Point::new(-1, -1),
        1,
        core::BORDER_REPLICATE,
        Scalar::default(),
    )?;
    edges = dilated;

    let mut contours: Vector<Vector<Point>> = Vector::new();
    imgproc::find_contours(
        &edges,
        &mut contours,
        imgproc::RETR_LIST,
        imgproc::CHAIN_APPROX_SIMPLE,
        Point::new(0, 0),
    )?;

    let mut candidates = Vec::with_capacity(contours.len() as usize);
    let mut seen = HashSet::new();

    for i in 0..contours.len() {
        let contour: Vector<Point> = contours.get(i)?;
        if contour.len() < 4 {
            continue;
        }
        let area = imgproc::contour_area(&contour, false)?;
        if area < 1200.0 {
            continue;
        }
        let perimeter = imgproc::arc_length(&contour, true)?;
        if perimeter < 40.0 {
            continue;
        }
        let mut approx: Vector<Point> = Vector::new();
        imgproc::approx_poly_dp(&contour, &mut approx, 0.02 * perimeter, true)?;
        if approx.len() != 4 {
            continue;
        }
        if !imgproc::is_contour_convex(&approx)? {
            continue;
        }
        let rect: Rect = imgproc::bounding_rect(&approx)?;
        if rect.width <= 4 || rect.height <= 4 {
            continue;
        }
        if rect.x < 0 || rect.y < 0 {
            continue;
        }
        if rect.x + rect.width > width as i32 || rect.y + rect.height > height as i32 {
            continue;
        }
        let rect_area = (rect.width * rect.height) as f64;
        if rect_area <= 0.0 {
            continue;
        }
        let solidity = (area / rect_area).clamp(0.0, 1.0) as f32;
        if solidity < 0.55 {
            continue;
        }
        let aspect_ratio = rect.width as f32 / rect.height as f32;
        let max_ratio = aspect_ratio.max(1.0 / aspect_ratio);
        if max_ratio > 12.0 {
            continue;
        }
        let key = (rect.x, rect.y, rect.width, rect.height);
        if !seen.insert(key) {
            continue;
        }
        let aspect_penalty = (aspect_ratio - 1.0).abs().min(3.0);
        let score = (solidity - aspect_penalty * 0.05).max(0.0);
        candidates.push(DetectedRect::new(
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            score,
        ));
    }

    // 为兜底添加全屏矩形，确保任何位置均能匹配
    candidates.push(DetectedRect::new(0, 0, width as i32, height as i32, 0.05));

    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.area().cmp(&b.area()))
    });
    Ok(candidates)
}
