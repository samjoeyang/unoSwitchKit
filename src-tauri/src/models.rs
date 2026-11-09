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

/// 清理项的风险等级（决定能否被自动清理 / 是否需要警告）。
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CleanRisk {
    /// 安全：系统/浏览器/开发缓存，可随时重建，允许自动清理。
    Safe,
    /// 谨慎：App 缓存、下载过期文件，清理可能丢失登录态/离线内容，需用户手动勾选。
    Caution,
    /// 危险：应用残留、iOS 备份、大文件，必须用户显式选择，绝不自动清理。
    Danger,
}

/// 单个可清理项的统计与元数据。
#[derive(Serialize, Deserialize, Clone)]
pub struct CleanItem {
    /// 稳定标识（清理时据此定位路径）；动态项形如 `downloads:<path>`。
    pub id: String,
    /// 展示名（如「Safari 缓存」）。
    pub label: String,
    /// 绝对路径。
    pub path: String,
    /// 分类键（browser/system/syscache/app/dev/xcode/logs/ios/downloads/leftover/custom）。
    pub category: String,
    /// 分类展示名（如「浏览器缓存」）。
    pub category_label: String,
    /// 风险等级。
    pub risk: CleanRisk,
    /// 占用大小（字节）。
    pub size_bytes: u64,
    /// 路径是否存在。
    pub exists: bool,
}

/// 清理预览结果（仅统计，不删除）。
#[derive(Serialize, Deserialize, Clone)]
pub struct CleanPreview {
    pub items: Vec<CleanItem>,
    pub total_bytes: u64,
}

/// 单个清理项的执行结果（逐项目容错）。
#[derive(Serialize, Deserialize, Clone)]
pub struct CleanResultItem {
    pub id: String,
    /// 是否成功移入废纸篓。
    pub ok: bool,
    /// 失败原因，或成功时的目标路径。
    pub detail: String,
    /// 释放字节数（成功时）。
    pub freed_bytes: u64,
}

/// 清理结果：逐项目容错，单个失败不影响其它项目。
#[derive(Serialize, Deserialize, Clone)]
pub struct CleanResult {
    pub freed_bytes: u64,
    pub cleaned_count: usize,
    /// 是否经过「移入废纸篓」安全网（当前恒为 true）。
    pub moved_to_trash: bool,
    pub failed: Vec<CleanResultItem>,
}

/// 按占用者（目录名）聚合的空间占用排行项。
#[derive(Serialize, Deserialize, Clone)]
pub struct BundleUsage {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
}

/// 大文件 / 疑似重复文件扫描项。
#[derive(Serialize, Deserialize, Clone)]
pub struct LargeFile {
    pub id: String,
    pub path: String,
    pub size_bytes: u64,
    /// 是否落在「相同大小 + 相同内容采样」的重复组中（疑似重复）。
    pub duplicate: bool,
}

/// 持久化设置（JSON 文件，存于 AppConfig 目录）。
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Settings {
    pub toggle_order: Vec<String>,
    /// 用户自定义清理路径（相对 ~/，仅 safe 级），仍未在旧版接线，本次统一接入。
    #[serde(default)]
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
    /// 自动清理开关（仅清理 safe 级项，避免误删 App 数据）。
    #[serde(default)]
    pub auto_clean_enabled: bool,
    /// 自动清理周期（天），最小 1。
    #[serde(default = "default_auto_clean_interval")]
    pub auto_clean_interval_days: u32,
    /// 上次自动清理的时间戳（秒）；0 表示从未执行。
    #[serde(default)]
    pub last_auto_clean_ts: i64,
}

fn default_auto_clean_interval() -> u32 {
    7
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
