use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::models::*;
use crate::util;

/// 清理功能统一模块：扫描 / 清理 / 分析 / 自动清理。
///
/// 安全策略（继承 NFR-3）：
/// - 所有路径限制在用户主目录内；清理前校验 `is_under_home`，越权直接拒绝。
/// - 删除前由前端走「预览 → 勾选 → 二次确认」。
/// - 清理一律「先移入废纸篓（`~/.Trash`）」而非硬删，可恢复。

// ---------------- 基础工具 ----------------

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn home() -> PathBuf {
    util::home_dir()
}

/// 将 `/`-分隔的相对路径展开到主目录下。
fn expand(rel: &str) -> PathBuf {
    let mut p = home();
    for seg in rel.split('/') {
        if seg.is_empty() {
            continue;
        }
        p.push(seg);
    }
    p
}

/// 路径是否位于主目录内（防越权）。
fn is_under_home(p: &Path) -> bool {
    p.strip_prefix(home()).is_ok()
}

/// 递归统计目录大小（字节）。
pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Ok(meta) = entry.metadata() {
                if meta.is_dir() {
                    total += dir_size(&p);
                } else {
                    total += meta.len();
                }
            }
        }
    }
    total
}

/// 文件或目录的大小（目录递归）。
fn file_or_dir_size(path: &Path) -> u64 {
    if let Ok(meta) = fs::metadata(path) {
        if meta.is_dir() {
            dir_size(path)
        } else {
            meta.len()
        }
    } else {
        0
    }
}

fn remove_path(path: &Path) {
    if path.is_dir() {
        let _ = fs::remove_dir_all(path);
    } else {
        let _ = fs::remove_file(path);
    }
}

/// 递归复制（跨设备回退用，仅在 rename 失败时使用）。
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let p = entry.path();
        let np = dst.join(entry.file_name());
        if p.is_dir() {
            copy_dir_recursive(&p, &np)?;
        } else {
            fs::copy(&p, &np)?;
        }
    }
    Ok(())
}

/// 将路径移入废纸篓（`~/.Trash`），文件名加时间戳前缀避免冲突。
/// 同卷走 `rename`（瞬时）；跨设备回退 copy + 删除。
fn move_to_trash(path: &Path) -> Result<PathBuf, String> {
    let trash = home().join(".Trash");
    fs::create_dir_all(&trash).map_err(|e| e.to_string())?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("item")
        .to_string();
    let stamp = now_secs();
    let mut dest = trash.join(format!("switchkit-{}-{}", stamp, name));
    let mut i = 1;
    while dest.exists() {
        dest = trash.join(format!("switchkit-{}-{}-{}", stamp, i, name));
        i += 1;
    }
    if fs::rename(path, &dest).is_ok() {
        return Ok(dest);
    }
    copy_dir_recursive(path, &dest).map_err(|e| e.to_string())?;
    remove_path(path);
    Ok(dest)
}

// ---------------- 静态清理目标 ----------------

struct Target {
    id: &'static str,
    label: &'static str,
    rel: &'static str,
    category: &'static str,
    category_label: &'static str,
    risk: CleanRisk,
}

