# Changelog

本文件记录 SwitchKit 实施进度，对应需求文档里程碑 M1–M4。

## [0.4.1] - 2026-10-31

### 构建：分发包默认 universal，修正「提示安装 Rosetta」与最低系统版本错配

**问题**

- 0.4.0 的 `SwitchKit-adhoc.zip` 实为 thin x86_64。构建机是 Intel Mac，脚本默认 `MODE=current`，`tauri build` 只出当前架构；Apple Silicon 用户装上即被要求安装 Rosetta 2，与「分发包为 universal 二进制」的对外承诺不一致。
- `minimumSystemVersion` 曾写 `10.13`，与 Swift 守护实际部署目标不符：守护依赖 Network 框架（macOS 10.14+），且 Intel 切片此前默认按主机 SDK（12.0）编译，老 Intel Mac「装上即崩」。最低系统版本受 Network 框架限制，不能低于 10.14。

**修复**

- `scripts/build-release.sh`：默认 `--universal`，新增 `--current` 显式单架构；`swift_daemon()` 统一 `-target <arch>-apple-macosx$MACOS_MIN`（`10.14`），两个切片对齐（arm64 / x86_64 均按 10.14，覆盖 Apple Silicon 与 Intel）。
- 构建后硬校验：主程序与守护进程的 `lipo -archs` 必须同时含 `arm64` + `x86_64`，缺失直接失败；另校验 Info.plist 的 `LSMinimumSystemVersion` 与 `MACOS_MIN` 一致。
- 分发包改名 `SwitchKit-<version>-universal.zip`（单架构为 `-<arch>`），并在打包前删掉旧命名文件，避免同名包混淆误发。
- `tauri.conf.json`：`minimumSystemVersion` 定为 `10.14`（受 Swift 守护依赖的 Network 框架限制，无法下调到 10.13；与两个切片部署目标一致）。
- 顺带修 `--current` 在 Apple Silicon 上取到非法 triple `arm64-apple-darwin`（映射为 `aarch64`）；修 macOS 自带 bash 3.2 把 `$var（` 吞成全角字节导致的 `unbound variable`（统一 `${var}`）。

**产物**：`src-tauri/target/universal-apple-darwin/release/bundle/macos/SwitchKit-0.4.1-universal.zip`（两个可执行文件 `minos` 均为 10.14）

### 修复：点击「退出」后进程残留，覆盖安装提示「应用程序正在运行」

**根因（三层叠加）**

1. **守护进程永不退出**：`switchkit-daemon` 是 Swift AppKit 常驻进程（`NSApplication.run()`），打包后位于 `SwitchKit.app/Contents/MacOS/`。此前 `spawn_sidecar` 用 `std::mem::forget(child)` 刻意不持有句柄，主程序退出后它变成孤儿进程继续运行，**持续持有包内可执行文件的 mmap**。Finder 判定「app 正在运行」看的是 bundle 内文件是否被占用，因此覆盖替换必然失败。
2. **主进程退出无兜底**：`quit_app` 直接 `app.exit(0)`，无超时保护。Tauri 在 `transparent + always_on_top + Accessory` 窗口组合下的退清理偶发挂起（实测有 debug 主进程存活 15 小时的现场），主进程未死时其子 daemon 自然也不会退。
3. **开机自启指向旧路径**：`com.switchkit.app.plist` 记录的是写入时刻的 `current_exe()`，包被替换或移动后路径失效，launchd 仍指向旧副本。

**修复**

- `main.swift`：新增父进程看门狗。App 通过 `--parent-pid` 传入自身 PID，守护进程用 `DispatchSource.makeProcessSource(eventMask: .exit)` 监听内核的进程退出事件，父进程一终止（含 `kill -9`、崩溃、终端关闭）立即自杀。**不用 `kill(pid,0)` 轮询**——父进程未被 reap 时是僵尸，探测会误判存活。
- `main.rs`：`spawn_sidecar` 改为返回 `Child` 并存入 `AppState.sidecar`；`app.manage` 移入 `setup` 内，保证句柄先于调用注册。
- `main.rs`：改用 `build() + run(|handle, event| ...)`，在 `RunEvent::Exit` 中回收守护进程。
- `commands.rs`：新增 `shutdown_sidecar`（kill 句柄 + 按 9876 端口兜底清理旧版无看门狗的残留）；`quit_app` 先回收再退出，并起 3 秒硬退兜底线程。
- `store.rs`：新增 `repair_launch_agent`，正式构建启动时校验自启 plist 的 exe 路径，不一致则重写重载。
- 新增 `scripts/clean-switchkit.sh`：一键清理残留进程与端口，`--all` 同时卸载自启项。

