import { useEffect, useState } from "react";
import { api } from "../ipc";
import { KeyForm } from "./KeyForm";

interface Props {
  version: string;
  /// 开机自启当前状态
  launch: boolean;
  onToggleLaunch: () => void;
  /// 在线更新
  updating: boolean;
  updateMsg: string | null;
  onCheckUpdate: () => void;
  /// 余额账号变化后通知外部刷新（余额区随之出现）
  onBalanceChanged: () => void;
  /// 外部账号变动的版本号，用于重新读取账号数量
  balanceVersion: number;
  /// 设置项操作提示（如开机自启切换结果）
  notice?: string | null;
  onClose: () => void;
}

/// 设置面板：开机自启、检查更新、模型余额账号（仅首个账号在此提供入口）。
export function SettingsSection({
  version,
  launch,
  onToggleLaunch,
  updating,
  updateMsg,
  onCheckUpdate,
  onBalanceChanged,
  balanceVersion,
  notice,
  onClose,
}: Props) {
  const [keyCount, setKeyCount] = useState<number | null>(null);
  const [addingKey, setAddingKey] = useState(false);
  const [proxy, setProxy] = useState("");
  const [savedProxy, setSavedProxy] = useState("");
  const [proxySaving, setProxySaving] = useState(false);

  useEffect(() => {
    (async () => {
      try {
        const s = await api.getSettings();
        setKeyCount((s.provider_keys ?? []).length);
        setProxy(s.http_proxy ?? "");
        setSavedProxy(s.http_proxy ?? "");
      } catch {
        setKeyCount(null);
      }
    })();
  }, [balanceVersion]);

  async function saveProxy() {
    setProxySaving(true);
    try {
      const s = await api.getSettings();
      s.http_proxy = proxy.trim();
      await api.saveSettings(s);
      setSavedProxy(proxy.trim());
    } catch {
      /* 失败时保留输入，便于重试 */
    } finally {
      setProxySaving(false);
    }
  }

  const rowLabel = "text-[12px] text-white/85";
  const rowHint = "text-[10px] text-white/35";
  const actionBtn =
    "shrink-0 rounded bg-white/10 px-2.5 py-1 text-[10px] text-white/70 transition hover:bg-white/20 disabled:opacity-40";

  return (
    <div className="mt-3 rounded-xl border border-white/10 bg-white/5 p-3">
      <div className="flex items-center justify-between">
        <span className="text-[13px] font-medium text-white/90">设置</span>
        <button
          onClick={onClose}
          className="text-[10px] text-white/45 transition hover:text-white/75"
        >
          收起
        </button>
      </div>

      <div className="mt-1 divide-y divide-white/5">
        {/* 开机自启 */}
        <div className="flex items-center justify-between py-2">
          <div>
            <div className={rowLabel}>开机自启</div>
            <div className={rowHint}>登录时自动启动，常驻菜单栏</div>
          </div>
          <button
            role="switch"
            aria-checked={launch}
            onClick={onToggleLaunch}
            className={
              "relative h-[18px] w-[32px] shrink-0 rounded-full transition " +
              (launch ? "bg-indigo-500/80" : "bg-white/15")
            }
          >
            <span
              className={
                "absolute top-[2px] h-[14px] w-[14px] rounded-full bg-white shadow transition-all " +
                (launch ? "left-[16px]" : "left-[2px]")
              }
            />
          </button>
        </div>

        {/* 检查更新 */}
        <div className="py-2">
          <div className="flex items-center justify-between">
            <div>
              <div className={rowLabel}>检查更新</div>
              <div className={rowHint}>当前版本 v{version}</div>
            </div>
            <button onClick={onCheckUpdate} disabled={updating} className={actionBtn}>
              {updating ? "检查中…" : "检查"}
            </button>
          </div>
          {updateMsg && (
            <div className="mt-1 text-[10px] leading-snug text-white/40" title={updateMsg}>
              {updateMsg}
            </div>
          )}
        </div>

        {/* 余额查询代理：部分境外站点直连会被重置 */}
        <div className="py-2">
          <div className="flex items-center justify-between">
            <div>
              <div className={rowLabel}>网络代理</div>
              <div className={rowHint}>
                {savedProxy ? `当前 ${savedProxy}` : "余额查询直连，境外站点可填代理"}
              </div>
            </div>
            <button
              onClick={saveProxy}
              disabled={proxySaving || proxy.trim() === savedProxy}
              className={actionBtn}
            >
              {proxySaving ? "保存中…" : "保存"}
            </button>
          </div>
          <input
            value={proxy}
            onChange={(e) => setProxy(e.target.value)}
            placeholder="http://127.0.0.1:7897（留空直连）"
            className="mt-1.5 w-full rounded border border-white/10 bg-black/20 px-2 py-1 text-[11px] text-white/85 placeholder:text-white/30 focus:border-indigo-400/50 focus:outline-none"
          />
        </div>

        {/* 模型余额账号：仅在尚无账号时于设置中提供首次添加入口 */}
        {keyCount === 0 && (
          <div className="py-2">
            <div className="flex items-center justify-between">
              <div>
                <div className={rowLabel}>模型余额账号</div>
                <div className={rowHint}>
                  DeepSeek / Kimi / 智谱 / 阶跃 / OpenRouter 等，60 秒自动刷新
                </div>
              </div>
              <button
                onClick={() => setAddingKey((v) => !v)}
                className={actionBtn}
              >
                {addingKey ? "取消" : "添加"}
              </button>
            </div>
            {addingKey && (
              <div className="mt-2">
                <KeyForm
                  onSaved={() => {
                    setAddingKey(false);
                    onBalanceChanged();
                  }}
                  onCancel={() => setAddingKey(false)}
                />
              </div>
            )}
          </div>
        )}
      </div>

      {notice && (
        <div className="mt-1.5 border-t border-white/5 pt-1.5 text-[10px] text-white/45">
          {notice}
        </div>
      )}
    </div>
  );
}
