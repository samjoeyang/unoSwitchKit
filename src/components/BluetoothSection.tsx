import { useCallback, useEffect, useState } from "react";
import { StatusDot } from "./StatusDot";
import { api } from "../ipc";
import type { BtDevice } from "../types";

/// 蓝牙设备区：列出已配对设备，一键连接 / 断开（系统框架 IOBluetooth，经 Swift 守护）。
export function BluetoothSection() {
  const [devices, setDevices] = useState<BtDevice[] | null>(null);
  const [busyAddr, setBusyAddr] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setDevices(await api.btDevices());
    } catch {
      setDevices(null); // 守护未运行或未授权
    }
  }, []);

  useEffect(() => {
    refresh();
    const onVis = () => {
      if (!document.hidden) refresh();
    };
    document.addEventListener("visibilitychange", onVis);
    return () => document.removeEventListener("visibilitychange", onVis);
  }, [refresh]);

  async function act(d: BtDevice) {
    setBusyAddr(d.address);
    try {
      if (d.connected) await api.btDisconnect(d.address);
      else await api.btConnect(d.address);
      await refresh();
    } catch (e) {
      alert(`蓝牙操作失败：${e}`);
    } finally {
      setBusyAddr(null);
    }
  }

  if (!devices || devices.length === 0) {
    // 守护未运行 / 未授权 / 无已配对设备时整块隐藏，避免噪音
    return null;
  }

  return (
    <div className="mt-3 rounded-xl border border-white/10 bg-white/5 p-3">
      <div className="flex items-center justify-between">
        <span className="text-[13px] font-medium text-white/90">蓝牙设备</span>
        <span className="text-[10px] text-white/40">
          {devices.filter((d) => d.connected).length}/{devices.length} 已连接
        </span>
      </div>
      {/* 设备较多时限高滚动，最多显示约 3 个设备，避免整块过高 */}
      <div className="thin-scroll mt-2 max-h-[134px] space-y-1 overflow-y-auto pr-1">
        {devices.map((d) => {
          const busy = busyAddr === d.address;
          return (
            <div
              key={d.address}
              className="flex items-center justify-between gap-2 rounded-lg px-2 py-1.5 hover:bg-white/5"
            >
              <div className="flex min-w-0 items-center gap-2">
                <StatusDot on={d.connected} />
                <div className="min-w-0">
                  <div className="truncate text-[12px] text-white/85">
                    {d.name}
                  </div>
                  <div className="truncate text-[10px] text-white/40">
                    {d.address}
                  </div>
                </div>
              </div>
              <button
                onClick={() => act(d)}
                disabled={busy}
                className={
                  "shrink-0 rounded-full px-2.5 py-0.5 text-[10px] transition disabled:opacity-50 " +
                  (d.connected
                    ? "bg-emerald-500/20 text-emerald-200 hover:bg-emerald-500/30"
                    : "bg-white/10 text-white/50 hover:bg-white/20")
                }
              >
                {busy ? "连接中…" : d.connected ? "断开" : "连接"}
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
