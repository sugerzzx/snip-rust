use std::collections::HashSet;

use anyhow::{anyhow, Result};
use opencv::{
    core::{self, AlgorithmHint, Mat, Point, Rect, Scalar, Size, Vector},
    imgproc,
    prelude::*,
};

#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicIsize, Ordering};

// 记录 Overlay 窗口的 HWND (仅 Windows)。用于系统窗口检测时跳过 Overlay 自身，避免被当成最上层遮挡导致其它窗口全部被过滤。
#[cfg(target_os = "windows")]
static OVERLAY_HWND: AtomicIsize = AtomicIsize::new(0);

#[cfg(target_os = "windows")]
pub fn register_overlay_hwnd(hwnd: isize) {
    OVERLAY_HWND.store(hwnd, Ordering::Relaxed);
}

#[cfg(target_os = "windows")]
use std::mem;
#[cfg(target_os = "windows")]
use windows::core::BOOL;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, GetAncestor, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect,
    IsIconic, IsWindowVisible, GA_ROOT, GWL_EXSTYLE, GWL_STYLE, SM_CXSCREEN, SM_CYSCREEN, WS_CHILD,
    WS_DISABLED, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
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

    #[inline]
    pub fn intersects(&self, other: &DetectedRect) -> bool {
        let x_overlap = self.x < other.x + other.width && self.x + self.width > other.x;
        let y_overlap = self.y < other.y + other.height && self.y + self.height > other.y;
        x_overlap && y_overlap
    }

    #[inline]
    pub fn contains_rect(&self, other: &DetectedRect) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.x + other.width <= self.x + self.width
            && other.y + other.height <= self.y + self.height
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

    let mut candidates = Vec::new();
    let mut seen = HashSet::new();

    // 系统窗口/控件枚举 (Win32) - 直接利用 OS 提供的窗口矩形
    #[cfg(target_os = "windows")]
    detect_by_system_windows(width, height, &mut candidates, &mut seen)?;

    //  多尺度边缘检测 - 检测窗口、对话框等明显边界
    detect_by_edges(&gray, width, height, &mut candidates, &mut seen)?;

    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.area().cmp(&b.area()))
    });

    log::debug!("Detected {} rectangles", candidates.len());

    // 打印面积最大的前5个矩形用于调试
    for rect in candidates.iter().rev().take(5) {
        log::debug!(
            "Rect: x={}, y={}, w={}, h={}, score={:.3}, area={}",
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            rect.score,
            rect.area()
        );
    }

    Ok(candidates)
}

