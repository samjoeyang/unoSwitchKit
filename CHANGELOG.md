# Changelog

本文件记录 SwitchKit 实施进度，对应需求文档里程碑 M1–M4。

## [0.4.14] - 2026-11-13

### 新增：权限提醒可「本次不显示」

- 背景：部分权限项（典型是「完全磁盘访问」）不会自动弹窗、必须在系统设置里手动添加；被动探针持续判为 `denied`，顶部引导因此长期显示。「已处理」只写签名哈希，对这类场景无效。
- 顶部引导新增「本次不显示」：前端内存 state 收起整个 banner，**不写盘、不进 `permission-state.json`**，App 重启后自动恢复显示；在设置页主动点「检查权限」也会重新展开，避免检查结果的文案指向被隐藏的引导。
- 「已处理」的反馈文案改为如实说明其作用（记录签名，不消除未生效项）；引导底部补「重启 App 后再次提醒」。Rust 侧零改动、不新增权限、不新增依赖。
- README「每次更新后需重新授权」修正原「点『已处理』后消失」的过时表述。

### 调整：权限自检不再保留「屏幕录制（预留）」

- 从 `base_items()` 移除 `screen_capture` 项，权限自检列表由 6 项缩减为 5 项实际使用的权限（自动化 Finder / 自动化 System Events / 完全磁盘访问 / 文件和文件夹 / 蓝牙）。
- 一并删除 `CGPreflightScreenCaptureAccess()` FFI、`#[link(CoreGraphics)]` 与被动探针中对应的分支：启动期不再做任何屏幕录制 TCC 查询；`PermissionItem.required` 字段及前端两处过滤条件同步删除。
- 不影响「截屏录屏」功能：该入口调用系统截图工具栏，本就零 TCC。README / 文档 / 官网中关于 TCC 重置命令与系统工具栏的通用说明保留。Rust 侧不新增权限、不新增依赖。

### 调整：面板底栏按钮重排

- 底栏由「左：`[设置][排序][原生守护]` + 右：`[退出]`」改为左侧设置分组 `[设置][排序][退出]`（同一浅色圆角底），「● 原生守护」单独靠右。
- 三个按钮统一尺寸与 hover 反馈。纯前端，Rust 零改动。

## [0.4.13] - 2026-11-12

### 新增：开关卡片自定义排序

- 面板底栏新增「排序 / 完成排序」：进入编辑模式后 14 个开关卡片显示拖拽把手、只响应拖动而不触发开关动作，实时换位并自动保存；编辑条提供「恢复默认」，写入空数组即回默认顺序。
- 顺序复用 v0.1.1 起就存在但前端从未接线的 `Settings.toggle_order`（`models.rs` / `store.rs` 白名单已含），**Rust 侧零改动、不新增依赖、不新增权限、不涉及 TCC**。空数组 = 默认顺序；读取时过滤已删除 key、补齐新增 key；隐藏项（macOS 12 的台前调度）仍保留在顺序里，换机器后位置不丢。
- 拖拽用 Pointer Events 手写，不引入前端依赖、不依赖 WKWebView 的原生 HTML5 DnD；window 级 `pointermove` + ref 镜像避免闭包过期；命中判定按 `data-toggle-key` 而非 DOM 下标（隐藏项会让下标错位）；`#root` 上下边缘自动滚动，可跨屏换位。
- `save_settings` 是整对象覆盖，写入前重新读取配置，避免用过期快照覆盖其它组件刚写入的 API Key / 排除列表。
- 边界：只做「拖拽换格子 / 排序」，不做自由 x/y 坐标、跨区块拖动、隐藏或增删卡片。
- 文档：README 功能列表、官网预览说明与版本更新说明同步（官网由 `deploy-pages.yml` 单独发布）。

### 修正：系统截图入口同时覆盖录屏，UI 与文档文案对齐

- 实测确认 `open -b com.apple.screenshot.launcher` 拉起的工具栏**同时含静态截图与屏幕录制两组按钮**（Apple 支持 102618；macOS 26 使用手册 mh26782 明列「捕捉整个屏幕 / 捕捉窗口 / 捕捉屏幕的一部分 / 录制整个屏幕 / 录制窗口 / 录制屏幕的一部分」）。本机 macOS 12.7.6 的 `Screenshot.app` 自带 `com.apple.private.screencapture.allow` 与 `com.apple.private.tcc.allow = [kTCCServiceScreenCapture]`，因此录屏同样零 TCC。
- 面板按钮与快捷键动作标签同步体现「截屏 + 录屏」双能力；命令名 `open_system_screenshot` 与动作 id `open_screenshot` 保持不变，用户已保存的快捷键绑定不受影响。
- 文案定名：功能名统一为「截屏录屏」（面板按钮）/「截屏录屏工具栏」（快捷键动作与官网、README 描述）；内部开关 key `system_screenshot` 与上述命令名、动作 id 均不改，避免影响已持久化的绑定。
- 评估修订：「一键录屏」的零权限入口已随 0.4.12 交付，无需另立批次；仅「跳过系统工具栏、按下即开始录制」的静默自采仍需屏幕录制 TCC 与跨 12–27 版本分支，暂缓。
- README / 官网功能卡与版本说明同步。

