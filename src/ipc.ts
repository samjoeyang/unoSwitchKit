import { invoke } from "@tauri-apps/api/core";
import type {
  CachePreview,
  MemoryComparison,
  MemoryStat,
  MenuBarState,
  Settings,
  SidecarStatus,
  ToggleStates,
} from "./types";

export const api = {
  getToggleStates: () => invoke<ToggleStates>("get_toggle_states"),
  toggleDarkMode: () => invoke<boolean>("toggle_dark_mode"),
  toggleHideDesktop: () => invoke<boolean>("toggle_hide_desktop"),
  togglePreventSleep: () => invoke<boolean>("toggle_prevent_sleep"),
  emptyTrash: () => invoke<number>("empty_trash"),
  releaseMemory: () => invoke<MemoryComparison>("release_memory"),
  flushDns: () => invoke<void>("flush_dns"),
  previewCacheCleanup: () => invoke<CachePreview>("preview_cache_cleanup"),
  clearCache: () => invoke<number>("clear_cache"),
  getMemoryStats: () => invoke<MemoryStat>("get_memory_stats"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (s: Settings) => invoke<void>("save_settings", { settings: s }),
  setLaunchAtLogin: (enabled: boolean) =>
    invoke<void>("set_launch_at_login", { enabled }),
  isLaunchAtLogin: () => invoke<boolean>("is_launch_at_login"),
  sidecarStatus: () => invoke<SidecarStatus>("sidecar_status"),
  menubarState: () => invoke<MenuBarState>("menubar_state"),
  menubarSetVisible: (id: string, show: boolean) =>
    invoke<void>("menubar_set_visible", { id, show }),
  menubarSetManaged: (managed: boolean) =>
    invoke<void>("menubar_set_managed", { managed }),
};