/// 静态白名单目标（相对 ~/）。涵盖 FR-3.3（用户缓存）与 FR-3.5（Xcode 派生数据）等。
const STATIC_TARGETS: &[Target] = &[
    // 浏览器缓存（safe）
    Target { id: "browser-safari", label: "Safari 缓存", rel: "Library/Caches/com.apple.Safari", category: "browser", category_label: "浏览器缓存", risk: CleanRisk::Safe },
    Target { id: "browser-safari-sb", label: "Safari 安全浏览缓存", rel: "Library/Caches/com.apple.Safari.SafeBrowsing", category: "browser", category_label: "浏览器缓存", risk: CleanRisk::Safe },
    Target { id: "browser-chrome", label: "Chrome 缓存", rel: "Library/Caches/com.google.Chrome", category: "browser", category_label: "浏览器缓存", risk: CleanRisk::Safe },
    Target { id: "browser-chrome-helper", label: "Chrome Helper 缓存", rel: "Library/Caches/com.google.Chrome.helper", category: "browser", category_label: "浏览器缓存", risk: CleanRisk::Safe },
    Target { id: "browser-edge", label: "Edge 缓存", rel: "Library/Caches/com.microsoft.Edge", category: "browser", category_label: "浏览器缓存", risk: CleanRisk::Safe },
    Target { id: "browser-brave", label: "Brave 缓存", rel: "Library/Caches/com.brave.Browser", category: "browser", category_label: "浏览器缓存", risk: CleanRisk::Safe },
    Target { id: "browser-arc", label: "Arc 缓存", rel: "Library/Caches/company.thebrowser.Browser", category: "browser", category_label: "浏览器缓存", risk: CleanRisk::Safe },
    Target { id: "browser-opera", label: "Opera 缓存", rel: "Library/Caches/com.operasoftware.Opera", category: "browser", category_label: "浏览器缓存", risk: CleanRisk::Safe },
    Target { id: "browser-firefox", label: "Firefox 缓存", rel: "Library/Caches/org.mozilla.firefox", category: "browser", category_label: "浏览器缓存", risk: CleanRisk::Safe },
    Target { id: "browser-webkit", label: "WebKit 缓存", rel: "Library/Caches/com.apple.WebKit", category: "browser", category_label: "浏览器缓存", risk: CleanRisk::Safe },
    // 系统缓存（safe）
    Target { id: "system-dyld", label: "dyld 共享缓存", rel: "Library/Caches/com.apple.dyld", category: "system", category_label: "系统缓存", risk: CleanRisk::Safe },
    Target { id: "system-quicklook", label: "QuickLook 缓存", rel: "Library/Caches/com.apple.QuickLookDaemon", category: "system", category_label: "系统缓存", risk: CleanRisk::Safe },
    Target { id: "system-helpd", label: "帮助文档缓存", rel: "Library/Caches/com.apple.helpd", category: "system", category_label: "系统缓存", risk: CleanRisk::Safe },
    Target { id: "system-spotlight", label: "Spotlight 缓存", rel: "Library/Caches/com.apple.Spotlight", category: "system", category_label: "系统缓存", risk: CleanRisk::Safe },
    Target { id: "system-fonts", label: "字体缓存", rel: "Library/Caches/com.apple.fonts", category: "system", category_label: "系统缓存", risk: CleanRisk::Safe },
    // App 缓存（caution：清了可能丢登录态/离线内容）
    Target { id: "app-wechat", label: "微信缓存", rel: "Library/Containers/com.tencent.xinWeChat/Data/Library/Caches", category: "app", category_label: "App 缓存", risk: CleanRisk::Caution },
    Target { id: "app-wework", label: "企业微信缓存", rel: "Library/Containers/com.tencent.WeWorkMac/Data/Library/Caches", category: "app", category_label: "App 缓存", risk: CleanRisk::Caution },
    Target { id: "app-qq", label: "QQ 缓存", rel: "Library/Containers/com.tencent.qq/Data/Library/Caches", category: "app", category_label: "App 缓存", risk: CleanRisk::Caution },
    Target { id: "app-dingtalk", label: "钉钉缓存", rel: "Library/Containers/com.dingtalk.mac/Data/Library/Caches", category: "app", category_label: "App 缓存", risk: CleanRisk::Caution },
    Target { id: "app-telegram", label: "Telegram 缓存", rel: "Library/Containers/ru.keepcoder.Telegram/Data/Library/Caches", category: "app", category_label: "App 缓存", risk: CleanRisk::Caution },
    Target { id: "app-xunlei", label: "迅雷缓存", rel: "Library/Caches/com.xunlei.Thunder", category: "app", category_label: "App 缓存", risk: CleanRisk::Caution },
    Target { id: "app-neteasemusic", label: "网易云音乐缓存", rel: "Library/Caches/com.netease.163music", category: "app", category_label: "App 缓存", risk: CleanRisk::Caution },
    Target { id: "app-spotify", label: "Spotify 缓存", rel: "Library/Caches/com.spotify.client", category: "app", category_label: "App 缓存", risk: CleanRisk::Caution },
    Target { id: "app-discord", label: "Discord 缓存", rel: "Library/Caches/com.hnc.Discord", category: "app", category_label: "App 缓存", risk: CleanRisk::Caution },
    Target { id: "app-slack", label: "Slack 缓存", rel: "Library/Caches/com.tinyspeck.slackmacgap", category: "app", category_label: "App 缓存", risk: CleanRisk::Caution },
    // 开发缓存（safe）
    Target { id: "dev-npm", label: "npm 缓存", rel: ".npm/_cacache", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-cargo-registry", label: "Cargo Registry 缓存", rel: ".cargo/registry", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-cargo-git", label: "Cargo Git 缓存", rel: ".cargo/git", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-pip", label: "pip 缓存", rel: "Library/Caches/pip", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-yarn", label: "Yarn 缓存", rel: ".yarn", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-gradle", label: "Gradle 缓存", rel: ".gradle/caches", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-maven", label: "Maven 仓库", rel: ".m2/repository", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-swiftpm", label: "SwiftPM 缓存", rel: "Library/Caches/org.swift.swiftpm", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-cocoapods", label: "CocoaPods 缓存", rel: "Library/Caches/org.cocoapods", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-go", label: "Go 模块缓存", rel: "Library/Caches/go-build", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-rust", label: "Rust 编译缓存", rel: "Library/Caches/rustc", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    // Xcode / 模拟器（safe，但体积大）
    Target { id: "xcode-derived", label: "Xcode 派生数据", rel: "Library/Developer/DerivedData", category: "xcode", category_label: "Xcode / 模拟器", risk: CleanRisk::Safe },
    Target { id: "xcode-simulator", label: "模拟器数据", rel: "Library/Developer/CoreSimulator/Devices", category: "xcode", category_label: "Xcode / 模拟器", risk: CleanRisk::Safe },
    Target { id: "xcode-cache", label: "Xcode 缓存", rel: "Library/Caches/com.apple.dt.Xcode", category: "xcode", category_label: "Xcode / 模拟器", risk: CleanRisk::Safe },
    Target { id: "xcode-devicelogs", label: "Xcode 设备日志", rel: "Library/Developer/Xcode/Logs", category: "xcode", category_label: "Xcode / 模拟器", risk: CleanRisk::Safe },
    // 系统日志（safe）
    Target { id: "logs-user", label: "用户日志", rel: "Library/Logs", category: "logs", category_label: "系统日志", risk: CleanRisk::Safe },
    Target { id: "logs-diag", label: "崩溃报告", rel: "Library/DiagnosticReports", category: "logs", category_label: "系统日志", risk: CleanRisk::Safe },
    Target { id: "logs-spm", label: "SwiftPM 日志", rel: "Library/Logs/org.swift.swiftpm", category: "logs", category_label: "系统日志", risk: CleanRisk::Safe },
    // iOS 备份（danger：巨大，需显式选择，绝不自动）
    Target { id: "ios-backup", label: "iOS 备份", rel: "Library/Application Support/MobileSync/Backup", category: "ios", category_label: "iOS 备份", risk: CleanRisk::Danger },
];

/// 哪些分类在清理后应「重建空目录」（避免依赖该目录的应用崩溃）。
/// 残留 / 下载过期通常是文件或孤儿目录，不重建。
fn recreate_after_clean(category: &str) -> bool {
    !matches!(category, "leftover" | "downloads")
}

// ---------------- 动态扫描：已安装 App 的 bundle id ----------------

fn bundle_id_of(app: &Path) -> Option<String> {
    let plist = app.join("Contents/Info.plist");
    if !plist.exists() {
        return None;
    }
    if let Ok(text) = fs::read_to_string(&plist) {
        if let Some(id) = parse_bundle_id_xml(&text) {
            return Some(id);
        }
    }
    // 回退：defaults 能解析二进制 plist
    if let Ok(out) =
        util::run("defaults", &["read", &plist.to_string_lossy(), "CFBundleIdentifier"])
    {
        let id = out.trim().to_string();
        if !id.is_empty() && !id.contains('{') && !id.contains('=') {
            return Some(id);
        }
    }
    None
}

fn parse_bundle_id_xml(text: &str) -> Option<String> {
    let key = "<key>CFBundleIdentifier</key>";
    let pos = text.find(key)?;
    let rest = &text[pos + key.len()..];
    let spos = rest.find("<string>")? + 8;
    let after = &rest[spos..];
    let epos = after.find("</string>")?;
    let v = after[..epos].trim().to_string();
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

fn collect_app_bundle_ids(dir: &Path, set: &mut std::collections::HashSet<String>, depth: u32) {
    if depth > 3 {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
            if name.ends_with(".app") {
                if let Some(id) = bundle_id_of(&p) {
                    set.insert(id);
                }
                continue;
            }
        }
        if p.is_dir() {
            collect_app_bundle_ids(&p, set, depth + 1);
        }
    }
}

/// 枚举 /Applications 与 ~/Applications 下所有 .app 的 bundle id。
fn installed_bundle_ids() -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    let roots = vec![PathBuf::from("/Applications"), home().join("Applications")];
    for root in roots {
        if root.exists() {
            collect_app_bundle_ids(&root, &mut set, 0);
        }
    }
    set
}

