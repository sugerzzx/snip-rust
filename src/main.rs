use env_logger;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    env_logger::init();

    // 运行iced应用
    snip_ui::run()?;

    Ok(())
}
