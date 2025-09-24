pub fn set_px(frame: &mut [u32], width: u32, height: u32, x: i32, y: i32, color: u32) {
    if x < 0 || y < 0 {
        return;
    }
    let (sw, sh) = (width as i32, height as i32);
    if x >= sw || y >= sh {
        return;
    }
    frame[(y as u32 * width + x as u32) as usize] = color;
}

pub fn fill_rect(
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
            frame[(row + xx as u32) as usize] = color;
        }
    }
}

pub fn stroke_rect(
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
    for xx in x.max(0)..(x + w).min(sw) {
        if y >= 0 && y < sh {
            frame[(y as u32 * width + xx as u32) as usize] = color;
        }
        let by = y + h - 1;
        if by >= 0 && by < sh {
            frame[(by as u32 * width + xx as u32) as usize] = color;
        }
    }
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

pub fn draw_handle(frame: &mut [u32], width: u32, height: u32, cx: i32, cy: i32, half: i32) {
    let (sw, sh) = (width as i32, height as i32);
    for yy in (cy - half)..=(cy + half) {
        if yy < 0 || yy >= sh {
            continue;
        }
        for xx in (cx - half)..=(cx + half) {
            if xx < 0 || xx >= sw {
                continue;
            }
            let idx = (yy as u32 * width + xx as u32) as usize;
            frame[idx] = 0xFFFFFFFF;
        }
    }
}