// ---------------- 动态扫描：应用残留 ----------------

/// 扫描 Containers / Group Containers / Application Support / Preferences 中、
/// 名称形如 reverse-DNS 但本机已无对应 .app 的孤儿项（应用残留）。
fn scan_leftovers(installed: &std::collections::HashSet<String>) -> Vec<CleanItem> {
    let lib = home().join("Library");
    let dirs = vec![
        lib.join("Containers"),
        lib.join("Group Containers"),
        lib.join("Application Support"),
        lib.join("Preferences"),
    ];
    let mut items = Vec::new();
    for d in dirs {
        if !d.is_dir() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(&d) {
        for entry in entries.flatten() {
            let p = entry.path();
            let name = match p.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            // 仅看 reverse-DNS 形态（含点、无空格），跳过系统组件
            if !name.contains('.') || name.contains(' ') {
                continue;
            }
            if name.starts_with("com.apple.") {
                continue;
            }
            let base = name.strip_suffix(".plist").unwrap_or(&name);
            if installed.contains(base) {
                continue;
            }
            let is_dir = p.is_dir();
            let is_plist = name.ends_with(".plist");
            if !is_dir && !is_plist {
                continue;
            }
            let size = file_or_dir_size(&p);
            items.push(CleanItem {
                id: format!("leftover:{}", p.to_string_lossy()),
                label: base.to_string(),
                path: p.to_string_lossy().to_string(),
                category: "leftover".into(),
                category_label: if is_plist {
                    "应用残留（偏好）".into()
                } else {
                    "应用残留".into()
                },
                risk: CleanRisk::Danger,
                size_bytes: size,
                exists: p.exists(),
            });
        }
        }
    }
    items
}

