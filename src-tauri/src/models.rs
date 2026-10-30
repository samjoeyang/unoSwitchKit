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

/// 单个缓存目录清理失败的原因（如受 TCC 保护、权限不足）。
#[derive(Serialize, Deserialize, Clone)]
pub struct CacheClearFailure {
    pub path: String,
    pub reason: String,
}

/// 缓存清理结果：逐目录容错，单个失败不影响其它目录。
#[derive(Serialize, Deserialize, Clone)]
pub struct CacheClearResult {
    pub freed_bytes: u64,
    pub cleared_count: usize,
    pub failed: Vec<CacheClearFailure>,
}

/// 持久化设置（JSON 文件，存于 AppConfig 目录）。
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Settings {
    pub toggle_order: Vec<String>,
    pub cache_whitelist_extra: Vec<String>,
    pub last_release: Option<MemoryComparison>,
    /// 各厂商 API Key 列表（支持多个、可混合厂商）。
    /// 字段别名兼容旧配置里的 `deepseek_keys`（旧结构无 provider 字段，读入后按 deepseek 处理）。
    #[serde(default, alias = "deepseek_keys")]
    pub provider_keys: Vec<ProviderKey>,
    /// 余额查询用的 HTTP 代理（如 http://127.0.0.1:7897），留空直连。
    /// 部分境外站点在境内直连会被 TLS 重置，需要走本地代理。
    #[serde(default)]
    pub http_proxy: String,
}

/// 一个模型平台 API Key 配置。
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ProviderKey {
    pub id: String,
    /// 厂商标识：deepseek / kimi / kimi_intl / zhipu / stepfun / novita /
    /// openrouter / ai302 / aihubmix / custom。空字符串按 deepseek 处理（旧配置兼容）。
    #[serde(default)]
    pub provider: String,
    pub label: String,
    pub api_key: String,
    /// 仅 custom（中转站）使用：站点地址，如 https://api.xxx.com
    #[serde(default)]
    pub base_url: String,
}

/// 一条余额明细（各厂商字段归一化后的展示项）。
#[derive(Serialize, Deserialize, Clone)]
pub struct BalanceLine {
    /// 展示名，如「余额」「赠送」「已用」
    pub label: String,
    /// 已格式化的金额字符串
    pub amount: String,
    /// CNY / USD / 空
    pub currency: String,
    /// 是否主金额（前端放大显示）
    pub primary: bool,
}

/// 某个 Key 的余额查询结果（含错误信息，便于前端逐条展示）。
#[derive(Serialize, Deserialize, Clone)]
pub struct AccountBalance {
    pub id: String,
    pub provider: String,
    pub label: String,
    pub ok: bool,
    /// 账户是否可用（余额不足时部分厂商会返回 false）
    pub is_available: bool,
    /// 展开后的明细行；第一行 primary 即主余额
    pub balances: Vec<BalanceLine>,
    pub error: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::Settings;

    /// 旧版配置里的 `deepseek_keys` 需能读入新的 `provider_keys`（provider 留空 → 视为 deepseek）。
    #[test]
    fn legacy_deepseek_keys_are_migrated() {
        let json = r#"{
            "toggle_order": [],
            "cache_whitelist_extra": [],
            "last_release": null,
            "deepseek_keys": [
                { "id": "1", "label": "主账号", "api_key": "sk-xxx" }
            ]
        }"#;
        let s: Settings = serde_json::from_str(json).expect("旧配置应能解析");
        assert_eq!(s.provider_keys.len(), 1);
        assert_eq!(s.provider_keys[0].provider, "");
        assert_eq!(s.provider_keys[0].label, "主账号");
        assert_eq!(s.provider_keys[0].api_key, "sk-xxx");
        assert_eq!(s.provider_keys[0].base_url, "");
    }
}