### 运维：替换 .app 的正确姿势

```bash
./scripts/clean-switchkit.sh     # 先清干净
# 再从 Finder 或命令行替换 SwitchKit.app
```

### 验证

- ✅ `swiftc` 编译零警告；✅ `cargo check` 零告警。
- ✅ 看门狗实测：`kill -9` 父进程后守护进程 3 秒内自动退出（日志 `sidecar: parent <pid> exited, shutting down`）。

## [0.4.0] - 2026-10-31

### 新增：多平台余额查询（取代原 DeepSeek 专用余额）

- **Rust**：`deepseek.rs` → `balance.rs`，按 `provider` 分发；结果模型统一为 `AccountBalance { provider, label, ok, is_available, balances[] }`，金额行归一化为 `{label, amount, currency, primary}`。
- **支持的平台与接口**：
  | provider | 接口 | 备注 |
  |---|---|---|
  | deepseek | `GET api.deepseek.com/user/balance` | 多币种、充值/赠送明细 |
  | kimi / kimi_intl | `GET {api.moonshot.cn,api.moonshot.ai}/v1/users/me/balance` | 可用/现金/代金券 |
  | zhipu | `GET open.bigmodel.cn/api/paas/v4/users/me/balance` | 旧版资源接口（官方文档未列），仅按量付费余额 |
  | stepfun | `GET api.stepfun.com/v1/accounts` | 后付费账户余额 0 仍判为可用 |
  | novita | `GET api.novita.ai/openapi/v1/billing/balance/detail`（旧 `v3/user/balance` 兜底） | 金额单位 1/10000 USD |
  | openrouter | `GET openrouter.ai/api/v1/credits`（回退 `/api/v1/key`） | 余额 = 购买额 − 已用 |
  | ai302 | `GET api.302.ai/dashboard/balance` | Key 需开通「管理后台」权限 |
  | aihubmix | `GET aihubmix.com/api/user/self`（回退 `/dashboard/billing/remain`） | quota ÷ 500000 = USD |
  | custom | `GET {base}/dashboard/billing/subscription` + `/usage`（回退 `/api/user/self`） | new-api / one-api 系中转站 |
- **并行查询**：多 Key 用 `std::thread::scope` 并行，单 Key 失败不影响其它。
- **网络代理**：设置新增「网络代理」，写入 `settings.json` 的 `http_proxy`，余额请求统一带 `--proxy`（境外站点直连易 TLS 重置）。留空直连。
- **配置迁移**：`Settings.deepseek_keys` → `provider_keys`（serde alias + 空 provider 视为 deepseek），旧配置无需手动迁移，并有单测覆盖。

### 改动：前端余额区与表单

- `DeepSeekSection` → `BalanceSection`：每行显示平台标签 + 备注 + 主余额，明细（充值/赠送/已用）并排小字；列表限高 196px 内部滚动。
- `DeepSeekKeyForm` → `KeyForm`：平台下拉选择、按平台显示取 Key 提示、中转站额外填站点地址。
- 设置面板：`DeepSeek API Key` 行改为 `模型余额账号`（无账号时才显示添加入口）。

### 验证

- ✅ `cargo check` 零告警；✅ `cargo test`（配置迁移用例通过）；✅ `npm run build`（tsc + vite）。
- ✅ 9 个官方接口用无效 Key 探测均为 401（路径有效，非 404）；302.AI / AiHubMix 需经代理访问，直连在本机被重置，故新增代理设置。
- ⏳ 真实 Key 余额数值待装机验证。

## [0.3.9] - 2026-10-30

### 新增：设置模块