// ---------------- 动态扫描：下载过期文件 ----------------

/// 扫描 ~/Downloads 中修改时间早于 `days` 天的项（下载满 N 天未改动）。
fn scan_downloads_expired(days: u64) -> Vec<CleanItem> {
    let dl = home().join("Downloads");
    let mut items = Vec::new();
    if !dl.is_dir() {
        return items;
    }
    let cutoff = now_secs().saturating_sub(days * 86400);
    if let Ok(entries) = fs::read_dir(&dl) {
    for entry in entries.flatten() {
        let p = entry.path();
        if let Ok(meta) = fs::metadata(&p) {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if mtime > 0 && mtime < cutoff {
                let name = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("item")
                    .to_string();
                items.push(CleanItem {
                    id: format!("downloads:{}", p.to_string_lossy()),
                    label: name,
                    path: p.to_string_lossy().to_string(),
                    category: "downloads".into(),
                    category_label: "下载过期".into(),
                    risk: CleanRisk::Caution,
                    size_bytes: file_or_dir_size(&p),
                    exists: true,
                });
            }
        }
    }
    }
    items
}

// ---------------- 动态扫描：用户缓存总览（~/Library/Caches） ----------------

/// 动态扫描的最小体积阈值：低于此值的零碎缓存不展示，减少噪音（可按需放宽）。
const SYSCACHE_MIN_BYTES: u64 = 1024 * 1024; // 1 MB

