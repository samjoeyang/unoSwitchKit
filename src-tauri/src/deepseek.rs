//! DeepSeek 余额查询。
//!
//! 接口：GET https://api.deepseek.com/user/balance
//! 认证：Authorization: Bearer <API_KEY>
//! 响应：{ is_available, balance_infos: [{ currency, total_balance,
//!         granted_balance, topped_up_balance }] }
//!
//! 采用系统自带 curl 发请求，避免引入 reqwest 等重型依赖（编译体积/时间）。

use std::process::Command;

use serde::Deserialize;

use crate::models::{DeepSeekAccountBalance, DeepSeekBalanceInfo, DeepSeekKey};

#[derive(Deserialize)]
struct BalanceResp {
    #[serde(default)]
    is_available: bool,
    #[serde(default)]
    balance_infos: Vec<DeepSeekBalanceInfo>,
}

/// 查询单个 Key 的余额；失败时把原因放进 `error` 返回（不抛错，便于多 Key 并行展示）。
fn fetch_one(key: &DeepSeekKey) -> DeepSeekAccountBalance {
    let fail = |msg: String| DeepSeekAccountBalance {
        id: key.id.clone(),
        label: key.label.clone(),
        ok: false,
        is_available: false,
        balances: Vec::new(),
        error: Some(msg),
    };

    let auth = format!("Authorization: Bearer {}", key.api_key.trim());
    let output = Command::new("curl")
        .arg("-sS")
        .arg("-m")
        .arg("15")
        .arg("-H")
        .arg(&auth)
        .arg("-w")
        .arg("\\n%{http_code}")
        .arg("https://api.deepseek.com/user/balance")
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => return fail(format!("无法执行 curl: {}", e)),
    };

    let raw = String::from_utf8_lossy(&output.stdout).to_string();
    // curl -w "\n%{http_code}" → 最后一行是 HTTP 状态码
    let (body, code) = match raw.rsplit_once('\n') {
        Some((b, c)) => (b.to_string(), c.trim().to_string()),
        None => (raw.clone(), String::new()),
    };
    let code_disp = if code.is_empty() { "?" } else { code.as_str() };

    if code != "200" {
        let msg = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| {
                v.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| {
                if body.trim().is_empty() {
                    "无响应（检查网络或 Key 是否正确）".to_string()
                } else {
                    body.chars().take(200).collect()
                }
            });
        return fail(format!("HTTP {}：{}", code_disp, msg));
    }

    match serde_json::from_str::<BalanceResp>(&body) {
        Ok(r) => DeepSeekAccountBalance {
            id: key.id.clone(),
            label: key.label.clone(),
            ok: true,
            is_available: r.is_available,
            balances: r.balance_infos,
            error: None,
        },
        Err(e) => fail(format!("响应解析失败: {}", e)),
    }
}

/// 批量查询所有 Key 的余额（串行；Key 数量通常很少）。
pub fn fetch_all(keys: &[DeepSeekKey]) -> Vec<DeepSeekAccountBalance> {
    keys.iter().map(fetch_one).collect()
}
