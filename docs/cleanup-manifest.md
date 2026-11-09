# SwitchKit 统一文件清理清单

> 整合 **App 内置清理**（`src-tauri/src/cleanup.rs` 的 `STATIC_TARGETS` + 动态扫描）与 **macos-system-cleaner**（`categories.md`）两份清单，统一为一份尽可能全面的 macOS 文件清理目录清单。
> 本清单即 App「清理无用文件」面板的全部扫描目标来源。

## 图例

- ✅ 已内置（原有）
- 🆕 本次新增（v0.4.8+，已写入 `cleanup.rs`）
- ⏳ 待评估（当前架构不支持：需 sudo 的系统级目录 / Docker 命令式清理 / 未实现的 glob 动态扫描）

## 风险与自动清理策略

- 风险等级：`safe`（可放心清）/ `caution`（删后不可自动恢复，需手动勾选）/ `danger`（体积巨大，绝不自动清理）。
- 自动清理只动 `safe` 级且排除 `xcode` 分类。
- 所有项先移入 `~/.Trash`（前缀 `switchkit-`）可恢复；路径强制限制在主目录内（`is_under_home`）；无系统级 sudo、无 Docker 命令式硬删。

---

## A. 浏览器 / 系统缓存

| 目标 | 路径（~ = $HOME） | 风险 | 状态 |
|---|---|---|---|
| Safari 缓存 | `~/Library/Caches/com.apple.Safari[.SafeBrowsing]` | safe | ✅ |
| Chrome / Edge / Brave / Arc / Opera / Firefox / WebKit | `~/Library/Caches/<bundle>` | safe | ✅ |
| dyld 共享缓存 | `~/Library/Caches/com.apple.dyld` | safe | ✅ |
| QuickLook / helpd / Spotlight / 字体 | `~/Library/Caches/com.apple.{QuickLook*,helpd,Spotlight,fonts}` | safe | ✅ |
| 用户缓存总览（≥1MB 顶层项） | `~/Library/Caches/*` | caution | ✅（syscache 动态兜底） |
| 系统缓存（需 sudo） | `/Library/Caches`、`/System/Volumes/Data/Library/Caches` | system | ⏳ |

## B. 日志 / 诊断 / 临时 / 休眠

| 目标 | 路径 | 风险 | 状态 |
|---|---|---|---|
| 用户日志 | `~/Library/Logs` | safe | ✅ |
| 崩溃报告 | `~/Library/DiagnosticReports` | safe | ✅ |
| 系统日志（sudo） | `/Library/Logs`、`/var/log` | system | ⏳ |
| 系统诊断数据（sudo） | `/private/var/db/{diagnostics,PowerLog,DiagnosticPipeline}` | system | ⏳ |
| 临时文件（>7 天未改） | `$TMPDIR/*` | safe | ⏳ |
| 系统临时文件（>7 天，sudo） | `/private/tmp/*` | system | ⏳ |
| 休眠镜像（sudo） | `/private/var/vm/sleepimage` | caution | ⏳ |

## C. App / 容器缓存

| 目标 | 路径 | 风险 | 状态 |
|---|---|---|---|
| 微信 / 企微 / QQ / 钉钉 / Telegram / 迅雷 / 网易云 / Spotify / Discord / Slack | `~/Library/Containers/<id>/Data/Library/Caches` | caution | ✅ |
| 应用支持目录缓存 | `~/Library/Application Support/*/Caches` | safe | ✅（syscache 兜底） |
| 应用残留（未装 app 的孤儿目录 / plist，动态扫描） | `Containers` / `Group Containers` / `Application Support` / `Preferences` 中 reverse-DNS 孤儿 | danger | ✅（leftover） |

## D. 开发：语言 / 包管理器缓存

