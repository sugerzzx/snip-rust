# Snip Rust - Rust 截图工具

一个使用 Rust 开发的跨平台截图软件，基于 `winit` + `softbuffer` + `tiny-skia` + `screenshots` 等库实现。

## 项目结构

```
snip_rust/
├─ Cargo.toml
├─ src/
│  ├─ main.rs            // 程序入口：event loop, init modules
│  ├─ capture.rs         // 使用 screenshots 做截屏（全屏 / 区域）
│  ├─ window_manager.rs  // winit 窗口创建、softbuffer surface 管理
│  ├─ renderer.rs        // tiny-skia 渲染逻辑：把图像/标注绘到像素缓冲
│  ├─ hotkey.rs          // 全局热键、鼠标/键盘事件抽象
│  ├─ paste.rs           // 贴图窗口逻辑：位置/缩放/z-index 管理
│  ├─ tray.rs            // 系统托盘集成
│  └─ settings.rs        // 配置与持久化（serde）
```

## 功能特性

- 🔧 界面设计（开发中）
- ✅ 截屏功能（已实现）
- 🔧 图像处理和编辑（开发中）
- 🔧 全局快捷键支持（开发中）
- 🔧 系统托盘集成（开发中）
- 🔧 异步任务处理（开发中）

## 依赖说明

- **winit**: （窗口 + 事件循环）
- **softbuffer**: （无需 GPU 的像素缓冲输出）

- **tiny-skia**（CPU 2D 绘图库）

- **screenshots**（跨平台截图）

- **image**（图像格式编码/解码）

- **global-hotkey** / rdev（全局热键）

- **tray-item**（系统托盘）

- **serde**, **dirs**, **anyhow**, **log** 等工具库

## 开发环境要求

- Rust 1.90
- Cargo
- Windows/macOS/Linux

## 开发启动步骤

### 1. 克隆项目

```bash
git clone <repository-url>
cd snip_rust
```

### 2. 安装依赖

```bash
cargo fetch
```

### 3. 构建项目

```bash
cargo build
```

### 4. 运行应用

```bash
cargo run
```

应用将打开一个带有标题"Snip Rust - 截图工具"的窗口，包含一个简单的计数器界面。

### 5. 开发模式（自动重新编译）

```bash
cargo watch -x run
```

## 项目架构

### snip-core

核心功能模块，包含：

- 截图功能实现
- 全局快捷键管理
- 图像处理逻辑

### snip-ui

用户界面模块，基于 iced 框架：

- 主窗口界面
- 系统托盘集成
- 用户交互逻辑

## 构建发布版本

```bash
cargo build --release
```

编译后的可执行文件位于 `target/release/` 目录下。

## 下一步计划

1. 集成全局快捷键
2. 选区截图功能
3. 截图贴图功能
4. 添加图像编辑工具
5. 实现系统托盘功能
6. 添加配置文件支持
7. 优化性能和用户体验

### 调试

使用环境变量启用日志输出：

```bash
RUST_LOG=debug cargo run
```

## 许可证

MIT License
