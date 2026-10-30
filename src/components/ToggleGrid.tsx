import { StatusDot } from "./StatusDot";
import type { ToggleStates } from "../types";

export type ToggleKey =
  | "dark_mode"
  | "desktop_hidden"
  | "prevent_sleep"
  | "empty_trash";

interface Item {
  key: ToggleKey;
  label: string;
  status: (s: ToggleStates) => string;
  active: (s: ToggleStates) => boolean;
}

const ITEMS: Item[] = [
  {
    key: "dark_mode",
    label: "深色模式",
    status: (s) => (s.dark_mode ? "开" : "关"),
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
];

interface Props {
  states: ToggleStates | null;
  busyKey: string | null;
  onToggle: (key: ToggleKey) => void;
}

export function ToggleGrid({ states, busyKey, onToggle }: Props) {
  return (
    <div className="grid grid-cols-2 gap-2">
      {ITEMS.map((it) => {
        const status = states ? it.status(states) : "—";
        const active = states ? it.active(states) : false;
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
