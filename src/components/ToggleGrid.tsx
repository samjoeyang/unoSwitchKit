import { StatusDot } from "./StatusDot";
import type { AppearanceMode, ToggleStates } from "../types";

export type ToggleKey =
  | "dark_mode"
  | "system_screenshot"
  | "desktop_hidden"
  | "prevent_sleep"
  | "empty_trash"
  | "hide_dock"
  | "show_hidden_files"
  | "screen_clean"
  | "stage_manager"
  | "lock_screen"
  | "clear_clipboard"
  | "screensaver"
  | "close_all_apps"
  | "flush_dns";

interface Item {
  key: ToggleKey;
  label: string;
  status: (s: ToggleStates) => string;
  active: (s: ToggleStates) => boolean;
  /** 不可见条件（返回 true 则隐藏该项） */
  hidden?: (s: ToggleStates) => boolean;
}

const ITEMS: Item[] = [
  {
    key: "dark_mode",
    label: "外观",
    status: (s) => (s.dark_mode ? "深色" : "浅色"),
    active: (s) => s.dark_mode,
  },
  {
    key: "desktop_hidden",
    label: "隐藏桌面",
    status: (s) => (s.desktop_hidden ? "已隐藏" : "显示中"),
    active: (s) => s.desktop_hidden,
  },
  {
    key: "prevent_sleep",
    label: "阻止睡眠",
    status: (s) => (s.prevent_sleep ? "生效中" : "关闭"),
    active: (s) => s.prevent_sleep,
  },
  {
    key: "empty_trash",
    label: "清空废纸篓",
    status: (s) => `${s.trash_count} 项`,
    active: () => false,
  },
  {
    key: "hide_dock",
    label: "隐藏 Dock",
    status: (s) => (s.hide_dock ? "已隐藏" : "显示中"),
    active: (s) => s.hide_dock,
  },
  {
    key: "show_hidden_files",
    label: "显示隐藏文件",
    status: (s) => (s.show_hidden_files ? "显示中" : "隐藏中"),
    active: (s) => s.show_hidden_files,
  },
  {
    key: "screen_clean",
    label: "屏幕清洁",
    status: (s) => (s.screen_clean ? "已收起窗口" : "一键收起"),
    active: (s) => s.screen_clean,
  },
  {
    key: "stage_manager",
    label: "台前调度",
    status: (s) => (s.stage_manager ? "开" : "关"),
    active: (s) => s.stage_manager,
    // macOS 13 以下不支持，直接不显示
    hidden: (s) => !s.stage_manager_available,
  },
  {
    key: "lock_screen",
    label: "锁定屏幕",
    status: () => "点击锁定",
    active: () => false,
  },
  {
    key: "clear_clipboard",
    label: "清空剪贴板",
    status: () => "点击清空",
    active: () => false,
  },
  {
    key: "screensaver",
    label: "屏幕保护",
    status: () => "点击启动",
    active: () => false,
  },
  {
    key: "close_all_apps",
    label: "关闭所有应用",
    status: () => "逐个退出",
    active: () => false,
  },
  {
    key: "flush_dns",
    label: "刷新 DNS",
    status: () => "点击刷新",
    active: () => false,
  },
  {
    key: "system_screenshot",
    label: "截图 / 录屏",
    status: () => "打开系统工具栏",
    active: () => false,
  },
];

export const APPEARANCE_TEXT: Record<AppearanceMode, string> = {
  light: "浅色 · 点按切深色",
  dark: "深色 · 点按切自动",
  auto: "自动 · 日出日落",
};

interface Props {
  states: ToggleStates | null;
  busyKey: string | null;
  onToggle: (key: ToggleKey) => void;
  /** 外观三态，独立于全量状态轮询（系统自动切换时也能跟上） */
  appearanceMode?: AppearanceMode | null;
  /** 覆盖某项的状态副标题（用于一次性操作反馈，如「已刷新」） */
  statusOverrides?: Partial<Record<ToggleKey, string>>;
}

export function ToggleGrid({
  states,
  busyKey,
  onToggle,
  appearanceMode,
  statusOverrides,
}: Props) {
  const visible = states ? ITEMS.filter((it) => !it.hidden?.(states)) : ITEMS;
  return (
    <div className="grid grid-cols-2 gap-2">
      {visible.map((it) => {
        const isAppearance = it.key === "dark_mode" && appearanceMode;
        const status =
          statusOverrides?.[it.key] ??
          (isAppearance
            ? APPEARANCE_TEXT[appearanceMode!]
            : states
              ? it.status(states)
              : "—");
        const active = isAppearance
          ? appearanceMode !== "light"
          : states
            ? it.active(states)
            : false;
        const busy = busyKey === it.key;
        return (
          <button
            key={it.key}
            onClick={() => onToggle(it.key)}
            disabled={busy}
            className="flex flex-col items-start rounded-xl border border-white/10 bg-white/5 px-3 py-2.5 text-left transition hover:bg-white/10 active:scale-[0.98] disabled:opacity-50"
          >
            <div className="flex w-full items-center justify-between">
              <span className="text-[13px] font-medium text-white/90">
                {it.label}
              </span>
              <StatusDot on={active} />
            </div>
            <span className="mt-1 text-[11px] text-white/50">
              {busy ? "处理中…" : status}
            </span>
          </button>
        );
      })}
    </div>
  );
}