// 策略 0 (Windows): 使用 EnumWindows / EnumChildWindows 直接枚举系统窗口与部分子控件
#[cfg(target_os = "windows")]
fn detect_by_system_windows(
    width: u32,
    height: u32,
    candidates: &mut Vec<DetectedRect>,
    seen: &mut HashSet<(i32, i32, i32, i32)>,
) -> Result<()> {
    #[derive(Clone, Copy)]
    struct SimpleRect {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    }

    fn subtract_once(base: SimpleRect, cover: SimpleRect) -> Vec<SimpleRect> {
        let bx2 = base.x + base.w;
        let by2 = base.y + base.h;
        let cx2 = cover.x + cover.w;
        let cy2 = cover.y + cover.h;
        if cover.x >= bx2 || cx2 <= base.x || cover.y >= by2 || cy2 <= base.y {
            return vec![base];
        }
        let ox1 = cover.x.max(base.x);
        let oy1 = cover.y.max(base.y);
        let ox2 = cx2.min(bx2);
        let oy2 = cy2.min(by2);
        // fully covered
        if ox1 <= base.x && oy1 <= base.y && ox2 >= bx2 && oy2 >= by2 {
            return vec![];
        }
        let mut out = Vec::with_capacity(4);
        // top
        if oy1 > base.y {
            out.push(SimpleRect {
                x: base.x,
                y: base.y,
                w: base.w,
                h: oy1 - base.y,
            });
        }
        // bottom
        if oy2 < by2 {
            out.push(SimpleRect {
                x: base.x,
                y: oy2,
                w: base.w,
                h: by2 - oy2,
            });
        }
        let mid_top = oy1.max(base.y);
        let mid_bottom = oy2.min(by2);
        if mid_bottom > mid_top {
            if ox1 > base.x {
                out.push(SimpleRect {
                    x: base.x,
                    y: mid_top,
                    w: ox1 - base.x,
                    h: mid_bottom - mid_top,
                });
            }
            if ox2 < bx2 {
                out.push(SimpleRect {
                    x: ox2,
                    y: mid_top,
                    w: bx2 - ox2,
                    h: mid_bottom - mid_top,
                });
            }
        }
        out.into_iter().filter(|r| r.w > 0 && r.h > 0).collect()
    }

    fn fully_covered(target: SimpleRect, covers: &[SimpleRect]) -> bool {
        let mut remaining = vec![target];
        for &c in covers {
            if remaining.is_empty() {
                return true;
            }
            let mut next = Vec::new();
            for r in remaining.drain(..) {
                next.extend(subtract_once(r, c));
            }
            remaining = next;
        }
        remaining.is_empty()
    }
    unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let vec_ptr = lparam.0 as *mut Vec<HWND>;
        if !vec_ptr.is_null() {
            (*vec_ptr).push(hwnd);
        }
        BOOL(1)
    }

    unsafe extern "system" fn enum_child_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let vec_ptr = lparam.0 as *mut Vec<HWND>;
        if !vec_ptr.is_null() {
            (*vec_ptr).push(hwnd);
        }
        BOOL(1)
    }

    let mut top_level: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(enum_windows_proc),
            LPARAM(&mut top_level as *mut _ as isize),
        );
    }

    let screen_area = (width as i64) * (height as i64);

    // 获取逻辑 DPI 缩放（通过窗口矩形与截图尺寸比值推断；若不一致则进行修正）
    // 这里简单使用系统主显示器逻辑尺寸 (GetSystemMetrics) 与捕获尺寸比较，估算 scaling。
    #[allow(unused_mut)]
    let mut scale_x = 1.0f32;
    #[allow(unused_mut)]
    let mut scale_y = 1.0f32;
    #[cfg(target_os = "windows")]
    unsafe {
        if std::env::var("SNIP_DETECT_FORCE_NO_SCALE").is_err() {
            // allow disabling via env
            let sys_w = GetSystemMetrics(SM_CXSCREEN) as i32;
            let sys_h = GetSystemMetrics(SM_CYSCREEN) as i32;
            if sys_w > 0 && sys_h > 0 && (sys_w != width as i32 || sys_h != height as i32) {
                scale_x = width as f32 / sys_w as f32;
                scale_y = height as f32 / sys_h as f32;
            }
        }
    }

    // 控制是否启用遮挡过滤：设置 SNIP_DETECT_DISABLE_OCCLUSION=1 可关闭（调试场景）
    let occlusion_enabled = std::env::var("SNIP_DETECT_DISABLE_OCCLUSION").is_err();
    // 已接受(前面)窗口集合，用于遮挡判断（按枚举顺序假设后枚举到的窗口在“上层”并保持原始 Z 序近似；若发现不准确可改为获取 Z 序专用 API）
    let mut visible_stack: Vec<SimpleRect> = Vec::new();

    // 先处理顶层窗口
    for &hwnd in &top_level {
        // 跳过 overlay 窗口本身（避免遮挡链条提前终止）
        let overlay_raw = OVERLAY_HWND.load(Ordering::Relaxed);
        if overlay_raw != 0 && hwnd.0 as isize == overlay_raw {
            continue;
        }
        // 可见 & 根窗口 & 未最小化
        unsafe {
            if IsWindowVisible(hwnd).as_bool() == false || IsIconic(hwnd).as_bool() == true {
                continue;
            }
            // 只保留真正的 root 窗口
            if GetAncestor(hwnd, GA_ROOT) != hwnd {
                continue;
            }

            let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;

            // 排除子窗口 / 工具窗口 (toolwindow 通常是浮动工具条,不需要过度检测)
            if style & WS_CHILD.0 as u32 != 0 {
                continue;
            }
            if ex_style & WS_EX_TOOLWINDOW.0 as u32 != 0 {
                continue;
            }
            if style & WS_DISABLED.0 as u32 != 0 {
                continue;
            }

            let mut rect: RECT = mem::zeroed();
            if GetWindowRect(hwnd, &mut rect).is_err() {
                continue;
            }

            // 使用扩展帧边界（排除阴影 / 外部透明留白），更贴近用户视觉窗口轮廓
            let mut ext: RECT = mem::zeroed();
            if DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut ext as *mut _ as *mut _,
                std::mem::size_of_val(&ext) as u32,
            )
            .is_ok()
            {
                // 仅在 ext 合理（宽高正 & 不比原始大很多）时替换
                let ew = ext.right - ext.left;
                let eh = ext.bottom - ext.top;
                let rw = rect.right - rect.left;
                let rh = rect.bottom - rect.top;
                if ew > 0 && eh > 0 && ew <= rw + 32 && eh <= rh + 32 {
                    // 允许少量差值
                    // 判断是否出现“过大”情况：如果原始比扩展大 > 6 像素（典型阴影/透明边）才替换
                    if rw - ew >= 6 || rh - eh >= 6 {
                        rect = ext;
                    }
                }
            }

            // 过滤 Cloaked (UWP 后台 / 隐藏) 窗口
            let mut cloaked: u32 = 0;
            let _ = DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                &mut cloaked as *mut _ as *mut _,
                std::mem::size_of_val(&cloaked) as u32,
            );
            if cloaked != 0 {
                continue;
            }

            // 应用 DPI 缩放修正（若检测到 scaling）
            let left = (rect.left as f32 * scale_x).round() as i32;
            let top = (rect.top as f32 * scale_y).round() as i32;
            let right = (rect.right as f32 * scale_x).round() as i32;
            let bottom = (rect.bottom as f32 * scale_y).round() as i32;

            let mut w = right - left;
            let mut h = bottom - top;
            // 如果窗口尺寸超出屏幕太多（>16 像素），说明缩放可能重复应用，尝试回退不缩放尺寸
            if (w > width as i32 + 16 || h > height as i32 + 16)
                && (scale_x != 1.0 || scale_y != 1.0)
            {
                let raw_w = rect.right - rect.left;
                let raw_h = rect.bottom - rect.top;
                if raw_w <= width as i32 && raw_h <= height as i32 {
                    w = raw_w;
                    h = raw_h;
                }
            }
            if w <= 20 || h <= 20 {
                continue;
            }

            // 将坐标裁剪到当前截屏区域 (假设截屏从(0,0)开始; 若未来支持多显示器需传入 origin 并做平移)
            if left >= width as i32 || top >= height as i32 {
                continue;
            }
            if right <= 0 || bottom <= 0 {
                continue;
            }

            let x = left.max(0);
            let y = top.max(0);
            let rw = (right.min(width as i32) - x).max(0);
            let rh = (bottom.min(height as i32) - y).max(0);
            if rw < 20 || rh < 20 {
                continue;
            }

            if std::env::var("SNIP_DETECT_LOG_SYSTEM").is_ok() {
                log::debug!(
                    "syswin hwnd={:?} rect=({},{} {}x{}) scale=({:.2},{:.2})",
                    hwnd,
                    x,
                    y,
                    rw,
                    rh,
                    scale_x,
                    scale_y
                );
            }

            // 遮挡过滤：若已完全被之前窗口覆盖则跳过
            if occlusion_enabled && fully_covered(SimpleRect { x, y, w: rw, h: rh }, &visible_stack)
            {
                continue;
            }

            let key = (x, y, rw, rh);
            if !seen.insert(key) {
                continue;
            }

            let area = (rw as i64) * (rh as i64);
            let area_factor = (area as f32 / screen_area as f32).clamp(0.0, 1.0);
            // 高权重,优先级略高于纯图像策略, 但避免绝对 1.0 以便后续可再调
            let score = 0.85 + area_factor * 0.10; // [0.85, 0.95]
            candidates.push(DetectedRect::new(x, y, rw, rh, score));

            // 不完全透明（非 layered）则作为遮挡源加入
            if occlusion_enabled && (ex_style & WS_EX_LAYERED.0 as u32) == 0 {
                visible_stack.push(SimpleRect { x, y, w: rw, h: rh });
            }

            // 枚举该窗口的子窗口 (经典 Win32 控件) 作为 UI 元素补充
            let mut child_hwnds: Vec<HWND> = Vec::new();
            let _ = EnumChildWindows(
                Some(hwnd),
                Some(enum_child_proc),
                LPARAM(&mut child_hwnds as *mut _ as isize),
            );
            for &ch in &child_hwnds {
                if ch == hwnd {
                    continue;
                }
                if IsWindowVisible(ch).as_bool() == false {
                    continue;
                }
                let mut cr: RECT = mem::zeroed();
                if GetWindowRect(ch, &mut cr).is_err() {
                    continue;
                }
                // DPI 修正
                let cleft = (cr.left as f32 * scale_x).round() as i32;
                let ctop = (cr.top as f32 * scale_y).round() as i32;
                let cright = (cr.right as f32 * scale_x).round() as i32;
                let cbottom = (cr.bottom as f32 * scale_y).round() as i32;
                let cw = cright - cleft;
                let chh = cbottom - ctop;
                if cw < 8 || chh < 8 {
                    continue;
                }
                if cleft >= width as i32 || ctop >= height as i32 {
                    continue;
                }
                if cright <= 0 || cbottom <= 0 {
                    continue;
                }
                let cx = cleft.max(0);
                let cy = ctop.max(0);
                let rcw = (cright.min(width as i32) - cx).max(0);
                let rch = (cbottom.min(height as i32) - cy).max(0);
                if rcw < 8 || rch < 8 {
                    continue;
                }
                let key2 = (cx, cy, rcw, rch);
                if !seen.insert(key2) {
                    continue;
                }
                // 子控件给一个中等权重分
                let score2 = 0.60;
                candidates.push(DetectedRect::new(cx, cy, rcw, rch, score2));
            }
        }
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn detect_by_system_windows(
    _width: u32,
    _height: u32,
    _candidates: &mut Vec<DetectedRect>,
    _seen: &mut HashSet<(i32, i32, i32, i32)>,
) -> Result<()> {
    Ok(())
}

