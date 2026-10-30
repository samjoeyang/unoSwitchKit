import { useEffect, useState } from "react";
import { api } from "../ipc";
import { DeepSeekKeyForm } from "./DeepSeekKeyForm";

interface Props {
  version: string;
  /// 开机自启当前状态
  launch: boolean;
  onToggleLaunch: () => void;
  /// 在线更新
  updating: boolean;
  updateMsg: string | null;
  onCheckUpdate: () => void;
  /// DeepSeek Key 变化后通知外部刷新（余额区随之出现）
  onDeepSeekChanged: () => void;
  /// 外部 Key 变动的版本号，用于重新读取 Key 数量
  dsVersion: number;
  /// 设置项操作提示（如开机自启切换结果）
  notice?: string | null;
  onClose: () => void;
}

/// 设置面板：开机自启、检查更新、DeepSeek API Key（仅首个 Key 在此时提供入口）。
export function SettingsSection({
  version,
  launch,
  onToggleLaunch,
  updating,
  updateMsg,
  onCheckUpdate,
  onDeepSeekChanged,
  dsVersion,
  notice,
  onClose,
}: Props) {
  const [keyCount, setKeyCount] = useState<number | null>(null);
  const [addingKey, setAddingKey] = useState(false);

  useEffect(() => {
    (async () => {
      try {
        const s = await api.getSettings();
        setKeyCount((s.deepseek_keys ?? []).length);
      } catch {
        setKeyCount(null);
      }
    })();
  }, [dsVersion]);

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

        {/* DeepSeek API Key：仅在尚无 Key 时于设置中提供首次添加入口 */}
        {keyCount === 0 && (
          <div className="py-2">
            <div className="flex items-center justify-between">
              <div>
                <div className={rowLabel}>DeepSeek API Key</div>
                <div className={rowHint}>添加后显示余额，每 60 秒自动刷新</div>
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
                <DeepSeekKeyForm
                  onSaved={() => {
                    setAddingKey(false);
                    onDeepSeekChanged();
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
