# Snip Rust - Rust 截图工具

一个使用 Rust 开发的跨平台截图软件，基于 iced GUI 框架构建。

## 项目结构

```
snip_rust/
├── Cargo.toml          # 主项目配置
├── src/
│   └── main.rs         # 应用入口
├── snip-core/          # 核心功能模块
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs      # 核心库入口
│       ├── screenshot.rs # 截图功能
│       └── hotkey.rs   # 全局快捷键
└── snip-ui/            # UI 界面模块
    ├── Cargo.toml
    └── src/
        └── lib.rs      # iced UI 实现
```

## 功能特性

- ✅ 基于 iced 的现代化 GUI 界面
- 🔧 截屏功能（开发中）
- 🔧 图像处理和编辑（开发中）
- 🔧 全局快捷键支持（开发中）
- 🔧 系统托盘集成（开发中）
- 🔧 异步任务处理（开发中）

## 依赖说明

- **iced**: GUI 框架，用于构建现代化用户界面
- **screenshots**: 截屏功能库
- **image**: 图像处理和 PNG 编码
- **global-hotkey**: 跨平台全局快捷键注册
- **tray-item**: 系统托盘功能
- **tokio**: 异步运行时
- **log/env_logger**: 日志记录
- **anyhow/thiserror**: 错误处理

## 开发环境要求

- Rust 1.70+
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

1. 实现基础截图功能
2. 添加图像编辑工具
3. 集成全局快捷键
4. 实现系统托盘功能
5. 添加配置文件支持
6. 优化性能和用户体验

## 开发指南

### 添加新功能

1. 在 `snip-core` 中实现核心逻辑
2. 在 `snip-ui` 中添加相应的 UI 组件
3. 更新主应用逻辑

### 调试

使用环境变量启用日志输出：

```bash
RUST_LOG=debug cargo run
```

## 许可证

MIT License

## 贡献

欢迎提交 Pull Request 和 Issue！