## [0.4.12] - 2026-11-12

### 新增：三项零 TCC 功能（外观自动切换 / 全局快捷键 / 系统截图入口）

本轮只落地评估中没有争议、不依赖隐私授权的部分；静默截屏、一键录音、一键录屏、媒体键等仍需讨论，暂缓。

- **外观三态（浅 / 深 / 自动）**：`commands.rs` 新增 `get_appearance_mode` / `set_appearance_mode` / `cycle_appearance_mode`，先读 `AppleInterfaceStyleSwitchesAutomatically` 再读 `AppleInterfaceStyle`（本机 12.7.6 实测两键可共存，旧实现会把 Auto 下的深色误报为手动深色）。浅/深沿用 System Events AppleScript；自动只写系统偏好键并补发 `AppleInterfaceThemeChangedNotification`（CoreFoundation distributed center，尽力即时生效），由 macOS 按日出/日落或夜览日程切换，不申请定位、不常驻天文计算。面板按钮改为三态循环，另用 30 秒轻量轮询跟随系统自动切换。
- **全局快捷键**：新增 `tauri-plugin-global-shortcut = "2"`（解析到 2.4.1 / global-hotkey 0.8.0），`rust-version` 1.77 → 1.90（官方插件最低要求）。新模块 `shortcuts.rs` 提供 9 个零新增权限动作的白名单、持久化绑定（`Settings.shortcut_bindings`）、启动后台恢复与逐项注册结果。只接受「修饰键 + 普通键」，媒体键 / 单键被排除（会落到 CGEventTap、需要输入监控类 TCC）；绑定冲突、被系统占用会逐条提示。注册/注销在 macOS 上由插件 dispatch 到主线程并同步等待，因此一律走 `spawn_blocking` 与启动后台线程，避免主线程死锁。
- **系统截图入口**：`open_system_screenshot` 用 `/usr/bin/open -b com.apple.screenshot.launcher` 启动系统截图 App（12.7.6 实测 bundle id），带名称 / 路径兜底。**不 spawn `/usr/sbin/screencapture`**：该 CLI 带 `com.apple.private.tcc.check-allow-on-responsible-process`，TCC 会检查调用者，未授权时静默产出黑图。本应用不读取屏幕内容、不保存截图文件、不申请屏幕录制权限。
- 前端：面板新增「系统截图」按钮与外观三态显示，设置页新增「日出日落自动切换」说明与 `ShortcutSection`（按键录制、显示 ⌘⇧S 格式、清除、冲突反馈）。
- 文档：`README.md` 功能列表、权限表与「零新增权限」说明；`website/index.html` 功能卡（线上站点走 gh-pages 分支，由 `deploy-pages.yml` 单独发布）。
- 验证：`cargo check` 无警告、`cargo test` 21/21 通过（新增 4 个快捷键解析 / 白名单与外观循环测试）、`npx tsc --noEmit` exit 0、`npm run build` 通过。未改功能权限声明（capabilities）。

### 新增：权限自检与「换包后重新授权」引导

- 背景：项目确定长期使用 ad-hoc 签名、不申请 Developer ID / 不做公证，并接受「每次换包重新授权」。ad-hoc 每次构建产生新 cdhash，TCC 把新包当作新 App，隐私授权静默失效且系统设置开关仍显示「开」、不会重新弹窗。
- 新增 `src-tauri/src/permissions.rs`：用 `/usr/bin/codesign -dv --verbose=4` 读取当前可执行文件 CDHash，与 `app_config_dir()/permission-state.json` 中上次记录比对，判断「包是否被替换」。首次运行自动建档，因此即使从未点过「已完成重置」，之后的 OTA 也能被检测到；OTA `relaunch()` 后无需额外埋点即会触发提示。
- 提供 5 个命令：被动自检（零副作用：`CGPreflightScreenCaptureAccess()` + 完全磁盘访问目录读取）、主动自检（AppleEvents 探测带 3 秒超时，避免授权弹窗卡死面板）、记录当前签名、复制 `tccutil reset All com.switchkit.app` 到剪贴板（`pbcopy`）、打开系统设置隐私面板。主动探测只在用户点击时执行，避免启动即弹窗。
- 前端新增 `PermissionBanner`：换包或检测到失效项时在面板顶部显示，列出逐项状态（已授权 / 未生效 / 未检查），提供复制命令、打开系统设置、检查权限、已完成重置四个操作；设置页新增「权限自检」入口与结果摘要。

