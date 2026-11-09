use std::time::{SystemTime, UNIX_EPOCH};

use sysinfo::System;

use crate::models::SystemStats;

/// 系统资源实时监控状态。
///
/// CPU / 内存走 sysinfo（持有常驻 System 实例，CPU 使用率依赖连续两次刷新计算），
/// 网络上下行走原生 `netstat -ib`（macOS BSD 工具，输出稳定），同样缓存上一次
/// 字节数与时间戳用于折算速率。sysinfo 各版本的网络 API 不稳定（0.30/0.33 无、
/// 0.34 改为独立 `Networks` 结构），故网络单独用系统命令实现，避免版本漂移。
pub struct SystemMonitor {
    sys: System,
    last_rx: u64,
    last_tx: u64,
    last_ts: u64,
}

impl SystemMonitor {
    pub fn new() -> Self {
        SystemMonitor {
            sys: System::new_all(),
            last_rx: 0,
            last_tx: 0,
            last_ts: 0,
        }
    }

    /// 采样一次：返回当前 CPU%、内存占用与网络上下行速率。
    /// 网络速率为「距上次采样」的区间均值；首帧无历史则记为 0。
    pub fn sample(&mut self) -> SystemStats {
        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();

        // CPU：所有核心使用率取平均（0..100）
        let cpus = self.sys.cpus();
        let cpu = if cpus.is_empty() {
            0.0
        } else {
            cpus.iter().map(|c| c.cpu_usage() as f64).sum::<f64>() / cpus.len() as f64
        };

        let mem_total = self.sys.total_memory();
        let mem_used = self.sys.used_memory();

        // 网络：原生 netstat 累加所有非回环接口的 Link 聚合行（开机累计字节数），
        // 与上次采样做区间差分得到上下行速率。
        let (rx, tx) = sample_net_bytes();

        let now = now_secs();
        let dt = if self.last_ts == 0 {
            0
        } else {
            now.saturating_sub(self.last_ts)
        };
        let (up_bps, down_bps) = if dt > 0 {
            (
                ((tx.saturating_sub(self.last_tx)) as f64 / dt as f64) as u64,
                ((rx.saturating_sub(self.last_rx)) as f64 / dt as f64) as u64,
            )
        } else {
            (0, 0)
        };
        self.last_rx = rx;
        self.last_tx = tx;
        self.last_ts = now;

        SystemStats {
            cpu_percent: cpu as f32,
            mem_used_bytes: mem_used,
            mem_total_bytes: mem_total,
            net_up_bps: up_bps,
            net_down_bps: down_bps,
        }
    }
}

/// 读取所有非回环接口的累计收/发字节数（开机至今）。
/// 仅取 `<Link#n>` 聚合行（每个接口唯一、且为总量），避免按地址重复计数；
/// 接口名以 `lo` 开头的一律跳过。
///
/// 注意 netstat 输出列数随行型变化：`<Link>` 聚合行 Address 为空，整行只有 10 列；
/// IP 行有地址、11 列。因此 Ibytes/Obytes 不用固定列号，而是从行尾锚定
/// （末尾固定为 `... Ipkts Ierrs Ibytes Opkts Oerrs Obytes Coll`，
/// 故 Ibytes = 倒数第 5 列、Obytes = 倒数第 2 列，两种行型都适用）。
fn sample_net_bytes() -> (u64, u64) {
    // 用绝对路径：GUI 启动时 PATH 可能不含 /usr/sbin。
    let out = crate::util::run("/usr/sbin/netstat", &["-ib", "-n"]).unwrap_or_default();
    let mut rx = 0u64;
    let mut tx = 0u64;
    for line in out.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 7 {
            continue;
        }
        if f[0].starts_with("lo") {
            continue;
        }
        if !f[2].starts_with("<Link") {
            continue;
        }
        let n = f.len();
        rx += f[n - 5].parse::<u64>().unwrap_or(0);
        tx += f[n - 2].parse::<u64>().unwrap_or(0);
    }
    (rx, tx)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_produces_sane_stats() {
        let mut m = SystemMonitor::new();
        let s = m.sample();
        // 内存总量应 > 0；CPU 应在 [0,100]
        assert!(s.mem_total_bytes > 0, "mem_total_bytes 应 > 0");
        assert!(
            s.cpu_percent >= 0.0 && s.cpu_percent <= 100.0,
            "cpu_percent 应在 [0,100]，实际 {}",
            s.cpu_percent
        );
        // 第二帧：网络速率计算不应 panic，且为有限值
        let s2 = m.sample();
        assert!(s2.net_up_bps == s2.net_up_bps);
        assert!(s2.net_down_bps == s2.net_down_bps);
    }

    #[test]
    fn net_bytes_parse_sum_is_positive() {
        // 本机实测 netstat 应能解析出 > 0 的收/发累计字节（开机以来必有流量）
        let (rx, tx) = sample_net_bytes();
        assert!(rx > 0, "rx 累计字节应 > 0");
        assert!(tx > 0, "tx 累计字节应 > 0");
    }
}
