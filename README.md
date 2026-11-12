# SwitchKit

Mac 菜单栏快捷开关 + 系统资源释放工具。macOS 专属、常驻菜单栏、体积极小（目标：安装包 < 15MB、常驻内存 < 50MB）。

## 技术栈

| 层 | 选型 |
|---|---|
| 桌面壳 | Tauri 2.0（Rust） |
| 前端面板 | React 18 + TypeScript + Tailwind + Vite |
| 原生补充 | Swift sidecar daemon（蓝牙 / 前台监听） |
| 特权执行 | `osascript` 密码框（MVP）→ `SMJobBless` helper（生产） |
| 持久化 | JSON 文件（AppConfig 目录，MVP 简化） |

## 功能

- **快捷开关（FR-2 P0）**：外观三态（浅色 / 深色 / 自动）、隐藏桌面图标、阻止睡眠（caffeinate）、清空废纸篓、隐藏 Dock、显示隐藏文件、屏幕清洁（一键收起/恢复全部前台窗口）、台前调度（仅 macOS 13+ 显示）。点面板底栏「排序」可拖动这些开关卡片换位置，顺序写入 `settings.toggle_order` 并在下次启动恢复，可一键恢复默认。
- **日出日落自动切换**：「自动」直接复用 macOS 外观设置（跟随日出/日落或夜览日程），不申请定位权限、不常驻天文计算、不新增 TCC 授权；面板每 30 秒用轻量命令同步当前外观。macOS 12 需先在「系统偏好设置 → 显示器 → 夜览」把日程设为「日落到日出」，13+ 在「系统设置 → 外观」选自动。
- **全局快捷键**：设置页可为 9 个动作录制组合键，持久化并在启动时恢复；只接受「至少一个修饰键 + 普通键」（Carbon `RegisterEventHotKey`，无需辅助功能 / 输入监控），媒体键、单键被排除，冲突与系统占用逐项提示。
- **截屏录屏入口**：一键拉起系统的截屏录屏工具栏（⌘⇧5 的同一个 App，`open -b com.apple.screenshot.launcher`）。该工具栏**同时含静态截图与屏幕录制两组按钮**（截取整个屏幕 / 窗口 / 选区，录制整个屏幕 / 选区；录制所选窗口需 macOS 26+），一个按钮覆盖两项功能。采集、编码、保存全部由系统 App 完成，SwitchKit **不读取屏幕内容、不保存文件、不申请屏幕录制权限**，也不 spawn `/usr/sbin/screencapture`（该 CLI 的 TCC 检查会归属到调用者）。系统 App 自带 `com.apple.private.screencapture.allow` 与 `com.apple.private.tcc.allow = [kTCCServiceScreenCapture]`，故录屏同样零 TCC。
- **即时动作**：锁定屏幕、清空剪贴板、启动屏幕保护、关闭所有应用（优雅退出，跳过自身与 Finder）。
- **蓝牙设备**：列出已配对设备，一键连接 / 断开（系统框架 IOBluetooth，经 Swift 守护，无第三方依赖）。
- **资源释放（FR-3 P0）**：释放内存（`purge`，需 root）、刷新 DNS（`dscacheutil`/`mDNSResponder`，需 root）、清理用户缓存（白名单 + 预览 + 二次确认）。
- **开机自启（FR-4）**：注册/移除 `~/Library/LaunchAgents/com.switchkit.app.plist`（单实例保护，不会出现双图标）。
- **在线更新**：基于 `tauri-plugin-updater`，面板「检查更新」按钮可一键下载安装并重启。CI 在配置 `TAURI_SIGNING_PRIVATE_KEY` 时自动把端点指向 GitHub Release 的 `latest.json`（详见构建指南 §6.6）；未配置时按钮显示「更新服务尚未配置」。**每次 OTA 都会换包，需重新放行并重新授权，见下方「每次更新后需重新授权」**。
- **模型余额（FR-8）**：多平台、多账号混排展示余额，60 秒自动刷新。支持 DeepSeek、Kimi（国内/国际）、智谱 GLM、阶跃星辰、Novita AI、OpenRouter、302.AI、AiHubMix，以及中转站自定义（OpenAI 计费接口 / new-api user quota）。可选 HTTP 代理，面板不展示明文 Key。
- **状态实时反映（FR-7）**：面板打开即读取系统真实状态，避免与系统设置不同步。
- **Swift sidecar（M4）**：本地 `127.0.0.1:9876` 承载蓝牙设备管理与开关。