### 文档：明确长期 ad-hoc 分发的授权成本

- `README.md` 首启步骤改为「每个新包、每次在线更新都要重做」，权限表补自动化（System Events）与文件和文件夹，新增「每次更新后需重新授权」小节（含 `tccutil reset All com.switchkit.app` 完整流程）。
- `docs/构建指南.md` §3.1 明确不申请 Developer ID / 不公证的既定决策，去掉过时的「右键 → 打开 / macOS 13+ 走系统设置」，新增 Release 说明固定段落；§4 / §5.1 / §6.6 同步修正，§6 标注「本项目不采用」。
- `scripts/build-release.sh` 的签名注释、构建输出与 zip 提示同步更新。
- 官网 `website/index.html` 同步：去掉两处「只需一次」，权限表补自动化（System Events）与文件和文件夹，新增「每次更新后需重新授权」说明与 `tccutil` 复制块。线上站点走 gh-pages 分支，由 `deploy-pages.yml` 发布。

### 工程：最低系统要求提升至 macOS 12.7.6

- 统一最低系统版本为 `12.7.6`（Monterey），消除此前 `tauri.conf.json` 写 `10.14`、PRD 写「Ventura 13」、开发机为 12.7.6 的三方矛盾；开发机现在正好等于最低版本，本地可自测。
- 同步修改：`src-tauri/tauri.conf.json`（`bundle.macOS.minimumSystemVersion`）、`scripts/build-release.sh`（`MACOS_MIN` 与 Swift 切片 target）、`.github/workflows/build.yml`（`MACOSX_DEPLOYMENT_TARGET`）、`README.md`、`docs/需求文档.md`、`docs/设计文档.md`、`docs/构建指南.md`、`website/index.html`。
- 构建侧核实：Tauri 打包器把 `minimumSystemVersion` 字符串原样写入 Info.plist 的 `LSMinimumSystemVersion`，不解析不截断；本机 Xcode 14.2 / Swift 5.7.2 实测 `swiftc -target x86_64-apple-macosx12.7.6` 可编译，产物 `LC_BUILD_VERSION.minos = 12.7.6`，与脚本中的字符串硬校验一致。
- 兼容性影响：版本精确到补丁号，**macOS 12.0–12.7.5 及更早系统不再受支持**；台前调度的 macOS 13+ 运行时判断必须保留（12.7.6 用户会走到该分支）。macOS 12 已停止接收 Apple 安全更新，建议用户尽量运行在受支持的系统版本上。
- 本次只改构建配置与文档，未改功能代码。官网改动由 `deploy-pages.yml` 单独发布。

### 修复：内存数字口径与页大小

- 「释放内存」的 before/after 改用与资源面板一致的 sysinfo 口径（active + wired + compressor + speculative，**不含** inactive 可回收缓存）。同一时刻不再出现「面板 8.9 GB / 日志 12.56 GB」这类相差数 GB 的数字。
- 释放结果改显示「回收缓存 X GB」+「已用（不含缓存）before → after」；`MemoryStat` 新增 `cached_mb` 记录 inactive 页数，便于衡量 `purge` 的实际效果。
- `parse_memory()` 不再硬编码 4096 页大小，改从 `vm_stat` 首行读取（Apple Silicon 16384 / Intel 4096），并以 `sysctl -n hw.pagesize` 兜底。修复 aarch64 构建上释放日志数字只有真实值 1/4 的问题。
- `free_mb` 不再重复累加 speculative（`free_count` 已包含它）。
- 新增 vm_stat 解析、页大小换算与口径一致性的 Rust 单元测试。

### 修复：面板「原生守护」再次不亮（启动竞态 + OTA 包未签名）

**现象**：0.4.11 装机后，11-11 14:47 启动 App，底部「原生守护」一直灰；同一台机 16:59 重启 App 后立刻点亮。

**日志证据**（`~/Library/Logs/switchkit-rust.log` + `switchkit-daemon.log`）

- 14:47:56 / 14:47:57 两次 `try_spawn_daemon` 都是 `alive=true` 但 9876 在 ~1s 内未 listen；守护日志该时段完全空白，连 `=== SwitchKit sidecar launch ===` 都没有 → 进程卡在 Swift 第一行之前的 exec / 代码签名评估阶段，不是被系统拒绝后退出。
- 16:59:29 同一路径、同一二进制 spawn 后 <1s 就绪 → 系统已完成首次评估并缓存。
- `xattr -d com.apple.quarantine` 全部返回「属性本就不存在」，端口也无残留 → 排除 0.4.2 的隔离拦截、0.4.5 的探测误报、旧守护占端口这三类已修过的失效。

