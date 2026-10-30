//! 多厂商余额查询。
//!
//! 支持：DeepSeek / Kimi（国内 + 国际）/ 智谱 GLM / 阶跃星辰 / Novita AI /
//! OpenRouter / 302.AI / AiHubMix / 中转站自定义（new-api、one-api 系）。
//!
//! 统一约定：GET + `Authorization: Bearer <KEY>`，返回 JSON。全部用系统自带 curl
//! 发请求，避免引入 reqwest 等重型依赖（编译体积/时间）；超时 15s，多 Key 并行查询。
//! 需要代理（部分境外站点在境内直连会 TLS 重置）时由设置里的 `http_proxy` 传入。

use std::process::Command;

use serde_json::Value;

use crate::models::{AccountBalance, BalanceLine, ProviderKey};

const TIMEOUT_SECS: &str = "15";

/// 返回 (明细行, 账户是否可用)
type BalanceResult = Result<(Vec<BalanceLine>, bool), String>;

// ---------------- HTTP 基础 ----------------

struct Resp {
    status: String,
    body: String,
}

/// 发 GET 请求。`proxy` 非空时额外传 `--proxy`（curl 支持 http/https/socks5 代理）。
fn http_get(url: &str, api_key: &str, proxy: &str) -> Result<Resp, String> {
    let auth = format!("Authorization: Bearer {}", api_key.trim());
    let mut cmd = Command::new("curl");
    cmd.args([
        "-sS",
        "-m",
        TIMEOUT_SECS,
        "-H",
        &auth,
        "-H",
        "Accept: application/json",
        "-w",
        "\\n%{http_code}",
    ]);
    let proxy = proxy.trim();
    if !proxy.is_empty() {
        cmd.args(["--proxy", proxy]);
    }
    let out = cmd.arg(url).output().map_err(|e| format!("无法执行 curl：{}", e))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let msg = err.trim();
        return Err(if msg.is_empty() {
            "网络请求失败".to_string()
        } else {
            msg.to_string()
        });
    }

    let raw = String::from_utf8_lossy(&out.stdout).to_string();
    // curl -w "\n%{http_code}" → 最后一行是 HTTP 状态码
    let (body, status) = match raw.rsplit_once('\n') {
        Some((b, c)) => (b.to_string(), c.trim().to_string()),
        None => (raw, String::new()),
    };
    Ok(Resp { status, body })
}

/// 尽量从错误响应里取出可读提示。
fn error_message(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    let candidates = [
        v.get("error").and_then(|e| e.get("message")),
        v.get("message"),
        v.get("msg"),
        v.get("error"),
    ];
    for c in candidates.iter().flatten() {
        if let Some(s) = c.as_str() {
            if !s.trim().is_empty() {
                return Some(s.trim().to_string());
            }
        }
    }
    None
}

fn parse_json(r: Resp) -> Result<Value, String> {
    if r.status != "200" {
        let hint = match r.status.as_str() {
            "401" => "Key 无效或已失效",
            "403" => "Key 无该接口权限",
            "404" => "站点未提供该接口",
            "429" => "请求过于频繁",
            _ => "",
        };
        let detail = error_message(&r.body)
            .unwrap_or_else(|| r.body.chars().take(160).collect::<String>().trim().to_string());
        let code = if r.status.is_empty() { "?" } else { r.status.as_str() };
        let mut msg = format!("HTTP {}", code);
        if !hint.is_empty() {
            msg.push_str("：");
            msg.push_str(hint);
        }
        if !detail.is_empty() {
            msg.push_str(" · ");
            msg.push_str(&detail);
        }
        return Err(msg);
    }
    serde_json::from_str(&r.body).map_err(|e| format!("响应解析失败：{}", e))
}

// ---------------- JSON / 格式化辅助 ----------------

