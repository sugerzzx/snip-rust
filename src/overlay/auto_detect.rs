use std::collections::HashSet;

use anyhow::{anyhow, Result};
#[cfg(target_os = "windows")]
use core::ffi::c_void;
#[allow(unused_imports)]
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
fn with_overlay_click_through<F, R>(mut f: F) -> R
where
    F: FnMut() -> R,
{
    let overlay_raw = OVERLAY_HWND.load(Ordering::Relaxed);
    if overlay_raw == 0 {
        return f();
    }

    let overlay_handle = HWND(overlay_raw as *mut c_void);

    unsafe {
        const CLICK_THROUGH_ALPHA: u8 = 255;
        let original_style = GetWindowLongPtrW(overlay_handle, GWL_EXSTYLE);
        let transparent_bits = WS_EX_LAYERED.0 as isize | WS_EX_TRANSPARENT.0 as isize;

        if (original_style & transparent_bits) == transparent_bits {
            return f();
        }

        let overlay = overlay_handle;
        let restore_guard = guard(original_style, |style| {
            let _ = SetWindowLongPtrW(overlay, GWL_EXSTYLE, style);
            let _ =
                SetLayeredWindowAttributes(overlay, COLORREF(0), CLICK_THROUGH_ALPHA, LWA_ALPHA);
        });

        let _ = SetWindowLongPtrW(overlay, GWL_EXSTYLE, original_style | transparent_bits);
        let _ = SetLayeredWindowAttributes(overlay, COLORREF(0), CLICK_THROUGH_ALPHA, LWA_ALPHA);

        let result = f();
        drop(restore_guard);
        result
    }
}

#[cfg(target_os = "windows")]
use scopeguard::guard;
#[cfg(target_os = "windows")]
use std::mem;
#[cfg(target_os = "windows")]
use windows::core::BOOL;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, POINT, RECT, RPC_E_CHANGED_MODE};
#[cfg(target_os = "windows")]
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetAncestor, GetWindowLongPtrW, GetWindowRect, IsWindowVisible,
    SetLayeredWindowAttributes, SetWindowLongPtrW, WindowFromPoint, GA_ROOT, GWL_EXSTYLE,
    LWA_ALPHA, WS_EX_LAYERED, WS_EX_TRANSPARENT,
};

#[derive(Clone, Debug)]
pub struct DetectedRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl DetectedRect {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
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

#[cfg(target_os = "windows")]
#[derive(Default)]
pub struct AutoDetectManager {
    cache: Option<WindowCache>,
    automation: Option<IUIAutomation>,
    com_initialized: bool,
}

#[cfg(target_os = "windows")]
#[derive(Clone)]
struct WindowCache {
    hwnd: isize,
    rects: Vec<DetectedRect>,
}

#[cfg(target_os = "windows")]
impl AutoDetectManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn invalidate(&mut self) {
        self.cache = None;
    }