**根因**：`spawn_sidecar()` 只在 setup 跑一次，就绪窗口固定 ~1s，失败即 kill 且整个 App 会话不再重试。任何一次偶发的首次执行评估变慢，都会让守护永久不亮，只能手动重启 App。历次修复各补了一个具体环节，但没有消除「一次性启动、无自愈」这个结构缺陷 —— 所以不是旧 bug 复活，是同一条单点链路被新诱因再次打断。
- 加剧因素：CI 在 `codesign --force --deep --sign -` **之前**由 Tauri 生成 `.app.tar.gz`，OTA 落地的 `.app` 完全未签名。实测 `/Applications/SwitchKit.app` 与内嵌 `switchkit-daemon` 均为 `code object is not signed at all`，与项目既定 ad-hoc 策略不符；Intel 上首次 exec 会被系统策略评估拖慢，Apple Silicon 上未签名 Mach-O 会被 AMFI 直接拒绝。手动下载的 zip 在签名后打包，不受影响。

**改动**

- `src-tauri/src/main.rs`：就绪窗口 ~1s → ~9s（60 × 150ms，正常 <1s 立即返回）；spawn 移到后台线程，不阻塞面板出现；新增 `SPAWN_LOCK` 串行化启动、`start_sidecar_watchdog()` 每 5s 探 9876，连续两次失败且距上次 spawn ≥12s 自动重新拉起；`stop_sidecar_watchdog()` 在退出时停狗，避免清端口后又把守护拉起。
- `src-tauri/src/commands.rs`：`shutdown_sidecar` 先停看门狗再清端口。
- `.github/workflows/build.yml`：thin 两架构在 codesign 之后重打 `SwitchKit.app.tar.gz` 并用 `tauri signer sign` 重签 `.sig`；新增「嵌套守护未签名则拒绝产出 OTA 包」硬校验，保证 OTA 与 zip 一样带 ad-hoc 签名。
- 前端状态点已有 5s 轮询，无需改动；自愈后最多约 20s 自动点亮，不必手动重启 App。

**验证**：`cargo check`、`cargo test`（17 项全通过）、`npx tsc --noEmit` 均通过。`build.yml` 的 OTA 重签与嵌套守护硬校验在 0.4.12 发版时实跑确认。

## [0.4.11] - 2026-11-09

### 优化：资源面板布局与快捷开关归位

- 「释放内存」按钮收进「内存」卡片内部，进度条、按钮、释放结果反馈集中在一处；监控数据未就绪时显示 `—`，面板不再跳动。
- 「刷新 DNS」从资源面板底部按钮组移入快捷开关网格，排在「关闭所有应用」之后，使用完全相同的卡片样式（含状态点与副标题）；操作后短暂显示「已刷新」/「刷新失败」，3 秒自动还原。
- 网络上下行速率合并为**一个**面板，两个速率用 ` / ` 分隔（下行绿 / 上行蓝）；CPU 面板与网络面板各占一半、同一行；内存面板保持整行。
- 半宽卡片加 `min-w-0` 防 320px 面板溢出，网络面板与 CPU 卡片视觉对齐。

### 工程

- 新增 `.nvmrc` 固定开发环境 Node 24，与 CI 的 `node-version: 24` 对齐。
- 核实本机开发环境实际为 Node 24.15.0 / npm 11.12.1（此前记录的 22.22.2 为工具沙箱注入的 Node，非用户终端）；Node 24 下 `npm run build` 通过，产物与 Node 22 下一致。

## [0.4.10] - 2026-11-09

### 新增：菜单栏系统资源实时监控（CPU / 内存 / 网络上下行）

- 新增「资源」面板，实时显示三项指标：
  - **CPU 使用率**：基于 sysinfo 0.33 采样刷新，进度条呈现。
  - **内存占用**：已用 / 总量（MB），进度条呈现。
  - **网络上下行速率**：基于 `netstat -ib` 间隔采样计算 ↓下行 / ↑上行，自动在 KB/s ↔ MB/s 间切换。
- 前端每 2 秒轮询一次 `get_system_stats`，面板隐藏时自动暂停轮询，降低开销。
- 后端新增 `SystemMonitor`（sysinfo 采 CPU/内存 + 原生 netstat 采网络）与 `get_system_stats` 命令，`SystemStats` 结构体贯穿 Rust 命令层与前端 IPC 类型层。

## [0.4.9] - 2026-11-09

### 优化：清理功能 12 项增强（安全 / 覆盖 / 体验）

