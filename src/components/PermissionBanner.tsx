import { useState } from "react";
import { api } from "../ipc";
import { useT, tb, type TranslationKey, type TranslateVars } from "../i18n";
import type { PermissionItem, PermissionState } from "../types";

interface Props {
  state: PermissionState | null;
  onRefresh: () => Promise<void>;
  onAcknowledge: () => Promise<void>;
  /// 本次运行内收起提醒；不持久化，App 重启后自动恢复显示。
  dismissed: boolean;
  onDismiss: () => void;
}

function statusDot(status: PermissionItem["status"]) {
  if (status === "granted") return "bg-emerald-400";
  if (status === "denied") return "bg-rose-400";
  return "bg-white/30";
}

type T = (key: TranslationKey, vars?: TranslateVars) => string;

function statusText(status: PermissionItem["status"], t: T) {
  if (status === "granted") return t("已授权");
  if (status === "denied") return t("未生效");
  return t("未检查");
}

/// 换包或检测到权限失效时出现在面板顶部，把「重新授权」变成可操作流程。
/// 显示条件完全由后端状态驱动：签名哈希变化 或 存在未生效的权限项。
export function PermissionBanner({
  state,
  onRefresh,
  onAcknowledge,
  dismissed,
  onDismiss,
}: Props) {
  const t = useT();
  const [expanded, setExpanded] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [tip, setTip] = useState<string | null>(null);

  if (!state || dismissed) return null;
  const denied = state.items.filter((i) => i.status === "denied");
  if (!state.app_changed && denied.length === 0) return null;

  async function run(key: string, fn: () => Promise<unknown>, ok: string) {
    setBusy(key);
    setTip(null);
    try {
      await fn();
      setTip(ok);
    } catch (e) {
      setTip(t("操作失败：{detail}", { detail: tb(e) }));
    } finally {
      setBusy(null);
    }
  }

  const btn =
    "rounded bg-white/10 px-2 py-1 text-[10px] text-white/75 transition hover:bg-white/20 disabled:opacity-40";

  return (
    <div className="mb-2 relative rounded-xl border border-amber-400/30 bg-amber-400/10 p-2.5">
      <button
        aria-label={t("仅本次运行隐藏提醒")}
        title={t("仅本次运行隐藏，重启 App 后会再次显示")}
        disabled={busy !== null}
        onClick={onDismiss}
        className="absolute right-1.5 top-1.5 flex h-5 w-5 items-center justify-center rounded text-white/40 transition hover:bg-white/10 hover:text-white/80"
      >
        <svg viewBox="0 0 16 16" className="h-3 w-3" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round">
          <path d="M4 4l8 8M12 4l-8 8" />
        </svg>
      </button>
      <div className="flex items-start gap-2">
        <span className="mt-[2px] text-[12px] leading-none text-amber-300">!</span>
        <div className="min-w-0 flex-1">
          <div className="text-[12px] font-medium text-amber-200">
            {state.app_changed
              ? t("应用已更新，需要重新授权")
              : t("有 {count} 项系统权限未生效", { count: denied.length })}
          </div>
          <div className="mt-1 text-[10px] leading-snug text-white/55">
            {t("首次安装需逐项授权；分发包为 ad-hoc 签名，每次换包还会产生新签名，macOS 把它当作新 App，系统设置里的开关可能仍显示「开」，但授权已失效且不会重新弹窗。")}
          </div>
          <div className="mt-1.5 text-[10px] leading-snug text-white/45">
            {t("退出 App → 在终端执行下面命令 → 重开 App → 逐项触发功能并允许弹窗：")}
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
                  t("已复制重置命令到剪贴板")
                )
              }
            >
              {busy === "copy" ? t("复制中…") : t("复制命令")}
            </button>
            <button
              disabled={busy !== null}
              className={btn}
              onClick={() => run("open", () => api.openPrivacySettings(), t("已打开系统设置"))}
            >
              {t("打开系统设置")}
            </button>
            <button
              disabled={busy !== null}
              className={btn}
              onClick={() => run("check", onRefresh, t("已重新检查权限"))}
            >
              {busy === "check" ? t("检查中…") : t("检查权限")}
            </button>
            <button
              disabled={busy !== null}
              className={btn}
              onClick={() =>
                run(
                  "ack",
                  onAcknowledge,
                  t("已记录当前签名；未生效项仍需手动授权，可点右上角 × 暂时收起")
                )
              }
            >
              {t("已处理")}
            </button>
          </div>

          {tip && (
            <div className="mt-1.5 text-[10px] leading-snug text-white/60">{tip}</div>
          )}

          <div className="mt-2 flex items-center justify-between gap-2">
            <button
              onClick={() => setExpanded((v) => !v)}
              className="text-[10px] text-white/45 transition hover:text-white/75"
            >
              {expanded ? t("收起权限状态") : t("查看权限状态")}（
              {state.probed ? t("已检查") : t("未检查主动项")}）
            </button>
            <span className="shrink-0 text-[9px] text-white/30">
              {t("重启 App 后再次提醒")}
            </span>
          </div>

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
                        {tb(it.label)}
                      </span>
                      <span className="shrink-0 text-[9px] text-white/40">
                        {statusText(it.status, t)}
                      </span>
                    </div>
                    <div className="text-[9px] leading-snug text-white/35">
                      {tb(it.detail)}
                    </div>
                  </div>
                </div>
              ))}
              {state.cdhash && (
                <div className="pt-1 text-[9px] text-white/30">
                  {t("当前签名 {hash}… · {bundleId}", {
                    hash: state.cdhash,
                    bundleId: state.bundle_id,
                  })}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
