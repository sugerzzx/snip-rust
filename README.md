# Snip Rust - 轻量截图 / 选区工具 (实验阶段)

基于纯 CPU 渲染与系统截图 API：`winit` + `softbuffer` + `tiny-skia` + `screenshots`。当前目标：验证核心截图、选区、显示管线与热键触发流程。

> 状态：早期原型（prototype）。不存在 UI 框架集成 / 托盘 / 配置持久化。README 已剔除尚未落地的模块描述，后续实现再补。

## 当前源码结构 (实际存在的文件)

```
src/
	main.rs             # 事件循环 + 状态机 (普通模式 / Overlay 选区模式)
	capture.rs          # 全屏 & 区域截图 / 原始 RGBA & PNG 编码
	renderer.rs         # tiny-skia Pixmap 管理 + PNG 解码到画布
	window_manager.rs   # 主窗口 + softbuffer surface 封装
	hotkey.rs           # F4 全局热键订阅（global-hotkey）
	overlay.rs          # 选区覆盖层：变暗背景 + 拖拽矩形 + 裁剪
lib.rs                # 模块 re-export
examples/
	capture_demo.rs     # 简单截图示例（保存文件）
```

（README 旧版提到的 `paste.rs / tray.rs / settings.rs` 尚未实现，后续实现后再加入）

## 已实现功能

- 全屏截图（RGBA 缓冲 + PNG 编码）
- Overlay 覆盖层：
  - 按 F4：隐藏主窗口 -> 捕获屏幕 -> 显示变暗背景
  - 拖拽左键：动态显示选区边框
  - 松开左键：裁剪区域 -> PNG 解码到主窗口显示
- 颜色通道适配（Windows BGRA / RGBA 切换 via 逻辑转换）
- Softbuffer 提交前自动 resize，避免 panic
- 简单 BGRA <-> 显示转换（`renderer.as_bgra_u32()`）
- 变暗背景预计算缓存（提升拖拽时流畅度）

## 使用方法

运行：

```bash
cargo run
```

步骤：

1. 启动后主窗口空白（等待操作）
2. 按下 F4 进入截图选区模式（主窗口隐藏）
3. 拖拽左键绘制区域
4. 松开：选中区域显示在主窗口

## 依赖概览

| Crate                     | 作用                                                     |
| ------------------------- | -------------------------------------------------------- |
| winit                     | 窗口与事件循环（0.30，使用 deprecated run 变体临时保留） |
| softbuffer                | CPU 像素缓冲呈现（BGRA u32）                             |
| tiny-skia                 | CPU 绘制（后续可做标注/矩形/文本）                       |
| screenshots               | 获取屏幕像素（多平台）                                   |
| image                     | PNG 编码 / 解码                                          |
| global-hotkey             | 注册 F4 全局热键                                         |
| anyhow / log / env_logger | 错误与日志                                               |
| bytemuck                  | 像素切片转换辅助                                         |

## 环境变量

| 变量              | 说明                                                          |
| ----------------- | ------------------------------------------------------------- |
| `SNIP_FORCE_BGRA` | 若设置任意值，则假定截图缓冲是 BGRA 并做转换（调试/兼容用途） |

## 构建与运行

```bash
cargo build            # 调试构建
cargo run              # 运行
RUST_LOG=debug cargo run   # 启用调试日志
cargo build --release  # 发布构建
```

Windows CMD：

```
set RUST_LOG=debug && cargo run
```

## 设计要点

- 单线程同步事件循环：无 async，窗口与 overlay 共享逻辑分支
- Overlay 使用单独无装饰 AlwaysOnTop 窗口 + 预计算 dim 缓冲，减少拖拽重绘开销
- 使用 `Box::leak` 维持 `'static` 生命周期给 softbuffer（后续需安全回收替换）
- 仅在鼠标移动且处于拖拽状态时请求 redraw，降低 CPU 占用

## 当前局限 / TODO

| 分类       | 待办                                            |
| ---------- | ----------------------------------------------- |
| 稳定性     | 移除 `Box::leak`，改用自管理生命周期结构        |
| 多显示器   | 目前只抓 `from_point(0,0)` 的一个屏幕；未做拼接 |
| DPI / 缩放 | 未处理 HiDPI 比例差异（逻辑像素 vs 物理像素）   |
| 取消操作   | Esc / 右键取消选区尚未实现                      |
| 选区高亮   | 仅边框；尚未填充半透明/反向遮罩效果             |
| 热键扩展   | 仅 F4，尚未添加自定义注册机制                   |
| 剪贴板     | 尚未复制 PNG / 原始像素到系统剪贴板             |
| 注释工具   | 计划：矩形/箭头/文本/马赛克 等                  |
| Paste 窗口 | 尚未实现贴图悬浮窗口（多实例）                  |
| 托盘       | 无托盘菜单；退出仅靠关闭主窗口                  |
| 配置       | 缺少用户配置/持久化（JSON/ron）                 |

## 典型调用示例

```rust
use snip_rust::capture::capture_fullscreen_raw_with_origin;
let (ox, oy, w, h, rgba) = capture_fullscreen_raw_with_origin()?;
// 处理 rgba 或交给 overlay 显示
```

## 贡献建议

当前更关注管线正确性与可维护性：

- 提交请避免引入大型 GUI / GPU 库（wgpu/egui 等）除非先讨论
- 保持函数职责单一；新增格式编码请添加独立 helper 而不是修改现有 encode 逻辑
- 引入新环境变量或公共 API 时同步更新本 README 与 `.github/copilot-instructions.md`

## Roadmap（短期）

1. Esc 取消 / 右键取消
2. 多显示器：当前光标所在屏幕 / 全拼接模式二选一接口
3. 剪贴板复制（PNG + 原始像素）
4. 注释层（矩形 + 文本）初版
5. 去除 `Box::leak` 替换为持久 Owner 容器
6. 性能采样（大 4K 屏 / 多屏拖拽）

## 调试日志

```bash
RUST_LOG=debug cargo run
```

## License

MIT