- **P0 安全与正确性**
  - 修正结果「释放」措辞：清理只是先移入 `~/.Trash`，并未真正释放空间。结果卡改为「已转移 X 至废纸篓」，并新增「清空本应用移入的废纸篓项」按钮。
  - 大文件扫描（`walk_large`）跳过 `Mobile Documents` 目录与 `.icloud` 占位文件，避免递归 `Documents/Desktop` 时触发 iCloud 云端下载（流量 + 误下载）。
  - 二次确认文案按所选最高风险动态变化：含「危险」项（iOS 备份 / 应用残留）或「谨慎」项（App 缓存等）时分别给出醒目警告。
- **P1 覆盖与精度**
  - 补全虚拟环境扫描：新增 `~/.conda/envs` 静态项 + `scan_pyenv_envs()` 动态扫 `~/.pyenv/versions/*/envs/*`。
  - `scan_dev_envs` 深度由 ≤2 放宽至 ≤3 并加 deadline 限流，覆盖 `apps/web/node_modules` 这类 monorepo，且不再卡扫描。
  - `bundle_usage` 占用排行纳入 home 一级隐藏目录（`.cache`/`.ollama`/`.cargo`/`.npm` 等数十 GB 模型与依赖缓存），并跳过 `.Trash` 防自循环。
  - 下载过期天数可配置（`downloads_expire_days`，定时页可设，默认 30）；`scan_downloads_expired` 递归子目录、仅列文件防父子重复计数。
- **P2 体验与工程**
  - 持久化排除列表 `settings.exclude_paths`（对应 `--keep`）：清理项新增「⊘」一键排除，下次扫描不再出现。
  - 回收站管理：每次清理写 `~/.Trash/switchkit-manifest.json`，新增「回收站」标签页，支持一键恢复回原路径与清空本应用移入项。
  - 体积统计超时近似 `dir_size_deadline`：大目录超时即返回近似，避免扫描卡顿。
  - 抽出 `is_auto_cleanable` 单一判定，后端自动清理与前端默认勾选共用，消除两处逻辑漂移。
  - `recreate_after_clean` 排除 dev 类，venv/node_modules 移走后不再重建无意义空目录。

### 工程

- `cargo test cleanup` 5 passed（含新增 `is_excluded` / `is_auto_cleanable` 单测）。
- `docs/cleanup-manifest.md` 同步更新（pyenv/conda-base 标 🆕、node_modules 深度改 3、iCloud 防护、第二轮增强章节）。

## [0.4.8] - 2026-11-09

### 扩展：清理纳入 `~/Library/Caches` 全量用户缓存

- 背景：原静态白名单仅覆盖少量逐项列出的缓存目录（Safari/Chrome/dyld…），`~/Library/Caches` 下绝大多数缓存（Homebrew、VSCode ShipIt、iCloud、clang、Google 等，总量可达数 GB）从未被本功能收录，只能借助第三方清理工具处理。
- 改动：`cleanup.rs` 新增 `scan_user_caches()`，动态枚举 `~/Library/Caches` 顶层项并作为「用户缓存」分类呈现（与 CleanMyMac 的「用户缓存文件」类别一致）。
  - 风险定为 `Caution`：缓存可重建，但内容混杂含部分 App 缓存，故仅呈现、需手动勾选、绝不自动清理，也不会被默认勾选（与「安全」级静态缓存区分）。
  - 去重：跳过已在静态白名单逐项列出的缓存末段名，及本应用自身缓存 `com.switchkit.app`，避免重复/自清理。
  - 过滤：仅展示 ≥ 1MB 的顶层项以减少零碎噪音，按体积降序。
  - `CleanupPanel.tsx` 的 `CATEGORY_ORDER` 新增 `syscache`（位于「系统缓存」之后）；`models.rs` 分类注释同步补充。
- 效果：清理面板可直接勾选并清理 `~/Library/Caches` 下任意用户缓存（仍先移入废纸篓、可恢复），不再依赖外部工具。

### 修复：清理列表长标签换行

- `CleanupPanel.tsx` 清理项标签过长时换行导致右侧文件大小被遮盖；改为标签列 `min-w-0 break-all` 允许内部换行、风险徽章 `shrink-0` 不被挤压、大小列 `self-start` 顶部对齐，长标签不再遮挡「大小」。

## [0.4.7] - 2026-11-04

### 修复：清理执行无反馈（不知道是否成功）

