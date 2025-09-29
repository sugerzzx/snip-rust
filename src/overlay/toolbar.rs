use crate::overlay::drawing::{fill_rect, set_px, stroke_rect};

pub const TB_BUTTONS: usize = 5; // Exit / Pin / Save / Copy / Annotate
const TB_BTN_W: i32 = 48;
const TB_BTN_H: i32 = 26;
const TB_BTN_PAD_X: i32 = 6;
const TB_BTN_GAP: i32 = 4;
pub const TB_MARGIN: i32 = 6;
const INSET_PAD: i32 = 4;

pub fn compute_toolbar_rect(
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
    let total_h = TB_BTN_H + 2;
    let (sw, sh) = (screen_w as i32, screen_h as i32);
    if sw <= 0 || sh <= 0 {
        return None;
    }
    let sel_bottom = sel_y as i32 + sel_h as i32;
    let space_below = (sh - sel_bottom).max(0);
    let space_above = sel_y as i32;
    if space_below >= total_h + TB_MARGIN {
        let bar_y = sel_bottom + TB_MARGIN;
        let mut bar_x = sel_x as i32 + (sel_w as i32 / 2) - total_w / 2;
        if bar_x < 0 {
            bar_x = 0;
        }
        let max_x = sw - total_w;
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
            let mut bar_x = sel_x as i32 + (sel_w as i32 / 2) - total_w / 2;
            if bar_x < 0 {
                bar_x = 0;
            }
            let max_x = sw - total_w;
            if max_x < 0 {
                bar_x = 0;
            } else if bar_x > max_x {
                bar_x = max_x;
            }
            return Some((bar_x, bar_y, total_w, total_h));
        }
    }
    // embed bottom-right (clamped to screen)
    let sel_w_i = sel_w as i32;
    let sel_h_i = sel_h as i32;
    let mut bar_x = sel_x as i32 + sel_w_i - total_w - INSET_PAD;
    let mut bar_y = sel_y as i32 + sel_h_i - total_h - INSET_PAD;
    if bar_x < 0 {
        bar_x = 0;
    }
    if bar_y < 0 {
        bar_y = 0;
    }
    let max_x = sw - total_w;
    let max_y = sh - total_h;
    if bar_x > max_x {
        bar_x = max_x.max(0);
    }
    if bar_y > max_y {
        bar_y = max_y.max(0);
    }
    Some((bar_x, bar_y, total_w, total_h))
}

pub fn draw_toolbar(
    frame: &mut [u32],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    hovered: Option<usize>,
) {
    // 改为完全不透明背景，避免看到后方变暗像素导致“透视”感
    fill_rect(frame, width, height, x, y, w, h, 0xFF202020);
    stroke_rect(frame, width, height, x, y, w, h, 0xFFFFFFFF);
    let mut cursor_x = x + TB_BTN_PAD_X;
    let center_y = y + h / 2;
    let icon_color = 0xFFFFFFFF;
    for idx in 0..TB_BUTTONS {
        let bx = cursor_x;
        let by = center_y - TB_BTN_H / 2;
        draw_button(
            frame,
            width,
            height,
            bx,
            by,
            TB_BTN_W,
            TB_BTN_H,
            idx,
            icon_color,
            hovered == Some(idx),
        );
        cursor_x += TB_BTN_W + TB_BTN_GAP;
    }
}

