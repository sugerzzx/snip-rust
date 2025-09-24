use std::{env, fs, io::Cursor, path::PathBuf};

fn main() {
    // 监听源图标变更（即使内容改名不变也触发重新运行）
    println!("cargo:rerun-if-changed=assets/app_icon.png");

    if !cfg!(target_os = "windows") {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let ico_path = out_dir.join("app_icon.ico");

    // 读取主 PNG
    let png_bytes = include_bytes!("assets/app_icon.png");
    let reader = image::ImageReader::new(Cursor::new(&png_bytes[..]))
        .with_guessed_format()
        .expect("icon format");
    let base_img = reader.decode().expect("decode icon").to_rgba8();

    // 需要的多尺寸集合（Windows shell/任务栏/Alt+Tab 不同场景）
    let sizes = [16u32, 24, 32, 48, 64, 128, 256];
    let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);
    for &sz in &sizes {
        // 若源图小于目标尺寸，直接放大；采用 Lanczos3 提升质量
        let resized = if base_img.width() == sz && base_img.height() == sz {
            base_img.clone()
        } else {
            image::imageops::resize(&base_img, sz, sz, image::imageops::Lanczos3)
        };
        let icon_image = ico::IconImage::from_rgba_data(sz, sz, resized.into_raw());
        let entry = ico::IconDirEntry::encode(&icon_image).expect("encode entry");
        icon_dir.add_entry(entry);
    }
    let mut ico_bytes: Vec<u8> = Vec::new();
    icon_dir.write(&mut ico_bytes).expect("write ico");
    fs::write(&ico_path, &ico_bytes).expect("save ico");

    let mut res = winres::WindowsResource::new();
    res.set_icon(ico_path.to_str().unwrap());
    let _ = res.compile();
}