/// 已在 `STATIC_TARGETS` 中逐项目录列出的 `Library/Caches/...` 末段名，
/// 动态扫描时跳过以免与静态目标重复呈现。
fn static_cache_basenames() -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    for t in STATIC_TARGETS.iter() {
        if let Some(rest) = t.rel.strip_prefix("Library/Caches/") {
            if let Some(name) = rest.split('/').next() {
                set.insert(name.to_string());
            }
        }
    }
    set
}

/// 枚举 `~/Library/Caches` 下所有顶层项，作为可清理项呈现
/// （与 CleanMyMac 的「用户缓存文件」类别一致：Homebrew / VSCode ShipIt / iCloud / clang / Google …）。
///
/// 风险定为 `Caution`：这些均为缓存、可重建，但内容混杂（含部分 App 缓存，清了可能丢登录态/离线内容），
/// 因此只呈现、需用户手动勾选，且**绝不**被自动清理；亦不会被默认勾选。
fn scan_user_caches() -> Vec<CleanItem> {
    let caches = home().join("Library").join("Caches");
    let mut items = Vec::new();
    if !caches.is_dir() {
        return items;
    }
    // 跳过：本应用自身缓存 + 已在 STATIC_TARGETS 中逐项目录列出的缓存
    let skip_static = static_cache_basenames();
    let own_id = "com.switchkit.app";
    if let Ok(entries) = fs::read_dir(&caches) {
        for entry in entries.flatten() {
            let p = entry.path();
            let name = match p.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if name.starts_with('.') {
                continue; // 隐藏项跳过
            }
            if name == own_id {
                continue; // 不列出本应用自身缓存，避免自清理
            }
            if skip_static.contains(&name) {
                continue; // 已由静态目标覆盖
            }
            let size = file_or_dir_size(&p);
            if size < SYSCACHE_MIN_BYTES {
                continue; // 零碎项过滤
            }
            items.push(CleanItem {
                id: format!("syscache:{}", name),
                label: name.clone(),
                path: p.to_string_lossy().into(),
                category: "syscache".into(),
                category_label: "用户缓存".into(),
                risk: CleanRisk::Caution,
                size_bytes: size,
                exists: p.exists(),
            });
        }
    }
    items.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    items
}

// ---------------- 汇总扫描 ----------------

/// 收集所有可清理项（静态 + 自定义 + 下载过期 + 应用残留 + 用户缓存总览）。
pub fn collect_items(extra: &[String]) -> Vec<CleanItem> {
    let mut items = Vec::new();

    for t in STATIC_TARGETS.iter() {
        let p = expand(t.rel);
        let exists = p.exists();
        let size = if exists { file_or_dir_size(&p) } else { 0 };
        items.push(CleanItem {
            id: t.id.into(),
            label: t.label.into(),
            path: p.to_string_lossy().into(),
            category: t.category.into(),
            category_label: t.category_label.into(),
            risk: t.risk.clone(),
            size_bytes: size,
            exists,
        });
    }

    // 用户自定义路径（safe 级，需位于主目录内）
    for (i, rel) in extra.iter().enumerate() {
        let rel = rel.trim();
        if rel.is_empty() {
            continue;
        }
        let p = expand(rel);
        if !is_under_home(&p) {
            continue;
        }
        let exists = p.exists();
        let size = if exists { file_or_dir_size(&p) } else { 0 };
        let label = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(rel)
            .to_string();
        items.push(CleanItem {
            id: format!("custom:{}", i),
            label,
            path: p.to_string_lossy().into(),
            category: "custom".into(),
            category_label: "自定义".into(),
            risk: CleanRisk::Safe,
            size_bytes: size,
            exists,
        });
    }

    items.extend(scan_downloads_expired(30));
    let installed = installed_bundle_ids();
    items.extend(scan_leftovers(&installed));
    items.extend(scan_user_caches());

    items
}

/// 预览：仅统计，不删除。
pub fn scan(extra: &[String]) -> CleanPreview {
    let items = collect_items(extra);
    let total_bytes: u64 = items
        .iter()
        .filter(|i| i.exists)
        .map(|i| i.size_bytes)
        .sum();
    CleanPreview { items, total_bytes }
}

