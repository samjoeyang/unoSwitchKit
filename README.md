# SwitchKit

Mac 菜单栏快捷开关 + 系统资源释放工具。macOS 专属、常驻菜单栏、体积极小（目标：安装包 < 15MB、常驻内存 < 50MB）。

> 对标 One Switch（快捷开关）+ CleanMyMac（资源释放）的轻量合体。

## 技术栈

| 层 | 选型 |
|---|---|
| 桌面壳 | Tauri 2.0（Rust） |
| 前端面板 | React 18 + TypeScript + Tailwind + Vite |
| 原生补充 | Swift sidecar daemon（蓝牙 / 前台监听） |
| 特权执行 | `osascript` 密码框（MVP）→ `SMJobBless` helper（生产） |
| 持久化 | JSON 文件（AppConfig 目录，MVP 简化） |

## 功能

- **快捷开关（FR-2 P0）**：深色模式、隐藏桌面图标、阻止睡眠（caffeinate）、清空废纸篓、隐藏 Dock、显示隐藏文件、屏幕清洁（一键收起/恢复全部前台窗口）、台前调度（仅 macOS 13+ 显示）。
- **即时动作**：锁定屏幕、清空剪贴板、启动屏幕保护、关闭所有应用（优雅退出，跳过自身与 Finder）。
- **菜单栏图标收纳**：其他 App 的菜单栏图标可收进面板统一管理——默认收纳（隐藏），点亮的才显示（需辅助功能授权）。
- **蓝牙设备**：列出已配对设备，一键连接 / 断开（系统框架 IOBluetooth，经 Swift 守护，无第三方依赖）。
- **资源释放（FR-3 P0）**：释放内存（`purge`，需 root）、刷新 DNS（`dscacheutil`/`mDNSResponder`，需 root）、清理用户缓存（白名单 + 预览 + 二次确认）。
- **开机自启（FR-4）**：注册/移除 `~/Library/LaunchAgents/com.switchkit.app.plist`（单实例保护，不会出现双图标）。
- **状态实时反映（FR-7）**：面板打开即读取系统真实状态，避免与系统设置不同步。
- **Swift sidecar（M4）**：本地 `127.0.0.1:9876` 承载菜单栏图标收纳（AX API）、蓝牙开关与前台应用监听。

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

> 本仓库已在本机（macOS 12.7.6 / Rust 1.99）通过 `cargo check` 验证后端可编译；前端 `npm run build` 通过 `tsc` 类型检查。完整 GUI 运行需在 macOS 13+ 实体机。

## 安全设计（NFR-3 / NFR-4）

- 缓存/文件删除**仅允许白名单目录**，且限制在主目录 `~` 内，拒绝任何越权绝对路径。
- 破坏性操作（清缓存 / 释放内存）前端走「预览 → 二次确认」流程。
- root 操作经特权通道，凭据不落盘、不进日志、不硬编码。
- 不调用私有 AppKit API；默认不联网、不上报。

## 已知约束

- 开发机为 macOS 12（Monterey），低于 PRD 要求的最低 Ventura 13；`minimumSystemVersion` 暂设 10.13 以保证本机可构建，发布前应上调至 13.0。
- 蓝牙开关依赖第三方 `blueutil`（需 `brew install blueutil`），Swift sidecar 才会真正生效。
