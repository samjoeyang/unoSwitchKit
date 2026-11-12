import { useCallback, useEffect, useState } from "react";
import { api } from "./ipc";
import { ALL_TOGGLE_KEYS, type ToggleKey } from "./components/ToggleGrid";

/**
 * 开关卡片的自定义顺序，复用 v0.1.1 起就存在、但前端从未接线的
 * settings.toggle_order 字段。空数组 = 默认顺序。
 *
 * 规范化规则：过滤已删除/非法 key，补齐新增 key（新版本加开关时不丢位置，
 * 也不会因为旧配置里残留未知 key 而崩）。
 */
function normalize(saved: unknown): ToggleKey[] {
  const valid = new Set<string>(ALL_TOGGLE_KEYS);
  const out: ToggleKey[] = [];
  if (Array.isArray(saved)) {
    for (const k of saved) {
      if (typeof k === "string" && valid.has(k) && !out.includes(k as ToggleKey)) {
        out.push(k as ToggleKey);
      }
    }
  }
  for (const k of ALL_TOGGLE_KEYS) {
    if (!out.includes(k)) out.push(k);
  }
  return out;
}

export function useToggleOrder() {
  const [order, setOrder] = useState<ToggleKey[]>(ALL_TOGGLE_KEYS);

  useEffect(() => {
    let alive = true;
    api
      .getSettings()
      .then((s) => {
        if (alive) setOrder(normalize(s.toggle_order));
      })
      .catch(() => {
        /* 配置读取失败时保持默认顺序，不阻塞面板 */
      });
    return () => {
      alive = false;
    };
  }, []);

  const commit = useCallback(async (next: ToggleKey[]) => {
    setOrder(next);
    try {
      // 读-改-写紧挨着：save_settings 是整对象覆盖，避免用过期快照覆盖
      // 其它组件刚写入的 apikey / 排除列表等字段。
      const s = await api.getSettings();
      await api.saveSettings({ ...s, toggle_order: next });
    } catch {
      /* 配置写入失败不阻塞面板，本次会话内顺序仍生效 */
    }
  }, []);

  const reset = useCallback(async () => {
    setOrder(ALL_TOGGLE_KEYS);
    try {
      const s = await api.getSettings();
      await api.saveSettings({ ...s, toggle_order: [] });
    } catch {
      /* ignore */
    }
  }, []);

  return { order, commit, reset };
}
