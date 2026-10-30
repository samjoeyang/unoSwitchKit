use std::fs;
use std::path::Path;

use crate::models::{CacheClearFailure, CacheClearResult, CacheItem};
use crate::util;

/// 仅允许清理的用户级缓存白名单（相对 ~/ 的路径）。
///
/// 安全策略（NFR-3）：
/// - 绝不递归清空整个 Library。
/// - 所有路径经主目录校验，拒绝任何越权绝对路径。
/// - 删除前由前端走「预览 → 二次确认」流程。
const WHITELIST: &[&str] = &[
    "Library/Caches/com.apple.Safari",
    "Library/Caches/com.apple.Safari.SafeBrowsing",
    "Library/Caches/com.google.Chrome",
    "Library/Caches/com.google.Chrome.helper",
    "Library/Caches/Google",
    "Library/Caches/com.apple.QuickLookDaemon",
    "Library/Caches/com.apple.dyld",
    "Library/Caches/org.swift.swiftpm",
];

fn expand(rel: &str) -> std::path::PathBuf {
    let mut p = util::home_dir();
    for seg in rel.split('/') {
        if seg.is_empty() {
            continue;
        }
        p.push(seg);
    }
    p
}

fn is_under_home(p: &Path) -> bool {
    p.strip_prefix(util::home_dir()).is_ok()
}

fn dir_size(path: &Path) -> u64 {
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

/// 预览白名单中各目录大小（仅统计，不删除）。
pub fn preview() -> Vec<CacheItem> {
    WHITELIST
        .iter()
        .map(|w| {
            let p = expand(w);
            let exists = p.exists();
            let size = if exists { dir_size(&p) } else { 0 };
            CacheItem {
                path: p.to_string_lossy().to_string(),
                size_bytes: size,
                exists,
            }
        })
        .collect()
}

/// 清理白名单目录，返回清理结果。
///
/// 逐目录容错：单个目录失败（典型如 Safari 缓存受 macOS TCC 保护、普通 app
/// 不可写）不会中断其它目录的清理，失败项收集到 `failed` 里由前端明确提示。
/// 删除后重建空目录，避免依赖该目录存在的应用崩溃。
pub fn clear() -> CacheClearResult {
    let mut freed_bytes = 0u64;
    let mut cleared_count = 0usize;
    let mut failed = Vec::new();

    for w in WHITELIST {
        let p = expand(w);
        if !p.exists() {
            continue;
        }
        if !is_under_home(&p) {
            failed.push(CacheClearFailure {
                path: p.display().to_string(),
                reason: "拒绝越权路径".to_string(),
            });
            continue;
        }
        let size = dir_size(&p);
        match fs::remove_dir_all(&p) {
            Ok(()) => {
                fs::create_dir_all(&p).ok();
                freed_bytes += size;
                cleared_count += 1;
            }
            Err(e) => {
                failed.push(CacheClearFailure {
                    path: p.display().to_string(),
                    reason: e.to_string(),
                });
            }
        }
    }

    CacheClearResult {
        freed_bytes,
        cleared_count,
        failed,
    }
}
