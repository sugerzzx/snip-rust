mod capture;
mod hotkey;

use env_logger;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    env_logger::init();

    Ok(())
}
