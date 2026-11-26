import { useEffect, useState } from "react";
import { ToggleGrid, type ToggleKey } from "./components/ToggleGrid";
import { ResourcePanel } from "./components/ResourcePanel";
import { CleanupPanel } from "./components/CleanupPanel";
import { BluetoothSection } from "./components/BluetoothSection";
import { BalanceSection } from "./components/BalanceSection";
import { QrCodeSection } from "./components/QrCodeSection";
import { SettingsSection } from "./components/SettingsSection";
import { StatusDot } from "./components/StatusDot";
import { PermissionBanner } from "./components/PermissionBanner";
import { api } from "./ipc";
import { checkAndInstall } from "./update";
import { useToggleOrder } from "./useToggleOrder";
import { loadAuth } from "./authStore";
import { gateFrontend, setRequireLoginHandler } from "./authGate";
import { getVersion } from "@tauri-apps/api/app";
import { useT, t as translateNow, tb } from "./i18n";
import type { AppearanceMode, PermissionState, ToggleStates } from "./types";

const NEXT_APPEARANCE: Record<AppearanceMode, AppearanceMode> = {
  light: "dark",
  dark: "auto",
  auto: "light",
};

export default function App() {
  // 组件内用 tr：effect 内有局部变量 t（定时器句柄），避免命名遮蔽
  const tr = useT();
  const [states, setStates] = useState<ToggleStates | null>(null);
  // 外观三态单独维护：Auto 由系统按日出/日落切换，面板常开时也要跟上
  const [appearanceMode, setAppearanceMode] = useState<AppearanceMode | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [launch, setLaunch] = useState(false);
  const [sidecar, setSidecar] = useState<boolean | null>(null);
  const [updateEnabled, setUpdateEnabled] = useState(false);
  const [updating, setUpdating] = useState(false);
  const [updateMsg, setUpdateMsg] = useState<string | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  // 开关卡片排序编辑模式：开启后卡片只响应拖动，不触发开关动作
  const [reordering, setReordering] = useState(false);
  // 余额账号变动版本号：设置页新增 / 余额区删除后递增，驱动双方同步
  const [balanceVersion, setBalanceVersion] = useState(0);
  // 设置项操作提示（alert 在 WKWebView 中被禁用，改用面板内提示）
  const [notice, setNotice] = useState<string | null>(null);
  // 「刷新 DNS」的一次性反馈，覆盖 ToggleGrid 对应项的状态副标题
  const [dnsStatus, setDnsStatus] = useState<string | null>(null);
  // 应用版本：运行时从二进制读取（getVersion），避免硬编码与发布版本脱节
  const [appVersion, setAppVersion] = useState("0.4.2");
  // 权限自检：换包后由后端比对签名哈希，驱动顶部重新授权引导
  const [permState, setPermState] = useState<PermissionState | null>(null);
  // 「本次不显示」只在当前进程内生效：不写盘、不进 permission-state.json，
  // App 重启后 webview 重建，提醒自动恢复显示。
  const [bannerDismissed, setBannerDismissed] = useState(false);
  const [permChecking, setPermChecking] = useState(false);
  const [permissionSummary, setPermissionSummary] = useState<string | null>(null);
  // 登录态由 authStore 全局订阅，Rust 是门控权威，前端仅用于 UI 展示与引导

  const {
    order: toggleOrder,
    commit: commitToggleOrder,
    reset: resetToggleOrder,
  } = useToggleOrder();

  async function refresh() {
    try {
      const s = await api.getToggleStates();
      setStates(s);
      setAppearanceMode(s.appearance_mode);
    } catch {
      /* 面板尚未就绪时忽略 */
    }
  }
  async function loadLaunch() {
    try {
      setLaunch(await api.isLaunchAtLogin());
    } catch {
      /* ignore */
    }
  }
  async function loadSidecar() {
    try {
      const s = await api.sidecarStatus();
      setSidecar(s.running);
    } catch {
      setSidecar(null);
    }
  }
  async function loadUpdateConfig() {
    try {
      const cfg = await api.getUpdateConfig();
      setUpdateEnabled(cfg.enabled);
    } catch {
      setUpdateEnabled(false);
    }
  }
  async function loadPermissionState() {
    try {
      setPermState(await api.getPermissionState());
    } catch {
      /* 权限查询失败不阻塞面板 */
    }
  }
  /// 主动检查：会尝试触发 AppleEvents 授权弹窗，仅在用户点击时执行。
  async function checkPermissions() {
    // 用户主动检查权限时要看到结果，取消「本次不显示」，让顶部引导重新出现。
    setBannerDismissed(false);
    setPermChecking(true);
    try {
      const s = await api.refreshPermissions();
      setPermState(s);
      const bad = s.items.filter((i) => i.status === "denied");
      setPermissionSummary(
        bad.length
          ? tr("发现 {count} 项未生效，详见面板顶部提示", { count: bad.length })
          : tr("未发现明确失效项（灰色项需在实际使用时由系统弹窗确认）")
      );
    } catch (e) {
      setPermissionSummary(tr("检查失败：{detail}", { detail: tb(e) }));
    } finally {
      setPermChecking(false);
    }
  }
  async function refreshPermissions() {
    await checkPermissions();
  }
  async function acknowledgePermissions() {
    await api.acknowledgePermissionChange();
    await loadPermissionState();
  }

  useEffect(() => {
    refresh();
    loadLaunch();
    loadSidecar();
    loadUpdateConfig();
    loadPermissionState();
    loadAuth();
    // 任意面板触发登录层功能时：打开设置面板并滚到登录区
    setRequireLoginHandler(() => {
      setShowSettings(true);
      // 回调只在挂载时注册一次，用模块级 t() 读取当前语言，避免闭包过期
      setNotice(translateNow("该功能需要登录，请在「账号」处登录"));
      // 设置面板自身有 60/260ms 两次贴底滚动，这里放到其后，避免被覆盖
      window.setTimeout(() => {
        document.getElementById("auth-section")?.scrollIntoView({ block: "nearest" });
      }, 340);
    });
    getVersion().then(setAppVersion).catch(() => {});
    const t = setInterval(loadSidecar, 5000);
    // 自动外观由系统切换（可能延迟到闲置时），面板常开时用轻量命令跟上，
    // 不跑 get_toggle_states 的十余次外部进程调用。
    const appearanceTimer = setInterval(async () => {
      if (document.hidden) return;
      try {
        setAppearanceMode(await api.getAppearanceMode());
      } catch {
        /* 忽略 */
      }
    }, 30000);
    // 登录态定期同步：Rust 侧会话可能因续期失效被清除，前端此前只在启动时
    // 拉一次，导致「设置显示已登录、功能却被拒」。60s 一次本地 IPC，零网络。
    const authTimer = setInterval(() => {
      if (!document.hidden) void loadAuth();
    }, 60000);
    // 面板每次重新显示时刷新（避免托盘弹出后显示过期状态，如废纸篓计数）
    const onVis = () => {
      if (!document.hidden) {
        refresh();
        loadPermissionState();
        void loadAuth();
      }
    };
    document.addEventListener("visibilitychange", onVis);
    return () => {
      clearInterval(t);
      clearInterval(appearanceTimer);
      clearInterval(authTimer);
      document.removeEventListener("visibilitychange", onVis);
    };
  }, []);

  // 刷新 DNS 的一次性反馈，3 秒后还原默认副标题
  useEffect(() => {
    if (!dnsStatus) return;
    const t = window.setTimeout(() => setDnsStatus(null), 3000);
    return () => window.clearTimeout(t);
  }, [dnsStatus]);

  // 设置面板展开在底栏上方，展开后滚到底部，保证面板与底栏都在可视区内
  useEffect(() => {
    if (!showSettings) return;
    const el = document.getElementById("root");
    if (!el) return;
    // 设置面板内的 Key 数量是异步读取的，布局会二次变化，故分两次贴底
    const stick = () => {
      el.scrollTop = el.scrollHeight;
    };
    const timers = [60, 260].map((ms) => window.setTimeout(stick, ms));
    return () => timers.forEach((t) => window.clearTimeout(t));
  }, [showSettings]);

  async function onToggle(key: ToggleKey) {
    // 「刷新 DNS」属登录层功能：未登录时引导到设置登录入口。
    // 外观/隐藏桌面等免费开关不受影响。
    if (key === "flush_dns" && !gateFrontend()) return;
    setBusy(key);
    try {
      if (key === "dark_mode") {
        const current = appearanceMode ?? states?.appearance_mode ?? "light";
        setAppearanceMode(await api.setAppearanceMode(NEXT_APPEARANCE[current]));
      } else if (key === "system_screenshot") {
        await api.openSystemScreenshot();
      } else if (key === "desktop_hidden") await api.toggleHideDesktop();
      else if (key === "prevent_sleep") await api.togglePreventSleep();
      else if (key === "empty_trash") await api.emptyTrash();
      else if (key === "hide_dock") await api.toggleHideDock();
      else if (key === "show_hidden_files") await api.toggleShowHiddenFiles();
      else if (key === "screen_clean") await api.toggleScreenClean();
      else if (key === "stage_manager") await api.toggleStageManager();
      else if (key === "lock_screen") await api.lockScreen();
      else if (key === "clear_clipboard") await api.clearClipboard();
      else if (key === "screensaver") await api.startScreensaver();
      else if (key === "close_all_apps") await api.closeAllApps();
      else if (key === "flush_dns") {
        try {
          await api.flushDns();
          setDnsStatus(tr("已刷新"));
        } catch {
          setDnsStatus(tr("刷新失败"));
        }
      }
      await refresh();
    } catch (e) {
      alert(tr("操作失败：{detail}", { detail: tb(e) }));
    } finally {
      setBusy(null);
    }
  }

  async function toggleLaunch() {
    const next = !launch;
    setNotice(null);
    try {
      await api.setLaunchAtLogin(next);
      setLaunch(next);
      setNotice(next ? tr("已开启开机自启") : tr("已关闭开机自启"));
    } catch (e) {
      setNotice(tr("自启设置失败：{detail}", { detail: tb(e) }));
    }
  }

  async function onCheckUpdate() {
    if (!updateEnabled) {
      setUpdateMsg(tr("更新服务尚未配置（暂未提供线上地址）"));
      return;
    }
    setUpdating(true);
    setUpdateMsg(tr("正在检查更新…"));
    try {
      const r = await checkAndInstall();
      setUpdateMsg(r.message);
    } catch (e) {
      setUpdateMsg(tr("更新失败：{detail}", { detail: tb(e) }));
    } finally {
      setUpdating(false);
    }
  }

  return (
    <div className="flex min-h-full w-full flex-col rounded-2xl border border-white/10 bg-zinc-900/70 p-3 text-white shadow-2xl backdrop-blur-xl">
      <div className="mb-2 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="text-[15px] font-semibold tracking-wide">
            SwitchKit
          </span>
          {sidecar === true && (
            <span className="rounded bg-white/10 px-1.5 py-0.5 text-[9px] text-white/60">
              sidecar
            </span>
          )}
        </div>
        <span className="text-[10px] text-white/40">v{appVersion}</span>
      </div>

      <PermissionBanner
        state={permState}
        onRefresh={refreshPermissions}
        onAcknowledge={acknowledgePermissions}
        dismissed={bannerDismissed}
        onDismiss={() => setBannerDismissed(true)}
      />

      {reordering && (
        <div className="mb-2 flex items-center justify-between rounded-lg border border-indigo-400/30 bg-indigo-500/10 px-2.5 py-1.5 text-[11px] text-indigo-100">
          <span>{tr("拖动卡片调整顺序，自动保存")}</span>
          <button
            onClick={() => void resetToggleOrder()}
            className="rounded px-1.5 py-0.5 text-indigo-200/80 transition hover:bg-white/10"
          >
            {tr("恢复默认")}
          </button>
        </div>
      )}

      <ToggleGrid
        states={states}
        busyKey={busy}
        onToggle={onToggle}
        appearanceMode={appearanceMode}
        statusOverrides={dnsStatus ? { flush_dns: dnsStatus } : undefined}
        editMode={reordering}
        order={toggleOrder}
        onReorder={(next) => void commitToggleOrder(next)}
      />
      <ResourcePanel />
      <CleanupPanel />
      <BluetoothSection />
      <BalanceSection
        version={balanceVersion}
        onChanged={() => setBalanceVersion((v) => v + 1)}
      />
      <QrCodeSection />

      {showSettings && (
        <SettingsSection
          version={appVersion}
          launch={launch}
          onToggleLaunch={toggleLaunch}
          updating={updating}
          updateMsg={updateMsg}
          onCheckUpdate={onCheckUpdate}
          balanceVersion={balanceVersion}
          notice={notice}
          permChecking={permChecking}
          permissionSummary={permissionSummary}
          onCheckPermissions={checkPermissions}
          onBalanceChanged={() => setBalanceVersion((v) => v + 1)}
          onClose={() => {
            setShowSettings(false);
            setNotice(null);
          }}
        />
      )}

      <div className="mt-3 flex items-center justify-between border-t border-white/10 pt-2">
        {/* 「设置」区块靠左：设置 / 排序 / 退出；守护状态独立靠右 */}
        <div className="flex items-center gap-1 rounded-lg bg-white/5 p-0.5 text-[11px]">
          <button
            onClick={() => setShowSettings((v) => !v)}
            className={
              "flex items-center gap-1 rounded-md px-2 py-0.5 transition hover:bg-white/10 " +
              (showSettings ? "bg-white/10 text-white/85" : "text-white/60 hover:text-white/90")
            }
          >
            <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="12" cy="12" r="3" />
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
            </svg>
            {tr("设置")}
          </button>
          <button
            onClick={() => setReordering((v) => !v)}
            className={
              "rounded-md px-2 py-0.5 transition hover:bg-white/10 " +
              (reordering
                ? "bg-indigo-500/25 text-indigo-100"
                : "text-white/60 hover:text-white/90")
            }
          >
            {reordering ? tr("完成排序") : tr("排序")}
          </button>
          <button
            onClick={async () => {
              try {
                await api.quitApp();
              } catch {
                /* ignore */
              }
            }}
            className="rounded-md px-2 py-0.5 text-white/50 transition hover:bg-white/10 hover:text-white/80"
          >
            {tr("退出")}
          </button>
        </div>
        <span className="flex items-center gap-1.5 text-[11px] text-white/50">
          <StatusDot on={sidecar === true} /> {tr("原生守护")}
        </span>
      </div>
    </div>
  );
}