| 目标 | 路径 | 风险 | 状态 |
|---|---|---|---|
| npm `_cacache` | `~/.npm/_cacache` | safe | ✅ |
| npm `_npx` / `_prebuilds` / `_logs` | `~/.npm/{_npx,_prebuilds,_logs}` | safe | 🆕 |
| node-gyp 编译头 | `~/.node-gyp` | safe | 🆕 |
| pnpm / Yarn 缓存 | `~/.cache/{pnpm,yarn}`、`~/.pnpm-store`、`~/.yarn` | safe | 🆕（补 `~/.cache/pnpm`、`~/.cache/yarn`、`~/.pnpm-store`；原有 `~/.yarn`） |
| 项目 `node_modules`（深度≤3，动态验证） | 开发目录 `**/node_modules` | caution | 🆕（scan_dev_envs） |
| pip 缓存 | `~/Library/Caches/pip`、`~/.cache/pip` | safe | ✅ + 🆕（`~/.cache/pip` 新增） |
| uv 缓存 | `~/.cache/uv` | safe | 🆕 |
| Cargo registry / git | `~/.cargo/{registry,git}` | safe | ✅ |
| rustup 下载 / 临时 | `~/.rustup/{downloads,tmp}` | safe | 🆕 |
| Rust 编译缓存 | `~/Library/Caches/rustc` | safe | ✅ |
| Ruby gem / Bundler 缓存 | `~/.gem/{cache,specs}`、`~/.bundle/cache` | safe | 🆕 |
| Maven 仓库 | `~/.m2/repository` | safe | ✅ |
| Gradle 缓存 | `~/.gradle/caches` | safe | ✅ |
| SwiftPM / CocoaPods / Go 编译 | `~/Library/Caches/{org.swift.swiftpm,org.cocoapods,go-build}` | safe | ✅ |
| NuGet / Composer / Ivy | `~/.nuget`、`~/.composer/cache`、`~/.ivy2/cache` | safe | 🆕 |
| Homebrew 缓存 | `~/Library/Caches/Homebrew` | safe | ✅（syscache 兜底） |
| XDG 通用缓存（剩余子项） | `~/.cache`（排除 pip/uv/huggingface/torch/yarn/pnpm/lm-studio） | safe | ⏳（未实现动态扫描） |

## E. 虚拟环境 / 解释器

| 目标 | 路径 | 风险 | 状态 |
|---|---|---|---|
| 项目 venv（深度≤2，动态验证 `pyvenv.cfg` / `activate`） | 开发目录 `.venv` / `venv` / `virtualenv` / `env` | caution | 🆕（scan_dev_envs） |
| virtualenvs | `~/.virtualenvs` | caution | 🆕 |
| conda 环境 | `~/miniconda3/envs`、`~/anaconda3/envs`、`~/.conda/envs` | caution | 🆕（含新增 `~/.conda/envs`） |
| pyenv 环境 | `~/.pyenv/versions/*/envs/*` | caution | 🆕（scan_pyenv_envs） |

## F. Xcode / 模拟器 / iOS

| 目标 | 路径 | 风险 | 状态 |
|---|---|---|---|
| Xcode 派生数据 | `~/Library/Developer/DerivedData` | safe | ✅ |
| 模拟器设备 | `~/Library/Developer/CoreSimulator/Devices` | safe | ✅ |
| Xcode 缓存 / 设备日志 | `~/Library/Caches/com.apple.dt.Xcode`、`~/Library/Developer/Xcode/Logs` | safe | ✅ |
| Xcode Archives | `~/Library/Developer/Xcode/Archives` | safe | 🆕 |
| iOS DeviceSupport | `~/Library/Developer/Xcode/iOS DeviceSupport` | safe | 🆕 |
| iOS 备份（巨大，绝不自动） | `~/Library/Application Support/MobileSync/Backup` | danger | ✅ |

## G. Docker / 容器运行时（⏳ 待评估）

| 目标 | 路径 / 命令 | 风险 | 状态 |
|---|---|---|---|
| Docker VM 镜像 | `~/Library/Containers/com.docker.docker/Data/vms/*/Docker.raw` + `~/.docker` | caution | ⏳ |
| 构建缓存（命令式） | `docker builder prune -af` | caution | ⏳ |
| 未使用镜像（命令式） | `docker image rm <id>` | caution | ⏳ |
| 已停止容器（命令式） | `docker container prune -f` | safe | ⏳ |

## H. AI 模型 / 邮件 / 其他

| 目标 | 路径 | 风险 | 状态 |
|---|---|---|---|
| HuggingFace / PyTorch / Ollama / LM Studio | `~/.cache/{huggingface,torch}`、`~/.ollama`、`~/Library/Application Support/LM Studio` | caution | 🆕 |
| 邮件下载附件 | `~/Library/Mail Downloads` | caution | 🆕（Envelope Index 复杂，未做） |
| 不完整下载 / 安装包 | `~/Downloads/*.{download,part,crdownload,dmg,iso,pkg}` | safe | ⏳（未实现 glob） |
| 废纸篓 | `~/.Trash` | safe | ✅（清理目标本身） |
| 大文件 / 旧文件（动态，跳过 iCloud 占位） | `~/Downloads/Desktop/Documents/Movies` > 阈值 | caution | ✅（large_files，已规避 iCloud 误下载） |
| 外部卷废纸篓（sudo） | `/Volumes/*/.Trashes/<uid>` | caution | ⏳ |
| Time Machine 本地快照（sudo，命令式） | `tmutil listlocalsnapshots` | caution | ⏳ |

---

## 本次代码改动摘要