- 根因：`CleanupPanel.doClean()` 把结果文案写入 `msg` 后紧接着 `await doScan()`，而 `doScan()` 开头就 `setMsg("")` —— 结果提醒被自己清空；执行期间按钮只变灰、无文案无 spinner，列表也无进行态。
- 改动：结果状态（`result` / `resultError`）与通用提示 `msg` 解耦，清理后走静默重扫 `scanCleanup(true)`，不再清空结果；新增执行中 spinner、顶部不确定进度条与列表遮罩；新增常驻结果卡（释放空间、成功项数、失败项可展开查看原因、手动关闭）；二次确认提示改为琥珀色强调。
- `src/index.css` 新增 `cleanup-spin` / `cleanup-indeterminate` / `cleanup-result-in` 动效，并兼容 `prefers-reduced-motion`。

### 修复：升级版本后余额 apikey 丢失

**背景**

- 配置持久化（`settings.json`，位于 `~/Library/Application Support/com.switchkit.app`）长期没有版本化迁移层，仅靠一个硬编码的 `#[serde(alias = "deepseek_keys")]` 兜底。
- 旧 `load_settings` 用 `serde_json::from_str::<Settings>()` 一次性整体反序列化：只要文件里**任一个已知字段**的类型与新结构不符（schema 漂移，如历史遗留字段、类型重构），整个解析就失败并回退 `Settings::default()`，于是同一文件里的余额 apikey 等所有设置被「连坐」清空——表现即「升级后 apikey 消失」。
- 实证：本机现存的 `settings.json` 仍携带 `menubar_managed` 等当前结构体早已删除的字段，证明旧 schema 在无迁移的情况下长期漂移。

**改动**

- `src-tauri/src/store.rs`：`load_settings` 改为**逐字段容错读取**（抽取为纯函数 `settings_from_value`），任一字段解析失败只跳过该字段，绝不连坐清空；余额 Key 同时兼容 `provider_keys`（当前）与 `deepseek_keys`（v0.3.8 旧名）。新增 `normalize_settings` 在启动期调用一次，把旧字段名归一、剔除未知遗留字段并写回规范化文件（仅在确有差异时写盘）。
- `src-tauri/src/main.rs`：`setup` 中调用 `store::normalize_settings`。
- 新增回归测试：`sibling_unknown_field_does_not_wipe_keys` / `known_field_type_mismatch_keeps_keys` / `legacy_deepseek_keys_survive_with_unknown_fields` / `invalid_json_returns_none`。

**效果**：此后任何版本升级都不会因 schema 漂移而丢失余额 apikey；已规范化的配置文件不再残留死字段。

### 重构 + 扩展：「清理缓存」升级为「清理无用文件」体系

**背景**

- 原 `清除缓存` 仅覆盖 8 个浏览器/系统缓存目录，且只能「全清」，无法按项选择、无法覆盖 App 残留/开发缓存/Xcode 等高频垃圾，也无自动清理。
- 本次按 P0→P3 顺序补齐「清理无用文件」范围，并统一接入 NFR-3 的白名单 + 预览 + 二次确认 + 主目录限制安全范式，新增「先移入废纸篓」安全网。

**改动**

- `src-tauri/src/cleanup.rs`（新增，替代已删除的 `cache_whitelist.rs`）：统一扫描/清理/分析/自动清理。
  - 静态白名单大幅扩充并按风险分级：`browser`/`system`（safe）、`app`（caution）、`dev`/`xcode`/`logs`（safe，Xcode 含 DerivedData/模拟器）、`ios` 备份（danger）。
  - 动态扫描：应用残留（`Containers`/`Group Containers`/`Application Support`/`Preferences` 中本机已无对应 .app 的 reverse-DNS 孤儿项，danger）、下载过期文件（Downloads 中修改早于 30 天，caution）。
  - **P0 选择性勾选**：`scan()` 返回全部项，`clean(ids)` 仅清理勾选项；风险分级标注（安全/谨慎/危险）。
  - **P1 废纸篓安全网**：所有清理先 `move_to_trash()` 移入 `~/.Trash`（文件名加时间戳避免冲突），缓存类目录清理后重建空目录，避免依赖它的应用崩溃；绝不直接硬删。
  - **P1 应用残留清理**：`scan_leftovers()` 比对已安装 App 的 bundle id（枚举 /Applications、~/Applications）识别孤儿项。
  - **P2 Xcode/开发缓存**：DerivedData、CoreSimulator、Cargo/npm/pip 等纳入白名单；`bundle_usage()` 按文件夹聚合 `~/Library` 下体积 Top 25（空间占用排行）。
  - **P3 大文件扫描**：`large_files(threshold)` 扫描常用目录 >= 阈值的大文件，按「大小 + 头部/尾部采样哈希」标记疑似重复（FNV-1a，限时 8s/8000 项防卡顿）。
  - **P3 自动清理**：`run_auto_clean()` 仅清理 safe 级（排除 Xcode），由 `run_auto_clean_if_due()` 按设置周期执行；`main.rs` 启动后台线程（启动 ~60s 首检 + 每小时）调用，并持久化 `last_auto_clean_ts`。
