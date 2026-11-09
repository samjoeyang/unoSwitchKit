import { useEffect, useState } from "react";
import type { MemoryComparison, SystemStats } from "../types";
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
}: {
  label: string;
  value: string;
  pct: number;
  bar: string;
}) {
  const p = Math.min(100, Math.max(0, pct));
  return (
    <div className="rounded-xl border border-white/10 bg-white/5 px-3 py-2">
      <div className="flex justify-between items-baseline">
        <span className="text-[10px] text-white/50">{label}</span>
        <span className="text-[13px] font-medium text-white/90">{value}</span>
      </div>
      <div className="mt-1 h-1.5 rounded-full bg-white/10 overflow-hidden">
        <div className={`h-full ${bar}`} style={{ width: `${p}%` }} />
      </div>
    </div>
  );
}

export function ResourcePanel() {
  const [busy, setBusy] = useState<string | null>(null);
  const [cmp, setCmp] = useState<MemoryComparison | null>(null);
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
    setBusy("mem");
    setMsg("");
    try {
      const r = await api.releaseMemory();
      setCmp(r);
      setMsg(
        `内存已释放：使用 ${fmtMB(r.before.used_mb)} → ${fmtMB(r.after.used_mb)}`
      );
    } catch (e) {
      setMsg(`释放失败：${e}`);
    } finally {
      setBusy(null);
    }
  }
  async function flush() {
    setBusy("dns");
    setMsg("");
    try {
      await api.flushDns();
      setMsg("DNS 缓存已刷新");
    } catch (e) {
      setMsg(`刷新失败：${e}`);
    } finally {
      setBusy(null);
    }
  }

  const memPct =
    stats && stats.mem_total_bytes > 0
      ? (stats.mem_used_bytes / stats.mem_total_bytes) * 100
      : 0;

  return (
    <div className="mt-3 space-y-2">
      {stats && (
        <div className="space-y-2">
          <StatBar
            label="CPU"
            value={`${stats.cpu_percent.toFixed(0)}%`}
            pct={stats.cpu_percent}
            bar="bg-amber-400"
          />
          <StatBar
            label="内存"
            value={`${fmtBytes(stats.mem_used_bytes)} / ${fmtBytes(
              stats.mem_total_bytes
            )}`}
            pct={memPct}
            bar="bg-violet-400"
          />
          <div className="grid grid-cols-2 gap-2">
            <div className="rounded-xl border border-white/10 bg-white/5 px-3 py-2">
              <div className="text-[10px] text-white/50">↓ 下行</div>
              <div className="text-[13px] font-medium text-emerald-200">
                {fmtSpeed(stats.net_down_bps)}
              </div>
            </div>
            <div className="rounded-xl border border-white/10 bg-white/5 px-3 py-2">
              <div className="text-[10px] text-white/50">↑ 上行</div>
              <div className="text-[13px] font-medium text-sky-200">
                {fmtSpeed(stats.net_up_bps)}
              </div>
            </div>
          </div>
        </div>
      )}

      <div className="grid grid-cols-2 gap-2">
        <button
          onClick={release}
          disabled={busy === "mem"}
          className="rounded-xl border border-white/10 bg-indigo-500/15 px-3 py-2 text-[13px] font-medium text-indigo-100 hover:bg-indigo-500/25 disabled:opacity-50"
        >
          {busy === "mem" ? "释放中…" : "释放内存"}
        </button>
        <button
          onClick={flush}
          disabled={busy === "dns"}
          className="rounded-xl border border-white/10 bg-sky-500/15 px-3 py-2 text-[13px] font-medium text-sky-100 hover:bg-sky-500/25 disabled:opacity-50"
        >
          {busy === "dns" ? "刷新中…" : "刷新 DNS"}
        </button>
      </div>

      {cmp && (
        <div className="rounded-xl border border-white/10 bg-white/5 p-3 text-[11px] text-white/70">
          <div className="flex justify-between">
            <span>内存使用</span>
            <span>
              {fmtMB(cmp.before.used_mb)} → {fmtMB(cmp.after.used_mb)}
            </span>
          </div>
        </div>
      )}
      {msg && <div className="text-[11px] text-white/60">{msg}</div>}
    </div>
  );
}
