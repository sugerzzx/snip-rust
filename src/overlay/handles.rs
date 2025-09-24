#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeHandle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

pub fn hit_test_handle(cx: i32, cy: i32, x: u32, y: u32, w: u32, h: u32) -> Option<ResizeHandle> {
    if w == 0 || h == 0 {
        return None;
    }
    let x = x as i32;
    let y = y as i32;
    let w = w as i32;
    let h = h as i32;
    let points = [
        (x, y, ResizeHandle::TopLeft),
        (x + w / 2, y, ResizeHandle::Top),
        (x + w - 1, y, ResizeHandle::TopRight),
        (x + w - 1, y + h / 2, ResizeHandle::Right),
        (x + w - 1, y + h - 1, ResizeHandle::BottomRight),
        (x + w / 2, y + h - 1, ResizeHandle::Bottom),
        (x, y + h - 1, ResizeHandle::BottomLeft),
        (x, y + h / 2, ResizeHandle::Left),
    ];
    const R: i32 = 5;
    for (px, py, id) in points {
        if (cx - px).abs() <= R && (cy - py).abs() <= R {
            return Some(id);
        }
    }
    None
}