## 安装

### 系统要求

- macOS 12.7.6（Monterey）或更高（主程序与 Swift sidecar 均按 12.7.6 部署目标编译，分发包为 universal 通用二进制，同时支持 Apple Silicon 与 Intel；台前调度开关仅 macOS 13+ 显示，其余功能不受影响）。注意最低版本精确到补丁号，**macOS 12.0–12.7.5 不受支持**；macOS 12 已停止接收 Apple 安全更新，建议尽量在受支持的系统版本上运行
- Apple Silicon / Intel 均可，分发包必须是 universal 二进制。`npm run tauri build` 只出当前架构，**分发请用 `./scripts/build-release.sh`**（默认 universal）；单架构包在另一种芯片上会弹「需要安装 Rosetta」

### 从分发包安装

1. **下载**。前往 [GitHub Release](https://github.com/samjoeyang/unoSwitchKit/releases)（或官网下载页）下载对应芯片的压缩包：Apple 芯片选 `arm64`、Intel 选 `x86_64`、不确定选 `universal`。

2. **解压并拖入应用程序**。若拿到的是分卷（`*.z01`、`*.z02` … `*.zip`），先合并再解压：

   ```bash
   zip -s 0 SwitchKit-0.4.8-universal.zip --out SwitchKit-full.zip
   unzip SwitchKit-full.zip     # 得到 SwitchKit.app
   ```

   单卷直接 `unzip SwitchKit-*.zip`。不要用第三方解压工具解分卷，容易丢 `.app` 包裹层。得到 `SwitchKit.app` 后，把它拖进 `/Applications`（「应用程序」文件夹，不要在压缩包内直接双击运行）。

3. **首次启动 / 每次换包后启动**。打开「应用程序」文件夹，**直接点 SwitchKit 图标打开**。分发包为 ad-hoc 签名、未公证，macOS 会先弹出安全警告，按下面顺序放行（**每个新下载包、每次在线更新后都要重做一次**；macOS 15 起已无「右键 → 打开」绕过）：

   - 双击打开后弹「未打开"SwitchKit"」→ 点「完成」关掉；
   - 打开「系统设置 → 隐私与安全性」，底部点「仍要打开」；
   - 再次确认弹窗点「仍要打开」；
   - 用触控 ID / 管理员密码授权，App 随即启动。
   - 若仍提示「已损坏，无法打开」，先移除隔离属性再重试：

     ```bash
     xattr -dr com.apple.quarantine /Applications/SwitchKit.app
     ```

4. 启动后**无 Dock 图标**，图标常驻菜单栏；点击菜单栏图标展开面板。

### 需要授予的权限

| 权限 | 授予位置 | 用途 | 不授予的后果 |
|---|---|---|---|
| 自动化（Finder） | 系统设置 → 隐私与安全性 → 自动化 | 清空废纸篓、废纸篓计数 | 操作报错，计数回退为直接枚举 |
| 自动化（System Events） | 同上 | 外观浅色 / 深色切换、屏幕清洁、关闭所有应用 | 这些操作静默失效（状态读取不受影响；切「自动」不需要它） |
| 完全磁盘访问权限 | 隐私与安全性 → 完全磁盘访问权限 | 清理 Safari 等受 TCC 保护的缓存 | 该目录被跳过，其余目录照常清理 |
| 文件和文件夹（桌面 / 文稿 / 下载） | 隐私与安全性 → 文件和文件夹 | 大文件扫描、应用占用统计 | 目录被跳过或扫描结果为空 |
| 蓝牙 | 首次扫描设备时系统弹窗 | 列出 / 连接 / 断开蓝牙设备 | 蓝牙模块列表为空或超时 |
| 管理员密码 | 操作时系统密码框 | 释放内存（`purge`）、刷新 DNS | 每次操作都需输入密码（凭据不落盘） |

完全磁盘访问需要**授权后重启 App** 才生效。屏幕录制、麦克风、辅助功能等权限当前版本尚未使用，后续若加入采集类功能同样受下面约束影响。

本轮的「外观自动切换 / 全局快捷键 / 截屏录屏入口」三项**不需要任何新增隐私授权**：自动外观只写系统偏好键，快捷键用 Carbon 热键，截屏与录屏均由系统 App 采集。因此换包后这三项不受 TCC 重新授权影响，可作为授权失效期的保底能力。

### 每次更新后需重新授权

分发包使用 **ad-hoc 签名**（项目既定选择：不申请 Developer ID、不做 Apple 公证）。ad-hoc 签名没有稳定的代码签名身份，每次重新构建、下载新包或在线更新，App 的 cdhash 都会变化；macOS TCC 会把它当作一个新 App，**之前授予的自动化、完全磁盘访问、文件和文件夹、蓝牙等隐私权限全部静默失效**。

更麻烦的是：系统设置里的开关**仍显示「开」**，而且通常**不会重新弹出授权窗口**（幽灵授权）。所以每次换包后请按下面流程做一次：

1. 完全退出 SwitchKit（面板底部「退出」）。
2. 在「终端」执行一行命令，清掉过期的授权记录：

   ```bash
   tccutil reset All com.switchkit.app
   ```

   `All` 重置本 App 的所有隐私权限；不带 `sudo` 只影响当前账户，大小写敏感。也可只重置单项，例如 `tccutil reset AppleEvents com.switchkit.app`、`tccutil reset ScreenCapture com.switchkit.app`。
3. 重新打开 SwitchKit。App 检测到「包被替换」时会在面板顶部显示「需要重新授权」提示，并提供「复制重置命令」「打开系统设置」「检查权限」入口。
4. 逐个点击对应功能，让系统重新弹出授权窗口并允许。若某个权限始终不弹窗，重启 macOS 后再试；屏幕录制、辅助功能、完全磁盘访问等系统级服务偶尔需要 `sudo` 或完全重启才能恢复弹窗。

> 顶部提示由「签名哈希变化」或「存在明确失效的必需权限项」触发。点「已处理」只记录当前签名，能消除「应用已更新」，**不会消除仍被判为未生效的项**——例如「完全磁盘访问」未授权时读取受保护目录直接返回 EPERM，且该项不会自动弹窗，必须在系统设置里手动添加。此时可点「**本次不显示**」暂时收起：仅当前运行有效，重启 App 或在设置页主动点「检查权限」后会再次显示。它只收起引导，不代表权限已恢复。`CGPreflightScreenCaptureAccess()` 一类预检在刚授权后可能返回旧值，必要时重启 App 再查。

> 可选依赖：蓝牙设备列表 / 连接走系统 `IOBluetooth`，无需额外安装；只有「蓝牙整机开关」需要 `brew install blueutil`。

### 开机自启

设置（齿轮）→ 开机启动。开启后写入 `~/Library/LaunchAgents/com.switchkit.app.plist`，自带单实例保护，不会出现双图标。

### 在线更新

设置 → 检查更新。CI 已配置 OTA 时可直接下载安装并重启；若构建时未配置更新端点，会提示「更新服务尚未配置」（配置方法见 [docs/构建指南.md](docs/构建指南.md) §6.6）。**更新换包后务必按上面「每次更新后需重新授权」处理**。

### 卸载

```bash
# 1. 先在 App 内关闭「开机启动」，然后退出 App
launchctl unload ~/Library/LaunchAgents/com.switchkit.app.plist 2>/dev/null
rm -f ~/Library/LaunchAgents/com.switchkit.app.plist
# 2. 删除应用
rm -rf /Applications/SwitchKit.app
# 3. 可选：清除配置与已保存的 Key
rm -rf ~/Library/Application\ Support/com.switchkit.app
```

## 目录结构

```
unoSwitchKit/
├── src/                     # React 前端面板
│   ├── App.tsx
│   ├── components/          # ToggleGrid / ResourcePanel / StatusDot
│   ├── ipc.ts               # 封装 Tauri invoke
│   └── types.ts
├── src-tauri/
│   ├── src/
│   │   ├── main.rs          # 入口：窗口/托盘/状态/命令注册
│   │   ├── tray.rs          # 托盘 + 面板定位
│   │   ├── commands.rs      # 所有开关/释放命令
│   │   ├── privilege.rs     # root 提升通道（osascript 密码框）
│   │   ├── cache_whitelist.rs # 缓存清理白名单 + 主目录限制
│   │   ├── store.rs         # 设置持久化 + 开机自启
│   │   ├── models.rs        # 结构化返回类型
│   │   └── util.rs          # shell/osascript 封装
│   ├── swift/               # Swift sidecar（M4）
│   ├── icons/               # 应用图标（已生成）
│   ├── capabilities/        # 权限声明
│   ├── Cargo.toml
│   ├── tauri.conf.json      # activationPolicy: accessory
│   └── build.rs
├── docs/                    # 需求文档 / 设计文档 / 构建指南
└── package.json
```

## 开发 / 构建

详见 [docs/构建指南.md](docs/构建指南.md)。快速开始：

```bash
# 1. 安装 Rust（含 stable + 双架构 target）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin

# 2. 安装前端依赖
npm install

# 3. 开发模式（前端热更新 + Rust 重新编译）
npm run tauri dev

# 4. 生产构建（universal-apple-darwin）
npm run tauri build
```

> 本仓库已在本机（macOS 12.7.6 / Rust 1.99）通过 `cargo check` 验证后端可编译；前端 `npm run build` 通过 `tsc` 类型检查。开发机版本与最低系统版本一致；完整 GUI 运行仍需在有图形界面的实体机上验证（本机无 GUI 显示）。

## 安全设计（NFR-3 / NFR-4）

- 缓存/文件删除**仅允许白名单目录**，且限制在主目录 `~` 内，拒绝任何越权绝对路径。
- 破坏性操作（清缓存 / 释放内存）前端走「预览 → 二次确认」流程。
- root 操作经特权通道，凭据不落盘、不进日志、不硬编码。
- 不调用私有 AppKit API；默认不联网、不上报。

## 已知约束

- 最低系统版本定为 `12.7.6`（Monterey）：与开发机一致便于自测，同时跨过 ScreenCaptureKit（12.3+）与 `CGPreflightScreenCaptureAccess()`（实际 11+）的门槛；原值 `10.14` 是受 Swift 守护依赖 Network 框架限制的历史下限。版本精确到补丁号，macOS 12.0–12.7.5 不受支持；分发包为 universal 通用二进制，同时支持 Apple Silicon 与 Intel 芯片。
- 蓝牙开关依赖第三方 `blueutil`（需 `brew install blueutil`），Swift sidecar 才会真正生效。

## 讨论群组与赞赏

### 交流群组

遇到问题、提建议、看更新动态，来 Telegram 群组找我们。扫码或搜索 **[@UNOSWITCHKIT](https://t.me/UNOSWITCHKIT)** 即可加入。

![SwitchKit Telegram 群组二维码](website/assets/telegram-qr.jpg)

### 赞赏支持

SwitchKit 完全免费、无广告、不收集数据。若它让你的 Mac 更好用，欢迎赞助，支持持续开发。

| 微信支付 | 支付宝 |
|---|---|
| ![微信赞赏二维码](website/assets/donate-wechat.jpg) | ![支付宝赞赏二维码](website/assets/donate-alipay.jpg) |

收款方式以二维码为准。