// 根据屏幕坐标命中哪个按钮（0..TB_BUTTONS-1）
pub fn hit_test_toolbar_button(
    px: i32,
    py: i32,
    bar_x: i32,
    bar_y: i32,
    bar_w: i32,
    bar_h: i32,
) -> Option<usize> {
    if px < bar_x || py < bar_y || px >= bar_x + bar_w || py >= bar_y + bar_h {
        return None;
    }
    // 按钮水平排布：从 bar_x + TB_BTN_PAD_X 开始
    let mut cursor = bar_x + TB_BTN_PAD_X;
    for idx in 0..TB_BUTTONS {
        if px >= cursor && px < cursor + TB_BTN_W && py >= bar_y && py < bar_y + bar_h {
            return Some(idx);
        }
        cursor += TB_BTN_W + TB_BTN_GAP;
    }
    None
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
    base_icon_color: u32,
    hovered: bool,
) {
    let (bg, border, icon_color) = if hovered {
        (0xFF4A4A4A, 0xFFFFFFFF, 0xFFFFD24D)
    } else {
        (0xFF333333, 0xFFCCCCCC, base_icon_color)
    };
    fill_rect(frame, width, height, x, y, w, h, bg);
    stroke_rect(frame, width, height, x, y, w, h, border);
    let icon_w = 12;
    let icon_h = 12;
    let ix = x + (w - icon_w) / 2;
    let iy = y + (h - icon_h) / 2;
    match index {
        0 => icon_exit(frame, width, height, ix, iy, icon_w, icon_h, icon_color),
        1 => icon_pin(frame, width, height, ix, iy, icon_w, icon_h, icon_color),
        2 => icon_save(frame, width, height, ix, iy, icon_w, icon_h, icon_color),
        3 => icon_copy(frame, width, height, ix, iy, icon_w, icon_h, icon_color),
        4 => icon_annotate(frame, width, height, ix, iy, icon_w, icon_h, icon_color),
        _ => {}
    }
}

fn icon_exit(
    frame: &mut [u32],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: i32,
    _h: i32,
    color: u32,
) {
    for i in 0..w {
        set_px(frame, width, height, x + i, y + i, color);
        set_px(frame, width, height, x + (w - 1 - i), y + i, color);
    }
}
fn icon_pin(
    frame: &mut [u32],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u32,
) {
    for xx in x..x + w {
        set_px(frame, width, height, xx, y, color);
    }
    for yy in y..y + h {
        set_px(frame, width, height, x + w / 2, yy, color);
    }
    for i in 0..w / 2 {
        set_px(frame, width, height, x + w / 2 - i, y + h - 1 - i, color);
        set_px(frame, width, height, x + w / 2 + i, y + h - 1 - i, color);
    }
}
fn icon_save(
    frame: &mut [u32],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u32,
) {
    for xx in x..x + w {
        set_px(frame, width, height, xx, y, color);
        set_px(frame, width, height, xx, y + h - 1, color);
    }
    for yy in y..y + h {
        set_px(frame, width, height, x, yy, color);
        set_px(frame, width, height, x + w - 1, yy, color);
    }
    for xx in x + 2..x + w - 2 {
        set_px(frame, width, height, xx, y + 2, color);
    }
    for yy in y + h / 2..y + h - 2 {
        set_px(frame, width, height, x + 2, yy, color);
        set_px(frame, width, height, x + w - 3, yy, color);
    }
}
fn icon_copy(
    frame: &mut [u32],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u32,
) {
    for xx in x + 2..x + w {
        set_px(frame, width, height, xx, y + 2, color);
        set_px(frame, width, height, xx, y + h - 1, color);
    }
    for yy in y + 2..y + h {
        set_px(frame, width, height, x + 2, yy, color);
        set_px(frame, width, height, x + w - 1, yy, color);
    }
    for xx in x..x + w - 2 {
        set_px(frame, width, height, xx, y, color);
        set_px(frame, width, height, xx, y + h - 3, color);
    }
    for yy in y..y + h - 2 {
        set_px(frame, width, height, x, yy, color);
        set_px(frame, width, height, x + w - 3, yy, color);
    }
}
fn icon_annotate(
    frame: &mut [u32],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u32,
) {
    let len = w.min(h);
    for i in 0..len {
        set_px(frame, width, height, x + i, y + h - 1 - i, color);
    }
    for yy in y + h - 4..y + h - 1 {
        for xx in x + 1..x + 4 {
            set_px(frame, width, height, xx, yy, color);
        }
    }
}
