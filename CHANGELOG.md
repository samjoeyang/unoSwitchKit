# Changelog

本文件记录 SwitchKit 实施进度，对应需求文档里程碑 M1–M4。

## [0.3.2] - 2026-10-30

### 修复：菜单栏图标收纳点击无效果（方案重构：AX 移动 → 分隔条撑开）

- **根因（实测定案）**：原方案用 AX API `AXPosition` 移动其它进程的状态项，在本机（macOS 12.7.6 Intel）**所有写入一律返回 `kAXErrorFailure`(-25200)**——屏幕内/外坐标、沙箱内外、辅助功能已授权均一样；进程内 AX 移动自己的状态项、直接 `setFrameOrigin` 挪状态项窗口、`orderFrontRegardless` 调整窗口层级同样无效。对照 Hidden Bar 官方架构文档确认：**macOS 没有提供隐藏/移动其它 App 菜单栏图标的公开 API**，逐项移动（Bartender 式）在 Monterey 上不可行。
- **新方案（Hidden Bar v1.10 同款，明确支持 macOS 10.13–12）**：守护进程在菜单栏放置自己的两个状态项——「箭头」（点击切换收起/展开，⌘ 拖动移动边界）+「分隔条」（收起时长度撑到 `max(500, min(最宽屏×2, 10000))`，把分界线左侧图标全部挤出屏幕左缘；展开时缩回 20pt，图标自动回流）。显示器热插拔后自动重新撑开。
- **交互变更**：面板改为「收起图标/展开图标」总开关 + 只读图标列表（显示中/已收纳徽标）；「点亮」某图标 = 按住 ⌘ 把它拖到箭头右侧（常驻显示），拖动结果经 `autosaveName` 由系统永久记住。首次使用需按住 ⌘ 把箭头拖到想收纳的图标右侧（与 Hidden Bar 一致的手动引导；自动摆放已验证无公开 API 可行，代码保留待未来系统版本）。
- **协议变更**：`menubar:apply`/`menubar:moveto` 移除；新增 `menubar:list`（含 collapsed）、`hidebar:state`（含 freshStart）、`hidebar:set`、`hidebar:autoplace`。Rust 命令 `menubar_set_visible`/`menubar_set_managed` 移除，新增 `menubar_set_collapsed`；settings 只保留 `menubar_collapsed`（守护重启后恢复收起状态）。
- **守护进程健壮性**：端口 9876 被旧实例退出占用时改为重试绑定（原 `try!` 直接崩溃且无人重启）；状态项创建改在 `NSApplication.shared` 初始化之后（修复 `CGSConnectionByID` 断言崩溃）；改用 `NSApplication.run()` 事件循环（状态项按钮点击需要）+ `accessory` 策略。
- 诊断工具：守护日志落 `~/Library/Logs/switchkit-daemon.log`。

### 验证

- ✅ `swiftc` 守护编译通过；✅ `cargo check` 零告警；✅ `npm run build` 通过。
- ✅ 实机协议全链路：list/state/collapse/expand 反复切换状态一致，收起/展开状态机稳定。

## [0.3.1] - 2026-10-30

### 修复：打开面板出现彩虹圈（主线程阻塞）

- **根因**：Tauri 的**同步命令默认在主线程执行**。面板每次打开要同步跑十余次外部进程（10 个 `defaults read`、Finder AppleScript 数废纸篓、`sw_vers`），每 5 秒的 sidecar 状态轮询与菜单栏枚举（最长 8s）也在主线程——主线程被阻塞，macOS 便显示彩虹圈。
- **修复**：`commands.rs` 全部耗时命令改为 `async fn`（移入 tokio 线程池执行），涉及状态读取、开关切换、即时动作、资源释放、蓝牙、菜单栏管理共 24 个命令；`State<'_, AppState>` 按异步命令要求改借用并返回 `Result`。前端 invoke 接口不变。
- 顺带：守护进程状态读取已改用系统框架（见 0.3.0 后修复），此前 `bluetooth:status` 挂起也会在主线程叠加彩虹圈。

## [0.3.0] - 2026-10-30

### 新功能：蓝牙设备管理 + 退出按钮

- **蓝牙设备列表 + 一键连接/断开**：面板新增「蓝牙设备」区，枚举全部已配对设备（名称/地址/连接状态），点选即可连接（守护侧轮询等待，最长 10s）或断开。实现走 macOS **系统框架 `IOBluetooth`**（Swift 守护新增 `bluetooth.swift`，非第三方 CLI）；协议新增 `bt:devices` / `bt:connect` / `bt:disconnect`，Rust 命令 `bt_devices` / `bt_connect` / `bt_disconnect`。首次使用会弹系统的「蓝牙使用」授权。无已配对设备或守护未运行时该区自动隐藏。
- **退出按钮**：面板底栏新增「退出」，调用 `quit_app`（`app.exit(0)`）；守护进程独立运行不受影响。
- 修复：连续两次 `vite build` 报 esbuild "Broker request timed out"，为残留 esbuild 进程所致，`pkill -f esbuild` 后恢复（环境问题，非代码）。

### 验证

- ✅ `swiftc` 守护编译通过（新增 `-framework IOBluetooth`）；✅ `cargo check` 零告警；✅ `npm run build` 通过。

## [0.2.0] - 2026-10-30

### 新功能：扩展开关与即时动作（仿同类工具面板，按可行性裁剪）

**新增开关（持久状态，读取系统真值）**

- **隐藏 Dock**：`defaults write com.apple.dock autohide` + `killall Dock`。
- **显示隐藏文件**：`defaults write com.apple.finder AppleShowAllFiles` + `killall Finder`。
- **屏幕清洁**：一键收起所有前台 App 窗口（System Events `set visible of p to false`，按 unix id 跳过自身）；再次点击恢复显示。状态由应用内存维护。
- **台前调度**：`com.apple.WindowManager GloballyEnabled` + `killall Dock`。**仅 macOS 13+ 可用，低版本系统上面板自动隐藏该开关**（`stage_manager_available`）；本机 macOS 12 上不可见，13+ 实机待验证。

**新增即时动作（一次性执行，非开关）**

- **锁定屏幕**：`CGSession -suspend`，失败回退 `pmset displaysleepnow`。
- **清空剪贴板**：`pbcopy` 置空。
- **屏幕保护**：`open -a ScreenSaverEngine`。
- **关闭所有应用**：System Events 遍历前台 App 逐个优雅 `quit`（跳过自身与 Finder；未保存文档会弹存盘框，AppleEvent 超时限制 15s 防面板卡死），返回已请求退出的数量。

**明确不做**：夜览（无官方 CLI，依赖第三方/脆弱 UI 脚本）、锁定键盘（CGEventTap 复杂且有失锁风险）；台前调度按系统版本条件显示。

### 验证

- ✅ `cargo check`（Tauri 2.12.5）通过；✅ `npm run build`（tsc + vite）通过。

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