//  基于边缘检测的矩形识别（检测窗口、对话框等）
fn detect_by_edges(
    gray: &Mat,
    width: u32,
    height: u32,
    candidates: &mut Vec<DetectedRect>,
    seen: &mut HashSet<(i32, i32, i32, i32)>,
) -> Result<()> {
    let mut blurred = Mat::default();
    imgproc::gaussian_blur(
        gray,
        &mut blurred,
        Size::new(9, 9),
        0.0,
        0.0,
        core::BORDER_REFLECT,
        AlgorithmHint::ALGO_HINT_DEFAULT,
    )?;

    let mut edges = Mat::default();
    imgproc::canny(&blurred, &mut edges, 50.0, 150.0, 3, false)?;

    // 轻微膨胀连接断裂的边缘
    let kernel =
        imgproc::get_structuring_element(imgproc::MORPH_RECT, Size::new(2, 2), Point::new(-1, -1))?;
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

    // 使用层次结构检测，保留嵌套关系
    let mut contours: Vector<Vector<Point>> = Vector::new();
    let mut hierarchy = Mat::default();
    imgproc::find_contours_with_hierarchy(
        &dilated,
        &mut contours,
        &mut hierarchy,
        imgproc::RETR_TREE,
        imgproc::CHAIN_APPROX_SIMPLE,
        Point::new(0, 0),
    )?;

    for i in 0..contours.len() {
        let contour: Vector<Point> = contours.get(i)?;
        if contour.len() < 4 {
            continue;
        }

        let area = imgproc::contour_area(&contour, false)?;
        // 降低最小面积以检测小元素（按钮、图标等）
        if area < 400.0 {
            continue;
        }

        let rect: Rect = imgproc::bounding_rect(&contour)?;
        if !is_valid_rect(&rect, width, height) {
            continue;
        }

        let perimeter = imgproc::arc_length(&contour, true)?;
        let rect_area = (rect.width * rect.height) as f64;
        if rect_area <= 0.0 {
            continue;
        }

        // 计算紧凑度（接近矩形的程度）
        let compactness = (4.0 * std::f64::consts::PI * area) / (perimeter * perimeter);
        let solidity = (area / rect_area).clamp(0.0, 1.0) as f32;

        // 放宽长宽比限制，允许任务栏等长条形元素
        let aspect_ratio = rect.width as f32 / rect.height as f32;
        let max_ratio = aspect_ratio.max(1.0 / aspect_ratio);
        if max_ratio > 20.0 {
            continue;
        }

        let key = (rect.x, rect.y, rect.width, rect.height);
        if !seen.insert(key) {
            continue;
        }

        // 综合评分：紧凑度、充实度、面积
        let size_factor = (area / (width as f64 * height as f64)).sqrt().min(1.0) as f32;
        let score = solidity * 0.6 + compactness as f32 * 0.3 + size_factor * 0.1;

        candidates.push(DetectedRect::new(
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            score,
        ));
    }

    Ok(())
}

// 辅助函数：验证矩形是否有效
fn is_valid_rect(rect: &Rect, width: u32, height: u32) -> bool {
    if rect.width <= 4 || rect.height <= 4 {
        return false;
    }
    if rect.x < 0 || rect.y < 0 {
        return false;
    }
    if rect.x + rect.width > width as i32 || rect.y + rect.height > height as i32 {
        return false;
    }
    true
}
