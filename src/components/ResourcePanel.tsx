import { useEffect, useState, type ReactNode } from "react";
import type { SystemStats } from "../types";
import { api } from "../ipc";

function fmtMB(mb: number) {
  return `${(mb / 1024).toFixed(2)} GB`;
}

function fmtBytes(b: number): string {
  if (b >= 1_073_741_824) return `${(b / 1_073_741_824).toFixed(1)} GB`;
  if (b >= 1_048_576) return `${(b / 1_048_576).toFixed(0)} MB`;
  if (b >= 1024) return `${(b / 1024).toFixed(0)} KB`;
  return `${b} B`;
}

function fmtSpeed(bps: number): string {
  if (bps >= 1_000_000) return `${(bps / 1_000_000).toFixed(1)} MB/s`;
  if (bps >= 1000) return `${(bps / 1000).toFixed(0)} KB/s`;
  return `${bps} B/s`;
}

function StatBar({
  label,
  value,
  pct,
  bar,
  action,
}: {
  label: string;
  value: string;
  pct: number;
  bar: string;
  action?: ReactNode;
}) {
  const p = Math.min(100, Math.max(0, pct));
  return (
    <div className="min-w-0 rounded-xl border border-white/10 bg-white/5 px-3 py-2">
      <div className="flex justify-between items-baseline">
        <span className="text-[10px] text-white/50">{label}</span>
        <span className="text-[13px] font-medium text-white/90">{value}</span>
      </div>
      <div className="mt-1 h-1.5 rounded-full bg-white/10 overflow-hidden">
        <div className={`h-full ${bar}`} style={{ width: `${p}%` }} />
      </div>
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

/** 上下行合并为一个面板，两个速率用 "/" 分隔 */
function NetPanel({ down, up }: { down: string; up: string }) {
  return (
    <div className="flex min-w-0 flex-col justify-between rounded-xl border border-white/10 bg-white/5 px-3 py-2">
      <span className="text-[10px] text-white/50">↓ 下行 / ↑ 上行</span>
      <div className="mt-1 text-[11px] font-medium leading-tight">
        <span className="text-emerald-200">{down}</span>
        <span className="text-white/40"> / </span>
        <span className="text-sky-200">{up}</span>
      </div>
    </div>
  );
}

export function ResourcePanel() {
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string>("");
  const [stats, setStats] = useState<SystemStats | null>(null);

  // 实时系统监控：每 2s 轮询一次（面板隐藏时跳过），展示 CPU/内存/网络上下行。
  useEffect(() => {
    let alive = true;
    const tick = async () => {
      if (document.hidden) return;
      try {
        const s = await api.getSystemStats();
        if (alive) setStats(s);
      } catch {
        /* 面板尚未就绪时忽略 */
      }
    };
    tick();
    const t = setInterval(tick, 2000);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, []);

  async function release() {
    setBusy(true);
    setMsg("");
    try {
      const r = await api.releaseMemory();
      const freedCacheMb = Math.max(0, r.before.cached_mb - r.after.cached_mb);
      setMsg(
        `内存已释放：回收缓存 ${fmtMB(freedCacheMb)}；已用（不含缓存）${fmtMB(r.before.used_mb)} → ${fmtMB(r.after.used_mb)}`,
      );
    } catch (e) {
      setMsg(`释放失败：${e}`);
    } finally {
      setBusy(false);
    }
  }

  const memPct =
    stats && stats.mem_total_bytes > 0
      ? (stats.mem_used_bytes / stats.mem_total_bytes) * 100
      : 0;

  return (
    <div className="mt-3 space-y-2">
      <div className="grid grid-cols-2 gap-2">
        <StatBar
          label="CPU"
          value={stats ? `${stats.cpu_percent.toFixed(0)}%` : "—"}
          pct={stats ? stats.cpu_percent : 0}
          bar="bg-amber-400"
        />
        <NetPanel
          down={stats ? fmtSpeed(stats.net_down_bps) : "—"}
          up={stats ? fmtSpeed(stats.net_up_bps) : "—"}
        />
      </div>
      <StatBar
        label="内存"
        value={
          stats
            ? `${fmtBytes(stats.mem_used_bytes)} / ${fmtBytes(stats.mem_total_bytes)}`
            : "—"
        }
        pct={memPct}
        bar="bg-violet-400"
        action={
          <>
            <div className="flex justify-end">
              <button
                onClick={release}
                disabled={busy}
                className="rounded-lg border border-white/10 bg-indigo-500/15 px-2.5 py-1 text-[11px] font-medium text-indigo-100 hover:bg-indigo-500/25 disabled:opacity-50"
              >
                {busy ? "释放中…" : "释放内存"}
              </button>
            </div>
            {msg && (
              <div className="mt-1.5 text-[11px] text-white/60">{msg}</div>
            )}
          </>
        }
      />
    </div>
  );
}