- **底部「设置」入口**：取代原先底部的「检查更新」按钮位置；点击展开 / 收起设置面板，面板位于底栏上方。
- **开机自启、检查更新移入设置**：底栏不再暴露这两项，统一在设置面板内管理（开关 + 检查按钮 + 结果提示）。
- **DeepSeek API Key 首次添加移入设置**：无 Key 时余额区整块隐藏（不再有浮动的「+ DeepSeek 余额」入口），首次添加在设置面板内完成；添加成功后余额区立即出现并开始定时刷新。设置与余额区共用 `DeepSeekKeyForm`，Key 增删通过版本号双向同步。
- **底栏布局**：`设置`（含齿轮图标）与 `原生守护` 状态在左，`退出` 单独置于最右。
- **修复**：开机自启切换失败原先用 `alert()` 提示，而 WKWebView 下 `alert` 被禁用会静默失败；改为设置面板内文字提示。

### 修复：蓝牙设备列表过高

- 设备列表限高 `134px` 并可内部滚动（约显示 3 个设备），设备多时不再把整个面板撑长；新增 `.thin-scroll` 细滚动条样式（`.thin-scroll` 同时用于 DeepSeek 余额列表）。

## [0.3.4] - 2026-10-30

### 新增：在线更新能力（graceful，当前无线上地址默认禁用）

- **`tauri-plugin-updater` + `tauri-plugin-process`**：接入 Tauri 标准更新器。面板底部新增「检查更新」按钮，有更新时自动下载安装并重启。
- **`get_update_config` 命令 + `UpdateConfig`**：告知前端更新服务是否已配置（编译期 `SWITCHKIT_UPDATE_ENDPOINTS` 决定，`option_env!` 固化进二进制）。当前未配置 → 按钮显示「更新服务尚未配置（暂未提供线上地址）」，不报错。
- **发布密钥**：`tauri signer generate` 生成 Ed25519 密钥对；公钥已写入 `tauri.conf.json` `plugins.updater.pubkey`，私钥存于 `~/.switchkit-updater-secret.key`（不入库）。
- **构建脚本联网开关**：`scripts/build-release.sh` 现支持在构建期导出 `SWITCHKIT_UPDATE_ENDPOINTS` 时，自动把 manifest URL 注入 `tauri.conf.json` 的 `endpoints` 并还原，无需改代码即可启用 OTA 更新。
- **开机自启**：复用既有 `LaunchAgent` 机制（勾选即写 `~/Library/LaunchAgents/com.switchkit.app.plist`，面板复选框已存在），本次仅做保留确认。

### 验证

- ✅ `cargo check` 零告警；✅ `npm run build` 通过（含 `@tauri-apps/plugin-updater` / `plugin-process`）。
- ⏳ 通用包（`--universal`）构建中；收起/展开、更新开关等实机交互待装机验证。

## [0.3.3] - 2026-10-30

### 新增：发布构建脚本与签名分发策略

- **`scripts/build-release.sh`**：一键发布构建——Swift 守护编译（支持 `--universal` 双架构 lipo 合并）→ 签名身份自动检测（钥匙串找 Developer ID 证书，可用 `APPLE_SIGNING_IDENTITY` 显式指定）→ `tauri build` → 产物签名收尾与校验（守护进程必须已进包且已签名，否则构建失败）。
- **签名降级策略**：无证书时自动 ad-hoc 签名（`codesign -s -`），守护进程随包签名不会被系统静默拦截；Gatekeeper 弹「未验证开发者」，用户右键 → 打开（macOS 13+ 走系统设置）。有证书且有公证凭据时走 Tauri 自动 notarize + staple。
- **修复发布包守护进程缺失**：`tauri.conf.json` 此前未配置 `externalBin`，`tauri build` 产物中**不含 Swift 守护进程**（release 下 `exe.parent()/switchkit-daemon` 找不到文件，蓝牙/菜单栏收纳全失效，且用户机器上无 `CARGO_MANIFEST_DIR` 兜底路径）。已加 `"externalBin": ["binaries/switchkit-daemon"]`，守护随包打进 `Contents/MacOS/` 并参与统一签名。
- 文档：构建指南新增 §3.1 分发签名策略、§6 公证分发完整流程（证书创建/凭证存储/环境变量/验证/手动公证）。

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
