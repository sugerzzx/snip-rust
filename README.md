# Snip Rust - 轻量截图 / 选区工具 (实验阶段)

基于纯 CPU 渲染与系统截图 API：`winit` + `softbuffer` + `tiny-skia` + `screenshots`。当前目标：验证核心截图、选区、钉住窗口 (Pin) 管线与热键触发流程。

> 状态：早期原型（prototype）。已加入：
>
> - Overlay 选区 + 多粘贴窗口 (Pin)
> - 系统托盘（退出菜单）
> - 可执行文件嵌入多尺寸应用图标 (build.rs 生成 ICO)
>   仍缺少：配置持久化 / 剪贴板 / 标注层 / 多显示器拼接。

## 当前源码结构 (实际存在的文件)

```
src/
	main.rs             # 事件循环：F4 截图 -> Overlay -> Pin 生成多个粘贴窗口 + 托盘
	capture.rs          # 全屏 & 区域截图 / 原始 RGBA & PNG 编码
	renderer.rs         # tiny-skia Pixmap 管理 (后续标注用)
	paste_window.rs     # 钉住(粘贴)窗口：预渲染边框/多实例/拖动
	hotkey.rs           # F4 全局热键订阅（global-hotkey）
	overlay/            # Overlay 子模块 (state / toolbar / handles / drawing)
build.rs              # 构建期生成多尺寸 ICO 并嵌入 exe 资源
assets/app_icon.png   # 源 PNG（构建时生成 16~256 多尺寸 ICO）
lib.rs                # 模块 re-export
examples/
	capture_demo.rs     # 简单截图示例（保存文件）
```

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
- 系统托盘：图标 + 退出菜单项（tray-icon）
- 可执行文件图标：多尺寸 ICO 内嵌（16/24/32/48/64/128/256）
- Paste 窗口预渲染边框缓冲，加速拖动（不再每帧重绘阴影）

## 使用方法

运行：

```bash
cargo run
```

步骤：

1. 启动后无主预览窗口（常驻后台监听 F4）
2. 按下 F4 进入截图选区 Overlay 模式
3. 拖拽左键绘制区域（可调整 / 移动 / 工具栏）
4. 点击工具栏“钉住”(Pin)：生成一个独立粘贴窗口（支持多实例）
5. 粘贴窗口 (Pin)：
   - 无边框 / 置顶 / 可左键拖动移动
   - 预渲染双层边框：聚焦亮蓝 / 失焦灰色
   - 多窗口并存，可各自关闭
   - 右键 / Esc（未来计划）关闭；当前右键已隐藏窗口（关闭逻辑后续统一）

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
| tray-icon                 | 系统托盘菜单与图标                                       |
| winres / ico (build)      | 构建期生成多尺寸 ICO 并嵌入                              |

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
| 取消操作   | Esc / 右键取消选区尚未实现（计划）              |
| 选区高亮   | 仅边框；尚未填充半透明/反向遮罩效果             |
| 热键扩展   | 仅 F4，尚未添加自定义注册机制                   |
| 剪贴板     | 尚未复制 PNG / 原始像素到系统剪贴板             |
| 注释工具   | 计划：矩形/箭头/文本/马赛克 等                  |
| Paste 窗口 | 已实现多实例/拖动/预渲染边框；缺关闭回收逻辑    |
| 托盘       | 已有退出菜单；待添加“立即截图/设置/主题”        |
| 图标缓存   | Windows 可能缓存旧图标；需清除 Explorer 缓存    |
| 配置       | 缺少用户配置/持久化（JSON/ron）                 |

## 图标缓存刷新（Windows）

若替换 `assets/app_icon.png` 后图标仍旧：

```cmd
taskkill /f /im explorer.exe
del /q %LOCALAPPDATA%\IconCache.db
del /q %LOCALAPPDATA%\Microsoft\Windows\Explorer\iconcache_*.db
start explorer.exe
```

然后重新构建：

```cmd
cargo build --release
```

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

1. Esc / 右键取消选区
2. 多显示器支持（当前屏 / 全拼接）
3. 剪贴板复制（PNG + RGBA）
4. 注释层（矩形 / 文本）
5. 去除 `Box::leak` 改为安全所有权容器
6. Paste 窗口清理 / 关闭一致性
7. 性能采样（4K / 多屏拖拽）
8. 托盘：添加“立即截图 / 设置”
9. 主题适配（深/浅色托盘图标）

## 调试日志

```bash
RUST_LOG=debug cargo run
```

## License

MIT
