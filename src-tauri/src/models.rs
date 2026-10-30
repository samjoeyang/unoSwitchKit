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
    /// 菜单栏管理总开关（开启时未点亮图标默认收纳）。
    #[serde(default = "default_true")]
    pub menubar_managed: bool,
    /// "点亮"（允许显示在菜单栏）的状态项 id 白名单。
    #[serde(default)]
    pub menubar_visible: Vec<String>,
    /// 各状态项隐藏前的原始位置，用于恢复。
    #[serde(default)]
    pub menubar_positions: std::collections::HashMap<String, [f64; 2]>,
}

fn default_true() -> bool {
    true
}

/// 面板菜单栏管理区的状态。
#[derive(Serialize, Deserialize, Clone)]
pub struct MenuBarItemState {
    pub id: String,
    pub owner: String,
    pub title: String,
    /// 是否点亮（允许显示在菜单栏）。
    pub show: bool,
    /// 当前是否实际可见（AX 读取；辅助功能未授权时无意义）。
    pub visible: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct MenuBarState {
    /// 守护进程是否已获辅助功能授权。
    pub ax_trusted: bool,
    /// 守护进程是否在运行。
    pub running: bool,
    /// 管理总开关。
    pub managed: bool,
    pub items: Vec<MenuBarItemState>,
}

/// Swift sidecar 健康状态。
#[derive(Serialize, Deserialize, Clone)]
pub struct SidecarStatus {
    pub running: bool,
    pub bluetooth: Option<bool>,
}