    pub fn detect_hover_rects(
        &mut self,
        width: u32,
        height: u32,
        origin: (i32, i32),
        cursor: (i32, i32),
    ) -> Result<Vec<DetectedRect>> {
        let screen_point = POINT {
            x: origin.0 + cursor.0,
            y: origin.1 + cursor.1,
        };

        with_overlay_click_through(|| {
            let hwnd_at_point = unsafe { WindowFromPoint(screen_point) };
            if hwnd_at_point.0.is_null() {
                log::debug!(
                    "auto_detect: WindowFromPoint returned null at ({}, {})",
                    screen_point.x,
                    screen_point.y
                );
                self.cache = None;
                return Ok(Vec::new());
            }

            let root = unsafe { GetAncestor(hwnd_at_point, GA_ROOT) };
            if root.0.is_null() {
                log::debug!(
                    "auto_detect: GetAncestor returned null for hwnd {:?}",
                    hwnd_at_point
                );
                self.cache = None;
                return Ok(Vec::new());
            }

            let overlay_raw = OVERLAY_HWND.load(Ordering::Relaxed);
            if overlay_raw != 0 && root.0 as isize == overlay_raw {
                log::debug!(
                    "auto_detect: hit overlay window ({:?}); skipping detection",
                    root
                );
                return Ok(Vec::new());
            }

            let need_rebuild = self
                .cache
                .as_ref()
                .map(|cache| cache.hwnd != root.0 as isize)
                .unwrap_or(true);

            if need_rebuild {
                let rects = collect_window_rects(root, width, height, origin)?;
                log::debug!(
                    "auto_detect: rebuilding cache for root {:?}, rects={}",
                    root,
                    rects.len()
                );
                self.cache = Some(WindowCache {
                    hwnd: root.0 as isize,
                    rects,
                });
            }

            let mut rects = self
                .cache
                .as_ref()
                .map(|cache| cache.rects.clone())
                .unwrap_or_default();

            if let Some(hover_rect) =
                self.collect_hover_rect(root, screen_point, origin, width, height)?
            {
                log::debug!(
                    "auto_detect: UIA hover rect x={} y={} w={} h={}",
                    hover_rect.x,
                    hover_rect.y,
                    hover_rect.width,
                    hover_rect.height
                );
                if !rects.iter().any(|r| {
                    r.x == hover_rect.x
                        && r.y == hover_rect.y
                        && r.width == hover_rect.width
                        && r.height == hover_rect.height
                }) {
                    rects.push(hover_rect);
                }
            }

            rects.sort_by(|a, b| {
                a.area()
                    .cmp(&b.area())
                    .then_with(|| a.y.cmp(&b.y))
                    .then_with(|| a.x.cmp(&b.x))
            });

            Ok(rects)
        })
    }

    fn collect_hover_rect(
        &mut self,
        root: HWND,
        point: POINT,
        origin: (i32, i32),
        width: u32,
        height: u32,
    ) -> Result<Option<DetectedRect>> {
        let automation = match self.ensure_automation()? {
            Some(automation) => automation,
            None => return Ok(None),
        };

        let element = unsafe { automation.ElementFromPoint(point)? };
        let rect = unsafe { element.CurrentBoundingRectangle()? };

        let (x, y, w, h) = match normalize_rect(
            rect.left - origin.0,
            rect.top - origin.1,
            rect.right - rect.left,
            rect.bottom - rect.top,
            width,
            height,
            4,
        ) {
            Some(r) => r,
            None => return Ok(None),
        };

        if !is_rect_within_root(root, origin, width, height, x, y, w, h)? {
            return Ok(None);
        }

        Ok(Some(DetectedRect::new(x, y, w, h)))
    }

    fn ensure_automation(&mut self) -> Result<Option<IUIAutomation>> {
        if self.automation.is_none() {
            let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            if hr == RPC_E_CHANGED_MODE {
                self.com_initialized = false;
            } else if hr.is_err() {
                return Err(anyhow!("CoInitializeEx failed: {hr:?}"));
            } else {
                self.com_initialized = true;
            }

            let automation: IUIAutomation =
                unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)? };
            self.automation = Some(automation);
        }

        Ok(self.automation.clone())
    }
}