- `src-tauri/src/models.rs`：移除旧 `Cache*` 类型，新增 `CleanItem`/`CleanPreview`/`CleanResult`/`BundleUsage`/`LargeFile` 与 `CleanRisk` 枚举；`Settings` 增加 `auto_clean_enabled`/`auto_clean_interval_days`/`last_auto_clean_ts`。
- `src-tauri/src/commands.rs` + `main.rs`：`scan_cleanup`/`clean_selected`/`bundle_usage`/`large_files`/`trash_path` 替代旧 `preview_cache_cleanup`/`clear_cache`；`main.rs` 注入自动清理定时器线程。
- `src/components/CleanupPanel.tsx`（新增）：三标签页「清理 / 分析 / 定时」。清理页按分类分组、逐条勾选、风险徽章、两步确认、废纸篓提示；分析页展示空间占用排行与大文件/疑似重复（可单项移入废纸篓）；定时页开关自动清理与周期。
- `src/components/ResourcePanel.tsx`：移除旧缓存卡，仅保留释放内存 / 刷新 DNS。
- `src/ipc.ts` / `src/types.ts` / `src/App.tsx`：对接新 API 与类型，主面板渲染顺序调整为 开关 → 资源 → 清理 → 蓝牙 → 余额。

**效果**：清理能力从「8 个固定目录全清」扩展为「可勾选、分级、可回收、可分析、可定时」的完整无用文件治理体系，且全程不越权、可恢复。

## [0.4.6] - 2026-11-03

### 优化：消除蓝牙能力在守护启动后的 ~15s 冷启动空白（macOS 27）

**背景**

- 0.4.5 已修复面板误报「原生守护未运行」（`ping` 存活判定 + 蓝牙状态独立探测），守护现已点亮。
- 但蓝牙模块仍有残留：macOS 27 上 `IOBluetooth` 首次调用需 ~14.5s 冷启动（`bluetoothd` XPC 预热），回暖后全部 0ms。此前 `bluetooth:status` / `bt:devices` 在请求路径同步调用 IOBluetooth，前端探测超时即空白，且 `BluetoothSection` 仅在挂载/可见性变化时拉取，需手动隐藏/显示面板才出现设备列表。

**改动**

- `bridge.swift`：新增蓝牙结果缓存 + `btKickoff()`。守护监听就绪后即后台预热 `bluetoothd` 并填充缓存；`bluetooth:status` / `bt:devices` 改为只读缓存（0ms，永不超时），未就绪返回 `warming:true`；后台每 10s 定时刷新保持新鲜。IOBluetooth 调用全部移出请求路径（仅后台队列串行执行，避免并发）。
- `main.swift`：监听就绪后调用 `btKickoff()` 触发预热。
- `BluetoothSection.tsx`：新增 5s 轮询，预热完成后设备列表自动出现，无需手动隐藏/显示面板。

**效果**：蓝牙状态/设备列表在守护启动后约 15s 内自动填充并常驻更新；请求路径不再因蓝牙冷启动挂起或超时。

## [0.4.5] - 2026-11-03

### 修复：面板「原生守护」在守护实际运行时仍报「未运行」（macOS 27）

**背景**

- 0.4.4 诊断日志证实：Apple 芯片 + macOS 27 上守护进程正常 spawn、9876 监听就绪，但面板仍显示「原生守护：未运行」。
- 根因在状态探测层而非 spawn：`sidecar_status` 用 `{"cmd":"bluetooth:status"}` 判定存活，该命令会调用 `IOBluetooth`（`IOBluetoothHostController.default()` / `powerState`）；在 macOS 27 上该调用可能变慢/挂起（蓝牙权限或 bluetoothd XPC），超过 800ms 读取超时，导致面板误判守护「未运行」，但守护其实活着。

**改动**

- `src-tauri/src/commands.rs`：`sidecar_status` 改为「先 `ping` 判定存活（不碰 IOBluetooth，不会挂起），再独立探测 `bluetooth:status`（独立连接 + 2500ms 超时，失败仅 `bluetooth` 置空）」。守护活着即显示「运行中」，与蓝牙探测解耦。
- `src-tauri/swift/bridge.swift`：守护对每条请求记录 `recv / dispatch / respond(耗时)`，便于下次回传日志确认 `bluetooth:status` 是否挂起。
- `src-tauri/swift/main.swift`：修复日志重复落盘——`skLog` 仅写 stderr，由 Rust 侧统一重定向到 `switchkit-daemon.log`（此前 stderr 与文件各写一遍导致每行重复）。
- `src-tauri/src/main.rs`：修正 `strip_quarantine` 日志误标（exit 0 = 已移除隔离属性，非「不存在」）。

