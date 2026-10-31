#!/usr/bin/env bash
# 清理 SwitchKit 残留进程 / 注册项，用于「覆盖安装提示正在运行」时兜底。
#
# 用法：
#   ./scripts/clean-switchkit.sh           # 只清理进程与端口
#   ./scripts/clean-switchkit.sh --all     # 同时卸载开机自启 LaunchAgent
set -uo pipefail

echo "== 1/3 结束 SwitchKit 主进程与守护进程 =="
pids=$(pgrep -f -i "SwitchKit|unoswitchkit|switchkit-daemon" || true)
if [ -n "$pids" ]; then
  echo "  找到进程: $(echo "$pids" | tr '\n' ' ')"
  echo "$pids" | xargs kill -9 2>/dev/null || true
  sleep 1
else
  echo "  无残留进程"
fi

echo "== 2/3 释放 9876 端口 =="
port_pids=$(lsof -nP -tiTCP:9876 -sTCP:LISTEN 2>/dev/null || true)
if [ -n "$port_pids" ]; then
  echo "$port_pids" | xargs kill -9 2>/dev/null || true
  echo "  已清理: $(echo "$port_pids" | tr '\n' ' ')"
else
  echo "  端口空闲"
fi

if [ "${1:-}" = "--all" ]; then
  echo "== 3/3 卸载开机自启 =="
  plist="$HOME/Library/LaunchAgents/com.switchkit.app.plist"
  if [ -f "$plist" ]; then
    launchctl unload "$plist" 2>/dev/null || true
    rm -f "$plist"
    echo "  已移除 $plist"
  else
    echo "  未注册开机自启"
  fi
else
  echo "== 3/3 跳过开机自启（加 --all 一并卸载）=="
fi

echo "== 结果 =="
if pgrep -f -i "SwitchKit|unoswitchkit|switchkit-daemon" >/dev/null 2>&1; then
  echo "❌ 仍有进程存活："
  pgrep -f -i "SwitchKit|unoswitchkit|switchkit-daemon" | while read -r p; do
    ps -p "$p" -o pid,ppid,command | tail -1
  done
else
  echo "✅ 已清空，可以覆盖替换 .app"
fi