// ---------------- 清理 ----------------

/// 清理指定 id 的项；先移入废纸篓，缓存类目录重建空目录。逐项目容错。
pub fn clean(ids: &[String], extra: &[String]) -> CleanResult {
    let items = collect_items(extra);
    let map: HashMap<&str, &CleanItem> = items.iter().map(|i| (i.id.as_str(), i)).collect();

    let mut freed_bytes = 0u64;
    let mut cleaned_count = 0usize;
    let mut failed = Vec::new();

    for id in ids {
        let item = match map.get(id.as_str()) {
            Some(i) => i,
            None => {
                failed.push(CleanResultItem {
                    id: id.clone(),
                    ok: false,
                    detail: "项目不存在或已被清理".into(),
                    freed_bytes: 0,
                });
                continue;
            }
        };
        if !item.exists {
            continue;
        }
        let path = PathBuf::from(&item.path);
        if !is_under_home(&path) {
            failed.push(CleanResultItem {
                id: id.clone(),
                ok: false,
                detail: "拒绝越权路径".into(),
                freed_bytes: 0,
            });
            continue;
        }
        let was_dir = path.is_dir();
        match move_to_trash(&path) {
            Ok(_dest) => {
                if was_dir && recreate_after_clean(&item.category) {
                    let _ = fs::create_dir_all(&path);
                }
                freed_bytes += item.size_bytes;
                cleaned_count += 1;
            }
            Err(e) => {
                failed.push(CleanResultItem {
                    id: id.clone(),
                    ok: false,
                    detail: e,
                    freed_bytes: 0,
                });
            }
        }
    }

    CleanResult {
        freed_bytes,
        cleaned_count,
        moved_to_trash: true,
        failed,
    }
}

/// 自动清理：仅清理 safe 级存在的项（Xcode/模拟器体积大、重建慢，自动清理排除）。
pub fn run_auto_clean(extra: &[String]) -> CleanResult {
    let items = collect_items(extra);
    let safe_ids: Vec<String> = items
        .iter()
        .filter(|i| i.exists && i.risk == CleanRisk::Safe && i.category != "xcode")
        .map(|i| i.id.clone())
        .collect();
    clean(&safe_ids, extra)
}

/// 将任意位于主目录内的路径移入废纸篓（供前端对大文件等做定向清理）。
pub fn trash_path(path: &str) -> Result<String, String> {
    let p = PathBuf::from(path);
    if !is_under_home(&p) {
        return Err("拒绝越权路径".into());
    }
    if !p.exists() {
        return Err("路径不存在".into());
    }
    move_to_trash(&p).map(|d| d.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 仅统计、不删除：确认扫描不 panic 且至少返回全部静态目标。
    #[test]
    fn scan_returns_static_items_without_panic() {
        let p = scan(&[]);
        assert!(
            p.items.len() >= STATIC_TARGETS.len(),
            "应至少包含全部静态目标"
        );
        // 静态目标 id 唯一
        let mut ids = std::collections::HashSet::new();
        for it in &p.items {
            assert!(ids.insert(it.id.clone()), "id 重复: {}", it.id);
        }
    }

    /// 确认越权路径被拒绝（安全网）。
    #[test]
    fn trash_path_rejects_out_of_home() {
        assert!(trash_path("/etc/passwd").is_err());
        assert!(trash_path("/tmp/../etc").is_err());
    }

    /// 不存在的路径返回错误而非 panic。
    #[test]
    fn trash_path_rejects_missing() {
        assert!(trash_path(&home().join("switchkit-nonexistent-xyz").to_string_lossy()).is_err());
    }
}

/// 定时器入口：按设置判断是否到期，到期则自动清理 safe 项并记录时间戳。
pub fn run_auto_clean_if_due(app: &tauri::AppHandle) {
    let mut s = crate::store::load_settings(app);
    if !s.auto_clean_enabled {
        return;
    }
    let now = now_secs() as i64;
    let interval = (s.auto_clean_interval_days.max(1) as i64) * 86400;
    if s.last_auto_clean_ts > 0 && (now - s.last_auto_clean_ts) < interval {
        return;
    }
    let r = run_auto_clean(&s.cache_whitelist_extra);
    s.last_auto_clean_ts = now;
    let _ = crate::store::save_settings(app, &s);
    if r.cleaned_count > 0 {
        eprintln!(
            "[switchkit] auto_clean: freed {} bytes, {} items, {} failed",
            r.freed_bytes,
            r.cleaned_count,
            r.failed.len()
        );
    }
}

// ---------------- 分析：空间占用排行 ----------------

/// 按占用者（目录名）聚合 `~/Library` 下各缓存/容器体积，返回 Top N。
pub fn bundle_usage() -> Vec<BundleUsage> {
    let lib = home().join("Library");
    let roots = vec![
        lib.join("Caches"),
        lib.join("Application Support"),
        lib.join("Containers"),
        lib.join("Group Containers"),
    ];
    let mut out = Vec::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(&root) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let name = match p.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            out.push(BundleUsage {
                name,
                path: p.to_string_lossy().to_string(),
                size_bytes: dir_size(&p),
            });
        }
        }
    }
    out.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    out.truncate(25);
    out
}