#[cfg(target_os = "windows")]
impl Drop for AutoDetectManager {
    fn drop(&mut self) {
        if self.com_initialized {
            unsafe {
                CoUninitialize();
            }
            self.com_initialized = false;
        }
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn collect_enum_child_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let vec_ptr = lparam.0 as *mut Vec<HWND>;
    if let Some(vec) = vec_ptr.as_mut() {
        vec.push(hwnd);
    }
    BOOL(1)
}

#[cfg(target_os = "windows")]
fn collect_window_rects(
    hwnd: HWND,
    width: u32,
    height: u32,
    origin: (i32, i32),
) -> Result<Vec<DetectedRect>> {
    let mut rects = Vec::new();
    let mut seen = HashSet::new();

    unsafe {
        let mut rect: RECT = mem::zeroed();
        if GetWindowRect(hwnd, &mut rect).is_ok() {
            if let Some((x, y, w, h)) = normalize_rect(
                rect.left - origin.0,
                rect.top - origin.1,
                rect.right - rect.left,
                rect.bottom - rect.top,
                width,
                height,
                16,
            ) {
                rects.push(DetectedRect::new(x, y, w, h));
                seen.insert((x, y, w, h));
            }
        }

        let mut child_hwnds: Vec<HWND> = Vec::new();
        let _ = EnumChildWindows(
            Some(hwnd),
            Some(collect_enum_child_proc),
            LPARAM(&mut child_hwnds as *mut _ as isize),
        );

        for child in child_hwnds {
            if child == hwnd {
                continue;
            }
            if IsWindowVisible(child).as_bool() == false {
                continue;
            }
            let mut child_rect: RECT = mem::zeroed();
            if GetWindowRect(child, &mut child_rect).is_err() {
                continue;
            }
            if let Some((cx, cy, cw, ch)) = normalize_rect(
                child_rect.left - origin.0,
                child_rect.top - origin.1,
                child_rect.right - child_rect.left,
                child_rect.bottom - child_rect.top,
                width,
                height,
                8,
            ) {
                let key = (cx, cy, cw, ch);
                if seen.insert(key) {
                    rects.push(DetectedRect::new(cx, cy, cw, ch));
                }
            }
        }
    }

    rects.sort_by(|a, b| {
        a.area()
            .cmp(&b.area())
            .then_with(|| a.y.cmp(&b.y))
            .then_with(|| a.x.cmp(&b.x))
    });

    Ok(rects)
}

#[cfg(target_os = "windows")]
fn normalize_rect(
    mut x: i32,
    mut y: i32,
    mut w: i32,
    mut h: i32,
    width: u32,
    height: u32,
    min_size: i32,
) -> Option<(i32, i32, i32, i32)> {
    if w <= 0 || h <= 0 {
        return None;
    }

    let max_w = width as i32;
    let max_h = height as i32;
    if max_w <= 0 || max_h <= 0 {
        return None;
    }

    let mut right = x + w;
    let mut bottom = y + h;
    if x >= max_w || y >= max_h || right <= 0 || bottom <= 0 {
        return None;
    }

    if x < 0 {
        w -= -x;
        x = 0;
    }
    if y < 0 {
        h -= -y;
        y = 0;
    }

    right = x + w;
    bottom = y + h;
    if right > max_w {
        w = max_w - x;
    }
    if bottom > max_h {
        h = max_h - y;
    }

    if w < min_size || h < min_size {
        return None;
    }

    Some((x, y, w, h))
}

#[cfg(target_os = "windows")]
fn is_rect_within_root(
    root: HWND,
    origin: (i32, i32),
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> Result<bool> {
    unsafe {
        let mut rect: RECT = mem::zeroed();
        if GetWindowRect(root, &mut rect).is_err() {
            return Ok(true);
        }
        if let Some((rx, ry, rw, rh)) = normalize_rect(
            rect.left - origin.0,
            rect.top - origin.1,
            rect.right - rect.left,
            rect.bottom - rect.top,
            width,
            height,
            8,
        ) {
            let within = x >= rx.saturating_sub(2)
                && y >= ry.saturating_sub(2)
                && x + w <= rx + rw + 2
                && y + h <= ry + rh + 2;
            Ok(within)
        } else {
            Ok(true)
        }
    }
}

#[cfg(not(target_os = "windows"))]
#[derive(Default)]
pub struct AutoDetectManager;

#[cfg(not(target_os = "windows"))]
impl AutoDetectManager {
    pub fn new() -> Self {
        Self
    }

    pub fn invalidate(&mut self) {}

    pub fn detect_hover_rects(
        &mut self,
        _width: u32,
        _height: u32,
        _origin: (i32, i32),
        _cursor: (i32, i32),
    ) -> Result<Vec<DetectedRect>> {
        Ok(Vec::new())
    }
}
