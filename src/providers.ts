/// 支持余额查询的模型平台清单（与 Rust 侧 balance.rs 的 provider 标识一一对应）。
export interface ProviderMeta {
  id: string;
  name: string;
  /// 添加表单里的提示：去哪里拿 Key / 注意事项
  hint: string;
  /// 需要额外填写站点地址（中转站）
  needsBaseUrl?: boolean;
  /// Key 输入框占位符
  placeholder?: string;
}

export const PROVIDERS: ProviderMeta[] = [
  {
    id: "deepseek",
    name: "DeepSeek",
    hint: "platform.deepseek.com → API Keys",
    placeholder: "sk-...",
  },
  {
    id: "kimi",
    name: "Kimi 国内站",
    hint: "platform.moonshot.cn → API Key（人民币）",
    placeholder: "sk-...",
  },
  {
    id: "kimi_intl",
    name: "Kimi 国际站",
    hint: "platform.kimi.ai → API Key",
    placeholder: "sk-...",
  },
  {
    id: "zhipu",
    name: "智谱 GLM",
    hint: "bigmodel.cn → API Keys（查的是按量付费余额，Coding 套餐额度不在此）",
    placeholder: "xxxxx.xxxxx",
  },
  {
    id: "stepfun",
    name: "阶跃星辰",
    hint: "platform.stepfun.com → API Key",
    placeholder: "sk-...",
  },
  {
    id: "novita",
    name: "Novita AI",
    hint: "novita.ai → Settings → API Keys（美元）",
    placeholder: "sk_...",
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    hint: "openrouter.ai/keys（美元，额度 = 购买额 - 已用）",
    placeholder: "sk-or-v1-...",
  },
  {
    id: "ai302",
    name: "302.AI",
    hint: "该 Key 需开通「管理后台」权限才能查余额",
    placeholder: "sk-...",
  },
  {
    id: "aihubmix",
    name: "AiHubMix",
    hint: "需 Manage Key（aihubmix.com → 设置）",
    placeholder: "sk-...",
  },
  {
    id: "custom",
    name: "中转站 / 自定义",
    hint: "填站点地址，按 OpenAI 计费接口或 user/self 额度查询",
    needsBaseUrl: true,
    placeholder: "sk-...",
  },
];

export function providerName(id: string): string {
  return PROVIDERS.find((p) => p.id === id)?.name ?? id;
}
