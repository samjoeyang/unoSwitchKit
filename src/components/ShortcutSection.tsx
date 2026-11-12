import { useEffect, useState } from "react";
import { api } from "../ipc";
import type { ShortcutReport } from "../types";

/// 必须与后端 src-tauri/src/shortcuts.rs 的 ACTION_IDS 顺序一致。
const ACTIONS: { id: string; label: string }[] = [
  { id: "toggle_panel", label: "显示 / 隐藏面板" },
  { id: "cycle_appearance", label: "切换外观（浅/深/自动）" },
  { id: "open_screenshot", label: "截图 / 录屏工具栏" },
  { id: "lock_screen", label: "锁定屏幕" },
  { id: "clear_clipboard", label: "清空剪贴板" },
  { id: "screensaver", label: "启动屏幕保护" },
  { id: "empty_trash", label: "清空废纸篓" },
  { id: "hide_desktop", label: "隐藏桌面图标" },
  { id: "hide_dock", label: "隐藏 / 显示 Dock" },
];

/// 与后端 is_supported_main_key 白名单一致：只收普通键，媒体键/亮度键不录。
const MAIN_KEY =
  /^(Key[A-Z]|Digit[0-9]|F([1-9]|1[0-2])|Space|Enter|Tab|Escape|Backspace|Delete|Home|End|PageUp|PageDown|ArrowUp|ArrowDown|ArrowLeft|ArrowRight|Minus|Equal|BracketLeft|BracketRight|Semicolon|Quote|Comma|Period|Slash|Backslash|Backquote)$/;

const MOD_LABEL: Record<string, string> = {
  Command: "⌘",
  Cmd: "⌘",
  CommandOrControl: "⌘",
  Control: "⌃",
  Ctrl: "⌃",
  Alt: "⌥",
  Option: "⌥",
  Shift: "⇧",
};

const KEY_LABEL: Record<string, string> = {
  Space: "空格",
  Enter: "↩",
  Tab: "⇥",
  Escape: "Esc",
  Backspace: "⌫",
  Delete: "⌦",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backslash: "\\",
  Backquote: "`",
};

function displayCombo(combo: string): string {
  return combo
    .split("+")
    .map((part) => {
      if (MOD_LABEL[part]) return MOD_LABEL[part];
      if (part.startsWith("Key")) return part.slice(3);
      if (part.startsWith("Digit")) return part.slice(5);
      return KEY_LABEL[part] ?? part;
    })
    .join(" ");
}

/// 设置页内的全局快捷键配置：录制按键 → 持久化 → 后端逐项反馈注册结果。
export function ShortcutSection() {
  const [bindings, setBindings] = useState<Record<string, string>>({});
  const [capturing, setCapturing] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  useEffect(() => {
    api
      .getSettings()
      .then((s) => setBindings(s.shortcut_bindings ?? {}))
      .catch(() => {});
  }, []);

  async function commit(next: Record<string, string>) {
    setSaving(true);
    try {
      const reports: ShortcutReport[] = await api.setShortcutBindings(next);
      setBindings(next);
      const bad = reports.filter((r) => !r.ok);
      setMsg(
        bad.length
          ? `${bad.length} 项未生效：${bad.map((r) => `${r.shortcut} ${r.error ?? ""}`).join("；")}`
          : "已保存并生效"
      );
    } catch (e) {
      setMsg(`保存失败：${e}`);
    } finally {
      setSaving(false);
    }
  }

  useEffect(() => {
    if (!capturing) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const mods: string[] = [];
      if (e.metaKey) mods.push("Command");
      if (e.ctrlKey) mods.push("Control");
      if (e.altKey) mods.push("Alt");
      if (e.shiftKey) mods.push("Shift");
      if (e.key === "Escape" && mods.length === 0) {
        setCapturing(null);
        setMsg("已取消录制");
        return;
      }
      if (mods.length === 0 || !MAIN_KEY.test(e.code)) {
        setMsg("请按「至少一个修饰键 + 一个普通按键」，Esc 取消");
        return;
      }
      const action = capturing;
      const combo = [...mods, e.code].join("+");
      setCapturing(null);
      void commit({ ...bindings, [action]: combo });
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [capturing, bindings]);

  const btn =
    "shrink-0 rounded bg-white/10 px-1.5 py-0.5 text-[10px] text-white/65 transition hover:bg-white/20 disabled:opacity-40";

  return (
    <div className="py-2">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-[12px] text-white/85">全局快捷键</div>
          <div className="text-[10px] text-white/35">
            组合键无需额外系统权限；按 Esc 取消录制
          </div>
        </div>
        {saving && <span className="text-[10px] text-white/40">保存中…</span>}
      </div>

      <div className="mt-1.5 divide-y divide-white/5 rounded-lg border border-white/5 bg-black/10 px-2">
        {ACTIONS.map((a) => {
          const combo = bindings[a.id];
          return (
            <div key={a.id} className="flex items-center justify-between gap-1 py-1.5">
              <span className="min-w-0 truncate text-[10px] text-white/70">
                {a.label}
              </span>
              <div className="flex shrink-0 items-center gap-1">
                <span
                  className={
                    "min-w-[52px] text-right font-mono text-[10px] " +
                    (capturing === a.id
                      ? "text-indigo-300"
                      : combo
                        ? "text-white/75"
                        : "text-white/25")
                  }
                >
                  {capturing === a.id
                    ? "请按键…"
                    : combo
                      ? displayCombo(combo)
                      : "未设置"}
                </span>
                <button
                  onClick={() => {
                    setMsg(null);
                    setCapturing(capturing === a.id ? null : a.id);
                  }}
                  disabled={saving}
                  className={btn}
                >
                  {capturing === a.id ? "取消" : "设置"}
                </button>
                {combo && (
                  <button
                    onClick={() => {
                      const next = { ...bindings };
                      delete next[a.id];
                      void commit(next);
                    }}
                    disabled={saving}
                    className={btn}
                  >
                    清除
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </div>

      <div className="mt-1 text-[10px] leading-snug text-white/30">
        仅支持带修饰键的普通组合键；媒体键、单键会触发输入监控授权，已排除。
        被系统或其它应用占用的组合会提示注册失败。
      </div>
      {msg && (
        <div className="mt-1 text-[10px] leading-snug text-white/45">{msg}</div>
      )}
    </div>
  );
}