/// 按点号路径取数值（兼容字符串形式的数字）。
fn num(v: &Value, path: &str) -> Option<f64> {
    let mut cur = v;
    for seg in path.split('.') {
        cur = cur.get(seg)?;
    }
    match cur {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// 金额格式化：>=1 保留两位，小额保留四位（余额常出现 0.0032 这类）。
fn amt(n: f64) -> String {
    if n.abs() >= 1.0 {
        format!("{:.2}", n)
    } else {
        format!("{:.4}", n)
    }
}

fn line(label: &str, value: Option<f64>, currency: &str, primary: bool) -> Option<BalanceLine> {
    value.map(|n| BalanceLine {
        label: label.to_string(),
        amount: amt(n),
        currency: currency.to_string(),
        primary,
    })
}

// ---------------- 各厂商实现 ----------------

/// DeepSeek：GET /user/balance
fn deepseek(k: &ProviderKey, proxy: &str) -> BalanceResult {
    let v = parse_json(http_get(
        "https://api.deepseek.com/user/balance",
        &k.api_key,
        proxy,
    )?)?;
    let available = v
        .get("is_available")
        .and_then(|x| x.as_bool())
        .unwrap_or(false);
    let mut lines = Vec::new();
    if let Some(arr) = v.get("balance_infos").and_then(|x| x.as_array()) {
        for info in arr {
            let cur = info
                .get("currency")
                .and_then(|x| x.as_str())
                .unwrap_or("CNY");
            if let Some(l) = line("余额", num(info, "total_balance"), cur, true) {
                lines.push(l);
            }
            if let Some(l) = line("充值", num(info, "topped_up_balance"), cur, false) {
                lines.push(l);
            }
            if let Some(l) = line("赠送", num(info, "granted_balance"), cur, false) {
                lines.push(l);
            }
        }
    }
    Ok((lines, available))
}

/// Kimi / Moonshot：GET {base}/v1/users/me/balance
fn kimi(k: &ProviderKey, proxy: &str, base: &str) -> BalanceResult {
    let url = format!("{}/v1/users/me/balance", base);
    let v = parse_json(http_get(&url, &k.api_key, proxy)?)?;
    let available = num(&v, "data.available_balance");
    let mut lines = Vec::new();
    if let Some(l) = line("可用余额", available, "CNY", true) {
        lines.push(l);
    }
    if let Some(l) = line("现金", num(&v, "data.cash_balance"), "CNY", false) {
        lines.push(l);
    }
    if let Some(l) = line("代金券", num(&v, "data.voucher_balance"), "CNY", false) {
        lines.push(l);
    }
    Ok((lines, available.unwrap_or(0.0) > 0.0))
}

/// 智谱 GLM：GET /api/paas/v4/users/me/balance（旧版资源接口，官方文档未列）
/// 只反映按量付费账户余额；Coding Plan 套餐额度不在此接口。
fn zhipu(k: &ProviderKey, proxy: &str) -> BalanceResult {
    let v = parse_json(http_get(
        "https://open.bigmodel.cn/api/paas/v4/users/me/balance",
        &k.api_key,
        proxy,
    )?)?;
    let balance = num(&v, "balanceInfo.balance");
    let mut lines = Vec::new();
    if let Some(l) = line("余额", balance, "CNY", true) {
        lines.push(l);
    }
    if let Some(l) = line("赠金", num(&v, "balanceInfo.giftBalance"), "CNY", false) {
        lines.push(l);
    }
    if let Some(l) = line(
        "代金券",
        num(&v, "balanceInfo.voucherBalance"),
        "CNY",
        false,
    ) {
        lines.push(l);
    }
    Ok((lines, balance.unwrap_or(0.0) > 0.0))
}

/// 阶跃星辰：GET /v1/accounts
fn stepfun(k: &ProviderKey, proxy: &str) -> BalanceResult {
    let v = parse_json(http_get(
        "https://api.stepfun.com/v1/accounts",
        &k.api_key,
        proxy,
    )?)?;
    let balance = num(&v, "balance");
    let postpaid = v.get("type").and_then(|x| x.as_str()) == Some("postpaid");
    let mut lines = Vec::new();
    if let Some(l) = line("余额", balance, "CNY", true) {
        lines.push(l);
    }
    if let Some(l) = line("充值", num(&v, "total_cash_balance"), "CNY", false) {
        lines.push(l);
    }
    if let Some(l) = line("赠送", num(&v, "total_voucher_balance"), "CNY", false) {
        lines.push(l);
    }
    // 后付费账户余额可为 0 仍可用
    Ok((lines, postpaid || balance.unwrap_or(0.0) > 0.0))
}

/// Novita AI：新路径 /openapi/v1/billing/balance/detail，旧路径 /v3/user/balance 兜底。
/// 金额单位是 1/10000 USD，需除以 10000。
fn novita(k: &ProviderKey, proxy: &str) -> BalanceResult {
    let urls = [
        "https://api.novita.ai/openapi/v1/billing/balance/detail",
        "https://api.novita.ai/v3/user/balance",
    ];
    let mut last_err = String::from("未知错误");
    for url in urls {
        match http_get(url, &k.api_key, proxy).and_then(parse_json) {
            Ok(v) => {
                let available = num(&v, "availableBalance").map(|n| n / 10000.0);
                let mut lines = Vec::new();
                if let Some(l) = line("可用余额", available, "USD", true) {
                    lines.push(l);
                }
                if let Some(l) = line("充值余额", num(&v, "cashBalance").map(|n| n / 10000.0), "USD", false)
                {
                    lines.push(l);
                }
                if let Some(l) = line("信用额度", num(&v, "creditLimit").map(|n| n / 10000.0), "USD", false)
                {
                    lines.push(l);
                }
                return Ok((lines, available.unwrap_or(0.0) > 0.0));
            }
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

/// OpenRouter：/api/v1/credits（购买总额 - 已用）；失败回退 /api/v1/key
fn openrouter(k: &ProviderKey, proxy: &str) -> BalanceResult {
    match http_get("https://openrouter.ai/api/v1/credits", &k.api_key, proxy)
        .and_then(parse_json)
    {
        Ok(v) => {
            let total = num(&v, "data.total_credits").unwrap_or(0.0);
            let used = num(&v, "data.total_usage").unwrap_or(0.0);
            let remaining = total - used;
            let mut lines = Vec::new();
            if let Some(l) = line("剩余额度", Some(remaining), "USD", true) {
                lines.push(l);
            }
            if let Some(l) = line("已用", Some(used), "USD", false) {
                lines.push(l);
            }
            if let Some(l) = line("累计购买", Some(total), "USD", false) {
                lines.push(l);
            }
            Ok((lines, remaining > 0.0))
        }
        Err(e) => {
            // 普通 key 可能只允许 /api/v1/key（按 key 限额）
            let v = parse_json(http_get(
                "https://openrouter.ai/api/v1/key",
                &k.api_key,
                proxy,
            )?)?;
            let remaining = num(&v, "data.limit_remaining");
            let mut lines = Vec::new();
            if let Some(l) = line("剩余额度", remaining, "USD", true) {
                lines.push(l);
            }
            if let Some(l) = line("已用", num(&v, "data.usage"), "USD", false) {
                lines.push(l);
            }
            if lines.is_empty() {
                return Err(e);
            }
            Ok((lines, remaining.map(|r| r > 0.0).unwrap_or(true)))
        }
    }
}

/// 302.AI：GET /dashboard/balance（Key 需开通「管理后台」权限）
fn ai302(k: &ProviderKey, proxy: &str) -> BalanceResult {
    let v = parse_json(http_get(
        "https://api.302.ai/dashboard/balance",
        &k.api_key,
        proxy,
    )?)?;
    let balance = num(&v, "data.balance");
    let mut lines = Vec::new();
    if let Some(l) = line("余额", balance, "CNY", true) {
        lines.push(l);
    }
    Ok((lines, balance.unwrap_or(0.0) > 0.0))
}

/// AiHubMix：/api/user/self（quota ÷ 500000 = USD），失败回退 /dashboard/billing/remain
fn aihubmix(k: &ProviderKey, proxy: &str) -> BalanceResult {
    if let Ok(v) = http_get("https://aihubmix.com/api/user/self", &k.api_key, proxy)
        .and_then(parse_json)
    {
        if let Some(q) = num(&v, "data.quota") {
            let mut lines = Vec::new();
            if let Some(l) = line("余额", Some(q / 500_000.0), "USD", true) {
                lines.push(l);
            }
            if let Some(l) = line(
                "已用",
                num(&v, "data.used_quota").map(|n| n / 500_000.0),
                "USD",
                false,
            ) {
                lines.push(l);
            }
            return Ok((lines, q > 0.0));
        }
    }
    let v = parse_json(http_get(
        "https://aihubmix.com/dashboard/billing/remain",
        &k.api_key,
        proxy,
    )?)?;
    let remain = num(&v, "data.remain")
        .or_else(|| num(&v, "remain"))
        .or_else(|| num(&v, "data"));
    let mut lines = Vec::new();
    if let Some(l) = line("余额", remain, "USD", true) {
        lines.push(l);
    }
    Ok((lines, remain.unwrap_or(0.0) > 0.0))
}

/// 中转站 / 自定义站点。
///
/// 1. 优先 new-api / one-api 的 OpenAI 计费接口：
///    GET {base}/dashboard/billing/subscription → hard_limit_usd
///    GET {base}/dashboard/billing/usage → total_usage（美分）
///    余额 = hard_limit_usd - total_usage / 100
/// 2. 回退 /api/user/self → data.quota ÷ 500000（通常需要控制台 Cookie，Key 直连可能 401）
fn custom(k: &ProviderKey, proxy: &str) -> BalanceResult {
    let raw = k.base_url.trim().trim_end_matches('/');
    if raw.is_empty() {
        return Err("请填写站点地址（如 https://api.example.com）".to_string());
    }
    // 只剥掉结尾的 /v1（不能用 split("/v1")：域名本身可能以 v1 开头）
    let origin = raw.strip_suffix("/v1").unwrap_or(raw).trim_end_matches('/');

    let mut bases: Vec<String> = vec![];
    for b in [raw.to_string(), origin.to_string(), format!("{}/v1", origin)] {
        if !bases.contains(&b) {
            bases.push(b);
        }
    }

    let mut last_err = String::from("未知错误");
    for base in &bases {
        let sub_url = format!("{}/dashboard/billing/subscription", base);
        let sub = match http_get(&sub_url, &k.api_key, proxy).and_then(parse_json) {
            Ok(v) => v,
            Err(e) => {
                last_err = e;
                continue;
            }
        };
        let limit = num(&sub, "hard_limit_usd").or_else(|| num(&sub, "system_hard_limit_usd"));
        let usage_url = format!("{}/dashboard/billing/usage", base);
        let used = http_get(&usage_url, &k.api_key, proxy)
            .and_then(parse_json)
            .ok()
            .and_then(|v| num(&v, "total_usage"))
            .map(|cents| cents / 100.0);
        let mut lines = Vec::new();
        match (limit, used) {
            (Some(l), Some(u)) => {
                let remain = l - u;
                if let Some(x) = line("余额", Some(remain), "USD", true) {
                    lines.push(x);
                }
                if let Some(x) = line("已用", Some(u), "USD", false) {
                    lines.push(x);
                }
                if let Some(x) = line("额度上限", Some(l), "USD", false) {
                    lines.push(x);
                }
                return Ok((lines, remain > 0.0));
            }
            (Some(l), None) => {
                if let Some(x) = line("额度上限", Some(l), "USD", true) {
                    lines.push(x);
                }
                return Ok((lines, l > 0.0));
            }
            (None, _) => {
                last_err = "站点返回的不是 OpenAI 计费结构".to_string();
            }
        }
    }

    for base in &bases {
        let url = format!("{}/api/user/self", base);
        if let Ok(v) = http_get(&url, &k.api_key, proxy).and_then(parse_json) {
            if let Some(q) = num(&v, "data.quota") {
                let mut lines = Vec::new();
                if let Some(x) = line("余额", Some(q / 500_000.0), "USD", true) {
                    lines.push(x);
                }
                if let Some(x) = line(
                    "已用",
                    num(&v, "data.used_quota").map(|n| n / 500_000.0),
                    "USD",
                    false,
                ) {
                    lines.push(x);
                }
                return Ok((lines, q > 0.0));
            }
        }
    }
    Err(last_err)
}

// ---------------- 对外入口 ----------------

fn fetch(key: &ProviderKey, proxy: &str) -> BalanceResult {
    match key.provider.as_str() {
        "" | "deepseek" => deepseek(key, proxy),
        "kimi" => kimi(key, proxy, "https://api.moonshot.cn"),
        "kimi_intl" => kimi(key, proxy, "https://api.moonshot.ai"),
        "zhipu" => zhipu(key, proxy),
        "stepfun" => stepfun(key, proxy),
        "novita" => novita(key, proxy),
        "openrouter" => openrouter(key, proxy),
        "ai302" => ai302(key, proxy),
        "aihubmix" => aihubmix(key, proxy),
        "custom" => custom(key, proxy),
        other => Err(format!("未知厂商：{}", other)),
    }
}

fn fetch_one(key: &ProviderKey, proxy: &str) -> AccountBalance {
    let base = AccountBalance {
        id: key.id.clone(),
        provider: if key.provider.is_empty() {
            "deepseek".to_string()
        } else {
            key.provider.clone()
        },
        label: key.label.clone(),
        ok: false,
        is_available: false,
        balances: Vec::new(),
        error: None,
    };
    match fetch(key, proxy) {
        Ok((balances, available)) => AccountBalance {
            ok: true,
            is_available: available,
            balances,
            ..base
        },
        Err(e) => AccountBalance {
            error: Some(e),
            ..base
        },
    }
}

/// 并行查询所有 Key 的余额（Key 数量通常很少，线程开销可忽略）。
pub fn fetch_all(keys: &[ProviderKey], proxy: &str) -> Vec<AccountBalance> {
    std::thread::scope(|s| {
        let handles: Vec<_> = keys
            .iter()
            .map(|k| s.spawn(move || fetch_one(k, proxy)))
            .collect();
        handles
            .into_iter()
            .filter_map(|h| h.join().ok())
            .collect()
    })
}
