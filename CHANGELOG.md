# Changelog

本文件记录 SwitchKit 实施进度，对应需求文档里程碑 M1–M4。

## [0.1.1] - 2026-10-30

### 首轮实机反馈修复

- **勾选开机自启后出现两个托盘图标**：根因是 LaunchAgent `RunAtLoad + launchctl load` 会立即拉起第二个实例。接入 `tauri-plugin-single-instance`（第二实例启动即退出并唤起已有面板），并让注册幂等（load 前先 unload）。无论 launchd 拉起还是手动双开，始终只有一个图标。

- **托盘图标单色化**：新增 `icons/tray-icon.png`（36×36 单色开关 glyph，18pt @2x），托盘改用 `icon_as_template(true)` 模板图，macOS 自动适配菜单栏深/浅色，与其他系统图标观感一致。图标编译期内嵌（`include_bytes!`），不依赖运行时路径。
- **面板可滚动**：窗口固定 `320×540`（`inner_size`），`#root` 允许内部滚动（`overflow-y: auto` + 细滚动条），底部被裁切看不到的条目（开机自启等）现在可完整查看。
- **废纸篓计数/清空修正**：`trash_count` 与 `empty_trash` 改为以 **Finder（AppleScript）为权威来源**（与系统废纸篓显示完全一致），失败时回退直读 `~/.Trash`；清空走 Finder `empty trash`，可处理锁定/特殊权限文件。注意首次调用可能弹一次「App 想要控制 Finder」授权框。
- **状态时效性**：面板每次重新显示（`visibilitychange`）时自动刷新开关状态，避免托盘弹出后显示过期计数。

### 新功能：菜单栏图标收纳（Bartender/Ice 类 MVP）

- 其他 App 的菜单栏图标可收纳进 SwitchKit 面板统一管理：**默认收纳（隐藏），"点亮"的才显示在菜单栏**；守护进程每 3 秒巡检，新出现的图标自动收纳。
- 实现：Swift 守护新增 `menubar.swift`（AX API 枚举各进程 `AXExtrasMenuBar`，以 `AXPosition` 移出屏幕左缘实现收纳、移回记录的原位实现恢复）；Rust 启动时自动拉起守护（`binaries/switchkit-daemon-<triple>`，已在运行则跳过）并推送策略（白名单 + 原始位置，持久化于 settings.json）。
- 面板新增「菜单栏图标」区：逐项点亮/收纳、管理总开关（关闭即全部恢复并停止巡检）、辅助功能授权引导提示。
- 需要 系统设置 → 隐私与安全性 → **辅助功能** 授权 `switchkit-daemon`（首次启动弹引导）。Control Center 的部分系统项（Wi-Fi/电池等）可能拒绝移动，属系统限制。
- 协议新增 `menubar:list` / `menubar:apply`；Rust 命令新增 `menubar_state` / `menubar_set_visible` / `menubar_set_managed`。

### 验证

- ✅ `swiftc` 守护编译通过（x86_64，macOS 12 SDK）；✅ `cargo check`（Tauri 2.12.5）零告警；✅ `npm run build` 通过。

## [0.1.0] - 2026-10-30

### 里程碑进度

- **M1 菜单栏骨架** ✅ 完成
  - Tauri 2.0 工程初始化（Cargo / Vite / Tailwind）。
  - `tauri.conf.json` 设 `activationPolicy: accessory`（无 Dock / 无 App Switcher）。
  - `tray.rs`：托盘图标 + 左键点击定位无边框面板 + 失焦自动收起。
  - `store.rs`：开机自启 LaunchAgent 注册/移除。

- **M2 FR-2 P0 开关** ✅ 完成（纯 shell）
  - 深色模式（`osascript` System Events）。
  - 隐藏桌面图标（`defaults write CreateDesktop` + `killall Finder`）。
  - 阻止睡眠（`caffeinate -dimsu` 子进程句柄管理）。
  - 清空废纸篓（仅删 `~/.Trash` 内容）。
  - 面板打开即读取系统真实状态（FR-7）。

- **M3 FR-3 P0 资源释放 + 权限提升** ✅ 完成
  - 释放内存（`purge`，`privilege.rs` osascript 密码框提权）。
  - 刷新 DNS（`dscacheutil -flushcache; killall -HUP mDNSResponder`）。
  - 清理用户缓存（`cache_whitelist.rs`：白名单 + 主目录限制 + 预览统计 + 二次确认）。
  - 内存释放前后对比（`vm_stat` / `sysctl`）。

- **M4 Swift sidecar + 设置持久化 + 打磨** 🟡 基础完成，待实体机验证
  - `swift/daemon.swift` + `bridge.swift`：本地 TCP/JSON 守护（`:9876`），暴露蓝牙开关（blueutil）与前台应用监听（NSWorkspace）。
  - `store.rs`：设置 JSON 持久化（开关顺序 / 白名单 / 上次释放结果）至 AppConfig 目录。
  - React 面板 UI（毛玻璃无边框、快捷开关网格、资源释放区、自启开关、sidecar 状态指示）。

### 实施偏差（相对设计文档 v0.1）

| 设计文档 | 实施决定 | 原因 |
|---|---|---|
| 持久化用 `tauri-plugin-store` | 改用 `std::fs` JSON 文件（AppConfig 目录） | MVP 减少插件依赖与权限面，等价能力；后续可平滑替换为插件 |
| Swift sidecar 走 WebSocket `:9876` | 改用 TCP + 换行分隔 JSON（同样的 `:9876`） | 零依赖落地；Rust 侧用 `std::net::TcpStream`，后续可平滑替换为 WebSocket |
| `minimumSystemVersion` 未指定 | 暂设 `10.13` | 开发机为 macOS 12，需可构建；发布前上调至 Ventura 13 |
| React 19 | 采用 React 18.3 | 更稳定、与 Vite 插件兼容性最佳 |

### 验证状态

- ✅ `cargo check`（macOS 12.7.6 / Rust 1.99.1 / Tauri 2.12.5）后端可编译，零告警。
- ✅ `npm run build`（`tsc` 类型检查 + `vite build`）前端通过，产物 `dist/` 正常生成。
- ⏳ `npm run tauri dev` 完整 GUI 运行需在 macOS 13+ 实体机验证（本机无 GUI 显示）。
- ⏳ 特权弹窗、蓝牙（需 blueutil）、公证分发待实体机联调。
