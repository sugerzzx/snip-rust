use snip_rust::capture::{capture_area, capture_fullscreen, Rect};
use std::fs;

fn main() -> anyhow::Result<()> {
    let full = capture_fullscreen()?;
    fs::write("fullscreen.png", &full)?;
    println!("Saved fullscreen.png ({} bytes)", full.len());

    // 试着截取左上角 400x300
    let area = capture_area(Rect {
        x: 0,
        y: 0,
        width: 400,
        height: 300,
    })?;
    fs::write("area.png", &area)?;
    println!("Saved area.png ({} bytes)", area.len());
    Ok(())
}