## [0.4.4] - 2026-11-03

### 诊断：原生守护全生命周期日志插桩（定位「守护不亮」根因用）

**背景**

- 0.4.2/0.4.3 连续两个版本在 Apple 芯片真机上「原生守护不亮」，但沙箱（Intel macOS、无 GUI）无法复现，根因推断反复被环境差异误导。核心盲区：GUI 启动（双击 / LaunchPad / 登录项）的 App 进程无 tty，`stderr` 被丢弃，且此前守护从未真正落盘日志，导致「守护有没有被拉起、卡在哪一步」在真机上完全不可见。

**改动**

- `src-tauri/src/main.rs`（Rust 侧，主程序）：新增 `switchkit-rust.log` 插桩，覆盖 `spawn_sidecar` 全链路——候选路径是否存在、去隔离（xattr）结果、spawn 是否成功（记录 pid）、9876 在 ~1s 内是否就绪、未就绪时进程是否还活着；并把守护的 `stderr` 重定向到 `switchkit-daemon.log` 统一收集。
- `src-tauri/swift/main.swift`（守护侧）：所有关键生命周期事件（启动、argv、bind/listen 结果、父进程看门狗）同时写 `stderr` 与 `~/Library/Logs/switchkit-daemon.log`（此前代码声称写该文件但实际未实现）。
- 日志位置：`~/Library/Logs/switchkit-rust.log`（主程序）+ `~/Library/Logs/switchkit-daemon.log`（守护）；配合系统 `log show --predicate 'process == "amfid" || process == "syspolicyd"'` 可三角定位是否被 AMFI/Gatekeeper 在 exec 瞬间拦截。

**目的**

- 本版本不发功能修复，仅作为「带诊断日志的观测版」发到线上，由用户在真机手动安装测试，用两份日志回传定位 0.4.2/0.4.3 的根因，再据此做针对性修复。

## [0.4.3] - 2026-11-01

### 修复：设置面板显示的版本号与真实版本脱节

**问题**

- 0.4.2 的「升级后版本仍显示 0.4.1」：面板右上角与「检查更新」里的版本号写死在 `src/App.tsx` 的 `const APP_VERSION = "0.4.1"`，发版时只改了 `tauri.conf.json`/`Cargo.toml`/`package.json`/`CHANGELOG`，漏了前端这个常量。实际安装包（含 OTA 升级后的二进制）版本已是真实的 0.4.2，所以「再检查更新」会提示已是最新——只是 UI 标签没跟着变。

**修复**

- `src/App.tsx`：移除硬编码 `APP_VERSION`，改为运行时用 Tauri 的 `getVersion()` 读取二进制真实版本（`CFBundleShortVersionString`）。以后升版本无需再手动同步前端常量，杜绝此类脱节。
- 升 0.4.3 换下载链接 + 走 OTA 自动更新，把修复推下去。

## [0.4.2] - 2026-11-01

### 修复：下载的包「原生守护」点不亮（Gatekeeper 隔离拦截）

**问题**

- 0.4.1 及之前：从 CI/Release/邮件下载的包（本仓库无 Developer ID 证书，只能 ad-hoc 自签名）被系统打 `com.apple.quarantine`，Gatekeeper 在 App 拉起 `switchkit-daemon` 这个 sidecar 时拦截它，端口 127.0.0.1:9876 起不来，面板「原生守护」始终不亮。dev/本地构建不带隔离属性，故表现正常。
- 关键认知修正：`std::fs::copy` 在 macOS 上**会**把源文件的 xattr（含 quarantine）一并拷到目标，所以复制后必须显式去隔离；且 GUI 启动（双击/LaunchPad/登录项）的 App 进程 PATH 仅 `/usr/bin:/bin`，裸 `xattr` 解析不到，须用绝对路径 `/usr/bin/xattr`。

**修复**

- `src-tauri/src/main.rs`：`spawn_sidecar` 拉起守护前先 `prepare_daemon_macos`——把 `switchkit-daemon` 复制到临时目录，对**源与副本各做一次** `xattr -d com.apple.quarantine`（绝对路径，defense-in-depth），再 `chmod 0o755` 兜底，最后从临时副本 spawn。用户无需手动 `xattr`。
- 主程序本身仍是 ad-hoc，需首次右键「打开」批准（固有行为）；守护这一步已自动处理。
- 本次升 0.4.2 仅换下载链接 + 走 OTA 自动更新，规避「覆盖同版本号发布不改 URL 导致浏览器缓存旧包」的问题。

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
