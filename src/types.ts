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

export interface CacheItem {
  path: string;
  size_bytes: number;
  exists: boolean;
}

export interface CachePreview {
  items: CacheItem[];
  total_bytes: number;
}

export interface CacheClearFailure {
  path: string;
  reason: string;
}

export interface CacheClearResult {
  freed_bytes: number;
  cleared_count: number;
  failed: CacheClearFailure[];
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
  cache_whitelist_extra: string[];
  last_release: MemoryComparison | null;
  provider_keys: ProviderKey[];
  /// 余额查询用的 HTTP 代理，留空直连
  http_proxy: string;
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
