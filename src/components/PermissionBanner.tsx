import { useState } from "react";
import { api } from "../ipc";
import type { PermissionItem, PermissionState } from "../types";

interface Props {
  state: PermissionState | null;
  onRefresh: () => Promise<void>;
  onAcknowledge: () => Promise<void>;
}

function statusDot(status: PermissionItem["status"]) {
  if (status === "granted") return "bg-emerald-400";
  if (status === "denied") return "bg-rose-400";
  return "bg-white/30";
}

function statusText(status: PermissionItem["status"]) {
  if (status === "granted") return "已授权";
  if (status === "denied") return "未生效";
  return "未检查";
}

/// 换包或检测到权限失效时出现在面板顶部，把「重新授权」变成可操作流程。
/// 显示条件完全由后端状态驱动：签名哈希变化 或 存在 required 且 denied 的项。
export function PermissionBanner({ state, onRefresh, onAcknowledge }: Props) {
  const [expanded, setExpanded] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [tip, setTip] = useState<string | null>(null);

  if (!state) return null;
  const denied = state.items.filter((i) => i.required && i.status === "denied");
  if (!state.app_changed && denied.length === 0) return null;

  async function run(key: string, fn: () => Promise<unknown>, ok: string) {
    setBusy(key);
    setTip(null);
    try {
      await fn();
      setTip(ok);
    } catch (e) {
      setTip(`操作失败：${e}`);
    } finally {
      setBusy(null);
    }
  }

  const btn =
    "rounded bg-white/10 px-2 py-1 text-[10px] text-white/75 transition hover:bg-white/20 disabled:opacity-40";

  return (
    <div className="mb-2 rounded-xl border border-amber-400/30 bg-amber-400/10 p-2.5">
      <div className="flex items-start gap-2">
        <span className="mt-[2px] text-[12px] leading-none text-amber-300">!</span>
        <div className="min-w-0 flex-1">
          <div className="text-[12px] font-medium text-amber-200">
            {state.app_changed
              ? "应用已更新，需要重新授权"
              : `有 ${denied.length} 项系统权限未生效`}
          </div>
          <div className="mt-1 text-[10px] leading-snug text-white/55">
            首次安装需逐项授权；分发包为 ad-hoc 签名，每次换包还会产生新签名，
            macOS 把它当作新 App，系统设置里的开关可能仍显示「开」，
            但授权已失效且不会重新弹窗。
          </div>
          <div className="mt-1.5 text-[10px] leading-snug text-white/45">
            退出 App → 在终端执行下面命令 → 重开 App → 逐项触发功能并允许弹窗：
          </div>
          <code className="mt-1.5 block select-all rounded bg-black/30 px-2 py-1 font-mono text-[10px] text-amber-100">
            {state.reset_command}
          </code>

          <div className="mt-2 flex flex-wrap gap-1.5">
            <button
              disabled={busy !== null}
              className={btn}
              onClick={() =>
                run(
                  "copy",
                  () => api.copyResetCommand(),
                  "已复制重置命令到剪贴板"
                )
              }
            >
              {busy === "copy" ? "复制中…" : "复制命令"}
            </button>
            <button
              disabled={busy !== null}
              className={btn}
              onClick={() => run("open", () => api.openPrivacySettings(), "已打开系统设置")}
            >
              打开系统设置
            </button>
            <button
              disabled={busy !== null}
              className={btn}
              onClick={() => run("check", onRefresh, "已重新检查权限")}
            >
              {busy === "check" ? "检查中…" : "检查权限"}
            </button>
            <button
              disabled={busy !== null}
              className={btn}
              onClick={() =>
                run("ack", onAcknowledge, "已记录当前版本；若仍有红色项请继续授权")
              }
            >
              已处理
            </button>
          </div>

          {tip && (
            <div className="mt-1.5 text-[10px] leading-snug text-white/60">{tip}</div>
          )}

          <button
            onClick={() => setExpanded((v) => !v)}
            className="mt-2 text-[10px] text-white/45 transition hover:text-white/75"
          >
            {expanded ? "收起权限状态" : "查看权限状态"}（
            {state.probed ? "已检查" : "未检查主动项"}）
          </button>

          {expanded && (
            <div className="mt-1.5 divide-y divide-white/5 border-t border-white/10 pt-1">
              {state.items.map((it) => (
                <div key={it.key} className="flex items-start gap-1.5 py-1">
                  <span
                    className={`mt-[4px] h-[6px] w-[6px] shrink-0 rounded-full ${statusDot(
                      it.status
                    )}`}
                  />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center justify-between gap-1">
                      <span className="truncate text-[10px] text-white/80">
                        {it.label}
                      </span>
                      <span className="shrink-0 text-[9px] text-white/40">
                        {statusText(it.status)}
                      </span>
                    </div>
                    <div className="text-[9px] leading-snug text-white/35">
                      {it.detail}
                    </div>
                  </div>
                </div>
              ))}
              {state.cdhash && (
                <div className="pt-1 text-[9px] text-white/30">
                  当前签名 {state.cdhash}… · {state.bundle_id}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
