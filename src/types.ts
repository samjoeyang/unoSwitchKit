export interface ToggleStates {
  dark_mode: boolean;
  desktop_hidden: boolean;
  prevent_sleep: boolean;
  trash_count: number;
  hide_dock: boolean;
  show_hidden_files: boolean;
  screen_clean: boolean;
  stage_manager: boolean;
  stage_manager_available: boolean;
}

export interface MemoryStat {
  used_mb: number;
  free_mb: number;
  total_mb: number;
  compressed_mb: number;
}

export interface MemoryComparison {
  before: MemoryStat;
  after: MemoryStat;
}

export interface SystemStats {
  cpu_percent: number;
  mem_used_bytes: number;
  mem_total_bytes: number;
  net_up_bps: number;
  net_down_bps: number;
}

export type CleanRisk = "safe" | "caution" | "danger";

export interface CleanItem {
  id: string;
  label: string;
  path: string;
  category: string;
  category_label: string;
  risk: CleanRisk;
  size_bytes: number;
  exists: boolean;
}

export interface CleanPreview {
  items: CleanItem[];
  total_bytes: number;
}

export interface CleanResultItem {
  id: string;
  ok: boolean;
  detail: string;
  freed_bytes: number;
}

export interface CleanResult {
  freed_bytes: number;
  cleaned_count: number;
  moved_to_trash: boolean;
  failed: CleanResultItem[];
}

export interface BundleUsage {
  name: string;
  path: string;
  size_bytes: number;
}

export interface LargeFile {
  id: string;
  path: string;
  size_bytes: number;
  duplicate: boolean;
}

/// 废纸篓中由本应用移入的项（回收站面板用）
export interface TrashItem {
  id: string;
  /// 原目录 / 文件名
  name: string;
  /// 原路径（恢复目标）
  original_path: string;
  /// 移入后路径（废纸篓内）
  trashed_path: string;
  size_bytes: number;
  /// 移入时间戳（秒）
  moved_at: number;
}

export interface ProviderKey {
  id: string;
  /// deepseek / kimi / kimi_intl / zhipu / stepfun / novita /
  /// openrouter / ai302 / aihubmix / custom
  provider: string;
  label: string;
  api_key: string;
  /// 仅 custom 使用：站点地址
  base_url: string;
}

export interface BalanceLine {
  label: string;
  amount: string;
  currency: string;
  primary: boolean;
}

export interface AccountBalance {
  id: string;
  provider: string;
  label: string;
  ok: boolean;
  is_available: boolean;
  balances: BalanceLine[];
  error: string | null;
}

export interface Settings {
  toggle_order: string[];
  /// 用户自定义清理路径（相对 ~/，仅 safe 级）
  cache_whitelist_extra: string[];
  last_release: MemoryComparison | null;
  provider_keys: ProviderKey[];
  /// 余额查询用的 HTTP 代理，留空直连
  http_proxy: string;
  /// 自动清理开关（仅清理 safe 级项）
  auto_clean_enabled: boolean;
  /// 自动清理周期（天）
  auto_clean_interval_days: number;
  /// 上次自动清理时间戳（秒），0 表示从未
  last_auto_clean_ts: number;
  /// 持久化排除列表（绝对路径），这些路径永不出现在清理清单中
  exclude_paths: string[];
  /// 下载目录过期文件判定天数（修改时间早于该天数视为可清理），默认 30
  downloads_expire_days: number;
}

export interface SidecarStatus {
  running: boolean;
  bluetooth: boolean | null;
}

export interface BtDevice {
  name: string;
  address: string;
  connected: boolean;
}

export interface UpdateConfig {
  enabled: boolean;
  current_version: string;
}
