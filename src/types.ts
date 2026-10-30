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

export interface Settings {
  toggle_order: string[];
  cache_whitelist_extra: string[];
  last_release: MemoryComparison | null;
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

export interface MenuBarItemState {
  id: string;
  owner: string;
  title: string;
  show: boolean;
  visible: boolean;
}

export interface MenuBarState {
  ax_trusted: boolean;
  running: boolean;
  managed: boolean;
  items: MenuBarItemState[];
}