// ---------------- 分析：大文件 / 疑似重复 ----------------

/// 对文件做「头部 64KB + 尾部 64KB」采样哈希（FNV-1a），用于快速近似判重。
fn sample_hash(path: &Path) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    if let Ok(mut f) = fs::File::open(path) {
        let mut buf = [0u8; 65536];
        if let Ok(n) = f.read(&mut buf) {
            for &b in &buf[..n] {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
        }
        if let Ok(meta) = f.metadata() {
            let len = meta.len();
            if len > 131072 {
                if f.seek(SeekFrom::Start(len - 65536)).is_ok() {
                    if let Ok(n) = f.read(&mut buf) {
                        for &b in &buf[..n] {
                            h ^= b as u64;
                            h = h.wrapping_mul(0x100000001b3);
                        }
                    }
                }
            }
        }
    }
    h
}

fn walk_large(
    dir: &Path,
    threshold: u64,
    found: &mut Vec<(PathBuf, u64, u64)>,
    count: &mut usize,
    start: &Instant,
    depth: u32,
) {
    if depth > 5 || start.elapsed().as_secs() > 8 || *count > 8000 {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        if start.elapsed().as_secs() > 8 || *count > 8000 {
            return;
        }
        let p = entry.path();
        *count += 1;
        if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
            if n.starts_with('.') {
                continue;
            }
            if n == "node_modules" || n == ".git" || n == "Library" {
                continue;
            }
        }
        if let Ok(meta) = fs::metadata(&p) {
            if meta.is_dir() {
                walk_large(&p, threshold, found, count, start, depth + 1);
            } else if meta.len() >= threshold {
                let h = sample_hash(&p);
                found.push((p, meta.len(), h));
            }
        }
    }
}

/// 扫描常用目录中 >= threshold 字节的大文件，并按「相同大小 + 相同内容采样」
/// 标记疑似重复（danger 级，需显式选择，绝不自动清理）。
pub fn large_files(threshold_bytes: u64) -> Vec<LargeFile> {
    let roots = vec![
        home().join("Downloads"),
        home().join("Desktop"),
        home().join("Documents"),
        home().join("Movies"),
    ];
    let start = Instant::now();
    let mut found: Vec<(PathBuf, u64, u64)> = Vec::new();
    let mut count = 0usize;
    for root in roots {
        if root.is_dir() {
            walk_large(&root, threshold_bytes, &mut found, &mut count, &start, 0);
        }
    }

    let mut groups: HashMap<(u64, u64), usize> = HashMap::new();
    for (_, size, h) in &found {
        *groups.entry((*size, *h)).or_insert(0) += 1;
    }

    let mut out: Vec<LargeFile> = found
        .into_iter()
        .map(|(p, size, h)| {
            let dup = groups.get(&(size, h)).copied().unwrap_or(0) > 1;
            LargeFile {
                id: format!("large:{}", p.to_string_lossy()),
                path: p.to_string_lossy().to_string(),
                size_bytes: size,
                duplicate: dup,
            }
        })
        .collect();
    out.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    out.truncate(200);
    out
}