- **`src-tauri/src/cleanup.rs`**
  - `STATIC_TARGETS` 新增约 28 项：`dev`（pip `~/.cache/pip`、uv、`~/.virtualenvs`、npm `_npx/_prebuilds/_logs`、`~/.node-gyp`、`~/.cache/pnpm`、`~/.cache/yarn`、`~/.pnpm-store`、rustup、`~/.gem/*`、`~/.bundle/cache`、NuGet/Composer/Ivy、conda envs）、`xcode`（Archives、iOS DeviceSupport）、`ai`（HuggingFace/PyTorch/Ollama/LM Studio）、`mail`（Mail Downloads）。
  - 新增动态扫描 `scan_dev_envs()`：开发目录根（`Projects/projects/code/dev/Developer/work/workspace/Documents/Desktop/Downloads`）深度≤2 的 `venv/virtualenv/.venv/env/node_modules`，带 `is_real_env()` 真环境校验（查 `pyvenv.cfg` / `bin/activate`），跳过 `unokb-*` 与 `unoSwitchKit`；结果并入 `collect_items`。
- **`src/components/CleanupPanel.tsx`**：`CATEGORY_ORDER` 增加 `ai`、`mail` 以便前端分组展示。
- **编译验证**：`cargo test cleanup` 通过（id 唯一性 / 不 panic / 越权拒绝，3 passed）。
- **未落地（⏳ 待评估）**：所有需 sudo 的系统级目录、Docker 命令式清理、Time Machine 快照、XDG 剩余缓存动态扫描、下载 junk glob——需另立项评估（涉及提权与命令式执行，超出当前「主目录内文件移回收站」架构）。

---

## v0.4.8 第二轮能力增强（优化建议全部落地）

在统一清单基础上，对清理功能做了 12 项优化，全部已写入代码并通过 `cargo test cleanup`。

### 安全 / 正确性（P0）
- **「释放」措辞修正**：清理只是把文件移入 `~/.Trash`，并未真正释放磁盘空间（需清空废纸篓后才释放）。结果卡文案改为「已转移 X 至废纸篓」，并新增「清空本应用移入的废纸篓项」按钮。
- **大文件扫描规避 iCloud 误下载**：`walk_large` 跳过 `Mobile Documents` 目录与 `.icloud` 占位文件，避免递归扫描 Documents/Desktop 时触发云端下载、产生不必要流量。
- **二次确认动态警告**：确认文案按所选最高风险变化——含危险项（iOS 备份、应用残留）时明确提示，含谨慎项（App 缓存）时提示，降低误操作概率。

### 覆盖与精度（P1）
- **conda / pyenv 环境补全**：新增 `~/.conda/envs` 静态项与 `scan_pyenv_envs()` 动态扫描 `~/.pyenv/versions/*/envs/*`（均为 caution，不自动清理）。
- **`scan_dev_envs` 深度放宽至 3 并加超时**：覆盖 monorepo（如 `apps/web/node_modules`），整体受 deadline 限流不卡扫描。
- **空间占用排行纳入 home 隐藏目录**：`bundle_usage` 除 `~/Library` 四类外，新增 home 一级隐藏目录（`.cache`/`.ollama`/`.cargo`/`.npm` 等数十 GB 模型/依赖缓存），且跳过 `.Trash` 避免自循环。
- **下载过期可配置 + 递归**：`downloads_expire_days` 可在「定时」页设置（默认 30 天），`scan_downloads_expired` 改为递归子目录（仅列文件，避免父子重复）+ 超时限流。

### 体验 / 工程（P2）
- **持久化排除列表**：`settings.exclude_paths`（对应 macos-system-cleaner 的 `--keep`）。清理项旁新增「⊘」按钮，一键加入排除；清单统一在末尾过滤，永不出现。
- **回收站（废纸篓）管理**：每次清理向 `~/.Trash/switchkit-manifest.json` 写记录；新增「回收站」标签页列出本应用移入项，支持一键「恢复」回原路径与「清空本应用项」。
- **体积统计超时近似**：新增 `dir_size_deadline`，大目录（Xcode DerivedData / node_modules / `.cache`）超时即停止更深递归返回近似，避免扫描卡顿。
- **自动清理判定单一化**：抽出 `is_auto_cleanable()`（safe 且非 xcode），后端 `run_auto_clean` 与前端默认勾选共用，消除两处逻辑漂移。
- **`recreate_after_clean` 排除 dev**：`venv`/`node_modules` 等开发环境移走后不再重建空目录（下次运行会自行重建，留空目录是无意义残留）。

### 测试
- 新增 `is_excluded_matches_prefix`、`auto_cleanable_rules` 单测；`cargo test cleanup` 5 passed，编译零 warning。
