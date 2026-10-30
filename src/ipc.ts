import { invoke } from "@tauri-apps/api/core";
import type {
  BtDevice,
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
  toggleHideDock: () => invoke<boolean>("toggle_hide_dock"),
  toggleShowHiddenFiles: () => invoke<boolean>("toggle_show_hidden_files"),
  toggleScreenClean: () => invoke<boolean>("toggle_screen_clean"),
  toggleStageManager: () => invoke<boolean>("toggle_stage_manager"),
  lockScreen: () => invoke<void>("lock_screen"),
  clearClipboard: () => invoke<void>("clear_clipboard"),
  startScreensaver: () => invoke<void>("start_screensaver"),
  closeAllApps: () => invoke<number>("close_all_apps"),
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
  btDevices: () => invoke<BtDevice[]>("bt_devices"),
  btConnect: (address: string) => invoke<boolean>("bt_connect", { address }),
  btDisconnect: (address: string) => invoke<boolean>("bt_disconnect", { address }),
  quitApp: () => invoke<void>("quit_app"),
  menubarState: () => invoke<MenuBarState>("menubar_state"),
  menubarSetVisible: (id: string, show: boolean) =>
    invoke<void>("menubar_set_visible", { id, show }),
  menubarSetManaged: (managed: boolean) =>
    invoke<void>("menubar_set_managed", { managed }),
};
