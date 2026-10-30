use serde::{Deserialize, Serialize};

/// 面板打开时读取的全部开关真实状态。
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct ToggleStates {
    pub dark_mode: bool,
    pub desktop_hidden: bool,
    pub prevent_sleep: bool,
    pub trash_count: u32,
    /// 隐藏 Dock（autohide）。
    pub hide_dock: bool,
    /// Finder 显示隐藏文件。
    pub show_hidden_files: bool,
    /// 屏幕清洁（本应用隐藏的全部前台窗口已收起）。
    pub screen_clean: bool,
    /// 台前调度（仅 macOS 13+ 有意义）。
    pub stage_manager: bool,
    /// 当前系统是否支持台前调度（< 13 时前端不渲染该开关）。
    pub stage_manager_available: bool,
}

/// 内存占用快照（来自 vm_stat / sysctl）。
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct MemoryStat {
    pub used_mb: u64,
    pub free_mb: u64,
    pub total_mb: u64,
    pub compressed_mb: u64,
}

/// 释放内存前后对比。
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct MemoryComparison {
    pub before: MemoryStat,
    pub after: MemoryStat,
}

/// 单个缓存目录的统计。
#[derive(Serialize, Deserialize, Clone)]
pub struct CacheItem {
    pub path: String,
    pub size_bytes: u64,
    pub exists: bool,
}

/// 缓存清理预览结果。
#[derive(Serialize, Deserialize, Clone)]
pub struct CachePreview {
    pub items: Vec<CacheItem>,
    pub total_bytes: u64,
}

/// 持久化设置（JSON 文件，存于 AppConfig 目录）。
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Settings {
    pub toggle_order: Vec<String>,
    pub cache_whitelist_extra: Vec<String>,
    pub last_release: Option<MemoryComparison>,
}


/// Swift sidecar 健康状态。
#[derive(Serialize, Deserialize, Clone)]
pub struct SidecarStatus {
    pub running: bool,
    pub bluetooth: Option<bool>,
}

/// 已配对蓝牙设备（来自 IOBluetooth，经 Swift 守护转发）。
#[derive(Serialize, Deserialize, Clone)]
pub struct BtDevice {
    pub name: String,
    pub address: String,
    pub connected: bool,
}

/// 在线更新能力配置（前端据此决定「检查更新」按钮是否可用）。
#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateConfig {
    /// 是否已在构建期配置更新服务（SWITCHKIT_UPDATE_ENDPOINTS + PUBKEY）。
    pub enabled: bool,
    /// 当前版本号（来自 tauri.conf.json）。
    pub current_version: String,
}
