use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

/// 带截止时间的目录大小统计（P2-10）。
///
/// 扫描大目录（Xcode DerivedData / node_modules / `.cache` 等）时，
/// 全量递归可能阻塞数秒乃至更久。此版本在 `deadline` 之后停止更深递归，
/// 返回「已累加的直接文件 + 已完成子目录」的近似值，保证扫描及时返回，
/// 牺牲少量精度换取 UI 不卡顿。
fn dir_size_deadline(path: &Path, deadline: Instant) -> u64 {
    let mut total = 0u64;
    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return 0,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if let Ok(meta) = entry.metadata() {
            if meta.is_dir() {
                if Instant::now() > deadline {
                    // 超时：不再递归更深，仅以 0 计入该子目录（近似）
                    continue;
                }
                total += dir_size_deadline(&p, deadline);
            } else {
                total += meta.len();
            }
        }
    }
    total
}

/// 带截止时间的文件/目录大小（目录递归，超时近似）。
fn file_or_dir_size_deadline(path: &Path, deadline: Instant) -> u64 {
    if let Ok(meta) = fs::metadata(path) {
        if meta.is_dir() {
            dir_size_deadline(path, deadline)
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

// ---------------- 废纸篓清单（回收站） ----------------

/// 废纸篓清单记录（内部，存于 `~/.Trash/switchkit-manifest.json`）。
/// 用于「回收站」面板列出本应用移入的项并支持一键恢复。
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct TrashRecord {
    original_path: String,
    trashed_path: String,
    size_bytes: u64,
    moved_at: i64,
}

/// 清单文件路径（`~/.Trash/switchkit-manifest.json`）。
fn trash_manifest_path() -> PathBuf {
    home().join(".Trash").join("switchkit-manifest.json")
}

/// 把一次「移入废纸篓」记录追加进清单（读-改-写，best-effort）。
fn record_trash(original: &Path, trashed: &Path) {
    let manifest = trash_manifest_path();
    let mut records: Vec<TrashRecord> = match fs::read_to_string(&manifest) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    records.push(TrashRecord {
        original_path: original.to_string_lossy().to_string(),
        trashed_path: trashed.to_string_lossy().to_string(),
        size_bytes: file_or_dir_size(trashed),
        moved_at: now_secs() as i64,
    });
    if let Ok(json) = serde_json::to_string_pretty(&records) {
        let _ = fs::write(&manifest, json);
    }
}

/// 将路径移入废纸篓（`~/.Trash`），文件名加时间戳前缀避免冲突。
/// 同卷走 `rename`（瞬时）；跨设备回退 copy + 删除。
/// 成功后向清单追加一条记录（供「回收站」面板恢复）。
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
        record_trash(path, &dest);
        return Ok(dest);
    }
    copy_dir_recursive(path, &dest).map_err(|e| e.to_string())?;
    remove_path(path);
    record_trash(path, &dest);
    Ok(dest)
}

/// 列出本应用移入废纸篓的项（供回收站面板恢复 / 清空）。
/// 自动剔除已在 Finder 中手动清空的记录。
pub fn list_trash_items() -> Vec<TrashItem> {
    let manifest = trash_manifest_path();
    let records: Vec<TrashRecord> = match fs::read_to_string(&manifest) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    let mut alive_idx: Vec<usize> = Vec::new();
    for (i, r) in records.iter().enumerate() {
        let p = PathBuf::from(&r.trashed_path);
        if p.exists() {
            let name = PathBuf::from(&r.original_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("item")
                .to_string();
            out.push(TrashItem {
                id: r.trashed_path.clone(),
                name,
                original_path: r.original_path.clone(),
                trashed_path: r.trashed_path.clone(),
                size_bytes: r.size_bytes,
                moved_at: r.moved_at,
            });
            alive_idx.push(i);
        }
    }
    // 清理 manifest 中已不存在的记录（用户在 Finder 里手动清了废纸篓）
    if alive_idx.len() != records.len() {
        let kept: Vec<TrashRecord> =
            alive_idx.into_iter().map(|i| records[i].clone()).collect();
        if let Ok(json) = serde_json::to_string_pretty(&kept) {
            let _ = fs::write(&manifest, json);
        }
    }
    out.sort_by(|a, b| b.moved_at.cmp(&a.moved_at));
    out
}

/// 将移入废纸篓的项恢复回原路径（覆盖前先校验原路径不存在）。
pub fn restore_trash_item(trashed_path: &str) -> Result<(), String> {
    let manifest = trash_manifest_path();
    let mut records: Vec<TrashRecord> =
        match fs::read_to_string(&manifest) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => return Err("无废纸篓清单".into()),
        };
    let pos = records.iter().position(|r| r.trashed_path == trashed_path);
    let Some(idx) = pos else {
        return Err("清单中无此记录".into());
    };
    let rec = records[idx].clone();
    let src = PathBuf::from(&rec.trashed_path);
    if !src.exists() {
        return Err("废纸篓中已不存在该项".into());
    }
    let dest = PathBuf::from(&rec.original_path);
    if dest.exists() {
        return Err("原路径已存在，恢复会覆盖，已取消".into());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::rename(&src, &dest).map_err(|e| e.to_string())?;
    records.remove(idx);
    if let Ok(json) = serde_json::to_string_pretty(&records) {
        let _ = fs::write(&manifest, json);
    }
    Ok(())
}

/// 清空本应用移入废纸篓的所有项（仅删 `switchkit-` 前缀，不动系统其它废纸篓文件）。
/// 返回删除项数量。
pub fn empty_switchkit_trash() -> u32 {
    let trash = home().join(".Trash");
    let manifest_name = "switchkit-manifest.json";
    let mut removed = 0u32;
    if trash.exists() {
        if let Ok(entries) = fs::read_dir(&trash) {
            for entry in entries.flatten() {
                let p = entry.path();
                let name = match p.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n.to_string(),
                    None => continue,
                };
                if name == manifest_name || !name.starts_with("switchkit-") {
                    continue;
                }
                remove_path(&p);
                removed += 1;
            }
        }
    }
    // 清空清单
    let _ = fs::write(&trash_manifest_path(), "[]");
    removed
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
    // —— 统一清单补全：原仅 skill 覆盖的文件型开发缓存缺口 ——
    Target { id: "dev-pip-cache", label: "pip 缓存 (~/.cache/pip)", rel: ".cache/pip", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-uv-cache", label: "uv 缓存 (~/.cache/uv)", rel: ".cache/uv", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-virtualenvs", label: "virtualenvs (~/.virtualenvs)", rel: ".virtualenvs", category: "dev", category_label: "开发缓存", risk: CleanRisk::Caution },
    Target { id: "dev-npm-npx", label: "npm _npx 缓存", rel: ".npm/_npx", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-npm-prebuilds", label: "npm _prebuilds", rel: ".npm/_prebuilds", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-npm-logs", label: "npm _logs", rel: ".npm/_logs", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-node-gyp", label: "node-gyp 编译头", rel: ".node-gyp", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-pnpm-cache", label: "pnpm 缓存 (~/.cache/pnpm)", rel: ".cache/pnpm", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-yarn-cache", label: "Yarn 缓存 (~/.cache/yarn)", rel: ".cache/yarn", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-pnpm-store", label: "pnpm store (~/.pnpm-store)", rel: ".pnpm-store", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-rustup-dl", label: "rustup 下载缓存", rel: ".rustup/downloads", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-rustup-tmp", label: "rustup 临时文件", rel: ".rustup/tmp", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-ruby-gem-cache", label: "Ruby gem 缓存", rel: ".gem/cache", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-ruby-gem-specs", label: "Ruby gem specs", rel: ".gem/specs", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-ruby-bundle", label: "Bundler 缓存", rel: ".bundle/cache", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-nuget", label: "NuGet 缓存", rel: ".nuget", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-composer", label: "Composer 缓存", rel: ".composer/cache", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-ivy", label: "Ivy 缓存", rel: ".ivy2/cache", category: "dev", category_label: "开发缓存", risk: CleanRisk::Safe },
    Target { id: "dev-conda-miniconda", label: "conda 环境 (miniconda3/envs)", rel: "miniconda3/envs", category: "dev", category_label: "开发缓存", risk: CleanRisk::Caution },
    Target { id: "dev-conda-anaconda", label: "conda 环境 (anaconda3/envs)", rel: "anaconda3/envs", category: "dev", category_label: "开发缓存", risk: CleanRisk::Caution },
    Target { id: "dev-conda-base", label: "conda 环境 (~/.conda/envs)", rel: ".conda/envs", category: "dev", category_label: "开发缓存", risk: CleanRisk::Caution },
    // Xcode / 模拟器（safe，但体积大）
    Target { id: "xcode-derived", label: "Xcode 派生数据", rel: "Library/Developer/DerivedData", category: "xcode", category_label: "Xcode / 模拟器", risk: CleanRisk::Safe },
    Target { id: "xcode-simulator", label: "模拟器数据", rel: "Library/Developer/CoreSimulator/Devices", category: "xcode", category_label: "Xcode / 模拟器", risk: CleanRisk::Safe },
    Target { id: "xcode-cache", label: "Xcode 缓存", rel: "Library/Caches/com.apple.dt.Xcode", category: "xcode", category_label: "Xcode / 模拟器", risk: CleanRisk::Safe },
    Target { id: "xcode-devicelogs", label: "Xcode 设备日志", rel: "Library/Developer/Xcode/Logs", category: "xcode", category_label: "Xcode / 模拟器", risk: CleanRisk::Safe },
    // —— 统一清单补全：Xcode 额外目录 ——
    Target { id: "xcode-archives", label: "Xcode Archives", rel: "Library/Developer/Xcode/Archives", category: "xcode", category_label: "Xcode / 模拟器", risk: CleanRisk::Safe },
    Target { id: "xcode-devicesupport", label: "Xcode iOS DeviceSupport", rel: "Library/Developer/Xcode/iOS DeviceSupport", category: "xcode", category_label: "Xcode / 模拟器", risk: CleanRisk::Safe },
    // 系统日志（safe）
    Target { id: "logs-user", label: "用户日志", rel: "Library/Logs", category: "logs", category_label: "系统日志", risk: CleanRisk::Safe },
    Target { id: "logs-diag", label: "崩溃报告", rel: "Library/DiagnosticReports", category: "logs", category_label: "系统日志", risk: CleanRisk::Safe },
    Target { id: "logs-spm", label: "SwiftPM 日志", rel: "Library/Logs/org.swift.swiftpm", category: "logs", category_label: "系统日志", risk: CleanRisk::Safe },
    // iOS 备份（danger：巨大，需显式选择，绝不自动）
    Target { id: "ios-backup", label: "iOS 备份", rel: "Library/Application Support/MobileSync/Backup", category: "ios", category_label: "iOS 备份", risk: CleanRisk::Danger },
    // —— 统一清单补全：AI 模型与邮件（体积大 / 重建成本高，caution，绝不自动清理）——
    Target { id: "ai-huggingface", label: "HuggingFace 模型", rel: ".cache/huggingface", category: "ai", category_label: "AI 模型", risk: CleanRisk::Caution },
    Target { id: "ai-torch", label: "PyTorch 缓存", rel: ".cache/torch", category: "ai", category_label: "AI 模型", risk: CleanRisk::Caution },
    Target { id: "ai-ollama", label: "Ollama 模型", rel: ".ollama", category: "ai", category_label: "AI 模型", risk: CleanRisk::Caution },
    Target { id: "ai-lmstudio", label: "LM Studio 模型", rel: "Library/Application Support/LM Studio", category: "ai", category_label: "AI 模型", risk: CleanRisk::Caution },
    Target { id: "mail-downloads", label: "邮件下载附件", rel: "Library/Mail Downloads", category: "mail", category_label: "邮件垃圾", risk: CleanRisk::Caution },
];

/// 哪些分类在清理后应「重建空目录」（避免依赖该目录的应用崩溃）。
/// 残留 / 下载过期 / 开发环境通常是文件或自管理目录，不重建：
/// venv / node_modules 下次运行会自行重建，留空目录反成无意义残留。
fn recreate_after_clean(category: &str) -> bool {
    !matches!(category, "leftover" | "downloads" | "dev")
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

/// 扫描 ~/Downloads 中修改时间早于 `days` 天的项（递归子目录，带超时）。
///
/// 仅列出文件（目录只用于递归探索，从不单独列出），避免父子重复计数；
/// 受 `deadline` 与数量上限限流，超时即停止，避免大 Downloads 卡住扫描。
fn scan_downloads_expired(days: u64, deadline: Instant) -> Vec<CleanItem> {
    let dl = home().join("Downloads");
    let mut items = Vec::new();
    if !dl.is_dir() {
        return items;
    }
    let cutoff = now_secs().saturating_sub(days * 86400);
    let mut count = 0usize;
    walk_downloads_expired(&dl, cutoff, &mut items, &mut count, deadline, 0, 4);
    items
}

fn walk_downloads_expired(
    dir: &Path,
    cutoff: u64,
    found: &mut Vec<CleanItem>,
    count: &mut usize,
    deadline: Instant,
    depth: u32,
    max_depth: u32,
) {
    if depth > max_depth || Instant::now() > deadline || *count > 8000 {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        if Instant::now() > deadline || *count > 8000 {
            return;
        }
        let p = entry.path();
        *count += 1;
        if let Ok(meta) = fs::metadata(&p) {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let expired = mtime > 0 && mtime < cutoff;
            if meta.is_dir() {
                walk_downloads_expired(&p, cutoff, found, count, deadline, depth + 1, max_depth);
            } else if expired {
                let name = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("item")
                    .to_string();
                found.push(CleanItem {
                    id: format!("downloads:{}", p.to_string_lossy()),
                    label: name,
                    path: p.to_string_lossy().to_string(),
                    category: "downloads".into(),
                    category_label: "下载过期".into(),
                    risk: CleanRisk::Caution,
                    size_bytes: meta.len(),
                    exists: true,
                });
            }
        }
    }
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

// ---------------- 动态扫描：开发环境（venv / node_modules） ----------------

/// 开发目录根（仅这些根下深度 ≤2 扫 venv / node_modules，避免全机慢扫与误伤）。
const DEV_ROOTS: &[&str] = &[
    "Projects", "projects", "code", "dev", "Developer", "work", "workspace",
    "Documents", "Desktop", "Downloads",
];

/// 扫描时跳过的目录名（用户声明保留的巨型项目 / 本应用自身 / 不应作为环境候选的目录）。
const DEVENV_SKIP: &[&str] = &[
    "unokb-weknora",
    "unokb-dify-weknora-ragflow",
    "unoSwitchKit",
    "Library",
    ".cache",
    ".git",
];

/// 验证候选目录是否为真实虚拟环境 / 依赖目录（名称不可信，只看标记文件）。
fn is_real_env(p: &Path, name: &str) -> bool {
    if name == "node_modules" {
        return p.is_dir();
    }
    if name == "venv" || name == "virtualenv" || name == ".venv" || name == "env" {
        return p.join("pyvenv.cfg").exists()
            || p.join("bin").join("activate").exists()
            || p.join("Scripts").join("activate").exists();
    }
    false
}

/// 动态扫描开发目录下 Python 虚拟环境与 node_modules（逐项目确认）。
///
/// 深度放宽到 ≤3 以覆盖 monorepo（如 `apps/web/node_modules`）；
/// 整体受 `deadline` 限流，超时即停止，避免大 workspace 卡住扫描。
fn scan_dev_envs(deadline: Instant) -> Vec<CleanItem> {
    let targets = ["venv", "virtualenv", ".venv", "env", "node_modules"];
    let mut items = Vec::new();
    for root_name in DEV_ROOTS.iter() {
        let root = home().join(root_name);
        if !root.is_dir() {
            continue;
        }
        scan_dev_envs_recursive(&root, &targets, &mut items, 0, 3, deadline);
    }
    items
}

fn scan_dev_envs_recursive(
    dir: &Path,
    targets: &[&str],
    items: &mut Vec<CleanItem>,
    depth: u32,
    max_depth: u32,
    deadline: Instant,
) {
    if depth > max_depth || Instant::now() > deadline {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let name = match p.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if DEVENV_SKIP.contains(&name.as_str()) {
            continue;
        }
        if p.is_dir() {
            if targets.contains(&name.as_str()) && is_real_env(&p, &name) {
                push_env_item(items, &p);
                // 命中后不再往依赖目录内部深扫（其内容不是更多环境）
                continue;
            }
            if !name.starts_with('.') {
                scan_dev_envs_recursive(&p, targets, items, depth + 1, max_depth, deadline);
            }
        }
    }
}

/// pyenv 虚拟环境：`~/.pyenv/versions/<ver>/envs/<name>`（逐项目确认，caution）。
fn scan_pyenv_envs() -> Vec<CleanItem> {
    let base = home().join(".pyenv").join("versions");
    let mut items = Vec::new();
    if !base.is_dir() {
        return items;
    }
    if let Ok(versions) = fs::read_dir(&base) {
        for v in versions.flatten() {
            let envs = v.path().join("envs");
            if !envs.is_dir() {
                continue;
            }
            if let Ok(entries) = fs::read_dir(&envs) {
                for e in entries.flatten() {
                    let p = e.path();
                    if !p.is_dir() {
                        continue;
                    }
                    let name = p
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("env")
                        .to_string();
                    if is_real_env(&p, &name) {
                        push_env_item(&mut items, &p);
                    }
                }
            }
        }
    }
    items
}

fn push_env_item(items: &mut Vec<CleanItem>, p: &Path) {
    let size = file_or_dir_size(p);
    let name = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("item")
        .to_string();
    items.push(CleanItem {
        id: format!("devenv:{}", p.to_string_lossy()),
        label: name,
        path: p.to_string_lossy().into(),
        category: "dev".into(),
        category_label: "开发缓存".into(),
        risk: CleanRisk::Caution,
        size_bytes: size,
        exists: p.exists(),
    });
}

// ---------------- 汇总扫描 ----------------

/// 路径是否命中持久化排除列表（精确匹配或以排除路径为父目录）。
/// 对应 macos-system-cleaner 的 `--keep`：用户声明「永不清理」的路径不会出现在清单里。
fn is_excluded(path: &str, exclude: &[String]) -> bool {
    for ex in exclude {
        if path == ex.as_str() || path.starts_with(&format!("{}/", ex)) {
            return true;
        }
    }
    false
}

/// 收集所有可清理项（静态 + 自定义 + 下载过期 + 应用残留 + 用户缓存总览 + 开发环境）。
///
/// - `exclude`：持久化排除列表（绝对路径），命中者不出现。
/// - `downloads_expire_days`：下载过期判定天数。
/// - 体积统计统一走带截止时间的近似版本（P2-10），避免大目录阻塞扫描。
pub fn collect_items(
    extra: &[String],
    exclude: &[String],
    downloads_expire_days: u32,
) -> Vec<CleanItem> {
    let mut items = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(6);

    for t in STATIC_TARGETS.iter() {
        let p = expand(t.rel);
        let exists = p.exists();
        let size = if exists {
            file_or_dir_size_deadline(&p, deadline)
        } else {
            0
        };
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
        let size = if exists {
            file_or_dir_size_deadline(&p, deadline)
        } else {
            0
        };
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

    items.extend(scan_downloads_expired(
        downloads_expire_days as u64,
        deadline,
    ));
    let installed = installed_bundle_ids();
    items.extend(scan_leftovers(&installed));
    items.extend(scan_user_caches());
    items.extend(scan_dev_envs(deadline));
    items.extend(scan_pyenv_envs());

    // 应用持久化排除列表（统一在末尾过滤，覆盖所有来源）
    items.retain(|it| !is_excluded(&it.path, exclude));

    items
}

/// 预览：仅统计，不删除。
pub fn scan(extra: &[String], exclude: &[String], downloads_expire_days: u32) -> CleanPreview {
    let items = collect_items(extra, exclude, downloads_expire_days);
    let total_bytes: u64 = items
        .iter()
        .filter(|i| i.exists)
        .map(|i| i.size_bytes)
        .sum();
    CleanPreview { items, total_bytes }
}

// ---------------- 清理 ----------------

/// 清理指定 id 的项；先移入废纸篓，缓存类目录重建空目录。逐项目容错。
pub fn clean(
    ids: &[String],
    extra: &[String],
    exclude: &[String],
    downloads_expire_days: u32,
) -> CleanResult {
    let items = collect_items(extra, exclude, downloads_expire_days);
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

/// 是否可被自动清理（safe 级且非 Xcode；Xcode 体积大、重建慢，排除）。
/// 单一事实源：后端 `run_auto_clean` 与前端默认勾选共用同一判定（P2-11）。
pub fn is_auto_cleanable(i: &CleanItem) -> bool {
    i.risk == CleanRisk::Safe && i.category != "xcode"
}

/// 自动清理：仅清理 safe 级存在的项（Xcode/模拟器体积大、重建慢，自动清理排除）。
pub fn run_auto_clean(
    extra: &[String],
    exclude: &[String],
    downloads_expire_days: u32,
) -> CleanResult {
    let items = collect_items(extra, exclude, downloads_expire_days);
    let safe_ids: Vec<String> = items
        .iter()
        .filter(|i| i.exists && is_auto_cleanable(i))
        .map(|i| i.id.clone())
        .collect();
    clean(&safe_ids, extra, exclude, downloads_expire_days)
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
        let p = scan(&[], &[], 30);
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

    /// 持久化排除列表：精确匹配与父目录前缀匹配，且不被前缀误伤。
    #[test]
    fn is_excluded_matches_prefix() {
        assert!(is_excluded("/Users/x/a", &["/Users/x/a".to_string()]));
        assert!(is_excluded("/Users/x/a/b", &["/Users/x/a".to_string()]));
        assert!(!is_excluded("/Users/x/ab", &["/Users/x/a".to_string()]));
        assert!(!is_excluded("/Users/x/a", &[]));
    }

    /// 自动清理规则：safe 且非 xcode 才可自动清理。
    #[test]
    fn auto_cleanable_rules() {
        let mk = |risk: CleanRisk, cat: &str| CleanItem {
            id: "x".into(),
            label: "x".into(),
            path: "/x".into(),
            category: cat.into(),
            category_label: "".into(),
            risk,
            size_bytes: 0,
            exists: true,
        };
        assert!(is_auto_cleanable(&mk(CleanRisk::Safe, "browser")));
        assert!(!is_auto_cleanable(&mk(CleanRisk::Safe, "xcode")));
        assert!(!is_auto_cleanable(&mk(CleanRisk::Caution, "app")));
        assert!(!is_auto_cleanable(&mk(CleanRisk::Danger, "ios")));
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
    let r = run_auto_clean(
        &s.cache_whitelist_extra,
        &s.exclude_paths,
        s.downloads_expire_days,
    );
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

/// 按占用者（目录名）聚合 `~/Library` 与 home 一级隐藏目录的体积，返回 Top N。
///
/// 覆盖范围扩展（P1-6）：除 `~/Library` 下四类外，新增 home 一级以 `.` 开头的
/// 隐藏目录（如 `.cache`/`.ollama`/`.cargo`/`.npm`），这些常是数十 GB 的模型 /
/// 依赖缓存，却不在 `~/Library` 内、原排行看不到。跳过 `.Trash` 避免自循环。
pub fn bundle_usage() -> Vec<BundleUsage> {
    let lib = home().join("Library");
    let roots = vec![
        lib.join("Caches"),
        lib.join("Application Support"),
        lib.join("Containers"),
        lib.join("Group Containers"),
    ];
    let deadline = Instant::now() + Duration::from_secs(4);
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
                    size_bytes: dir_size_deadline(&p, deadline),
                });
            }
        }
    }
    // home 一级隐藏目录（巨型缓存常驻此处）
    if let Ok(entries) = fs::read_dir(home()) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let name = match p.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if !name.starts_with('.') || name == ".Trash" {
                continue;
            }
            out.push(BundleUsage {
                name,
                path: p.to_string_lossy().to_string(),
                size_bytes: dir_size_deadline(&p, deadline),
            });
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
            if n == "node_modules" || n == ".git" || n == "Library"
                || n == "Mobile Documents" {
                continue;
            }
        }
        if let Ok(meta) = fs::metadata(&p) {
            if meta.is_dir() {
                walk_large(&p, threshold, found, count, start, depth + 1);
            } else if meta.len() >= threshold {
                // iCloud 占位文件（以 .icloud 结尾）：打开读取会触发云端下载，
                // 既产生不必要流量，也可能误下载大文件，直接跳过。
                if let Some(name) = p.file_name().and_then(|nm| nm.to_str()) {
                    if name.ends_with(".icloud") {
                        continue;
                    }
                }
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
