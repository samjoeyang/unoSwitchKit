#!/bin/bash
# 只读诊断：定位「原生守护不亮」属于哪一类根因，不改任何文件、不重启进程。
# 用法：在出问题的机器上执行  ./scripts/diagnose-daemon.sh [可选 .app 路径]
# 输出同时打印到终端，并落盘到 ~/Desktop/switchkit-daemon-diagnosis.txt 供回传。
#
# 背景：历史上四次「守护不亮」表象相同、根因分别在分发隔离 / 状态探测 /
# 端口占用 / 启动时序四层。本脚本按两份生命周期日志 + 签名/架构/端口实况
# 自动分类，避免第五次继续套用旧结论。

set -u
APP="${1:-/Applications/SwitchKit.app}"
OUT="${DIAG_OUT:-$HOME/Desktop/switchkit-daemon-diagnosis.txt}"
RUST_LOG="$HOME/Library/Logs/switchkit-rust.log"
DAEMON_LOG="$HOME/Library/Logs/switchkit-daemon.log"
DIAG_LOG="$HOME/Library/Logs/switchkit-daemon-startup.log"
MAIN="$APP/Contents/MacOS/unoswitchkit"
DAEMON="$APP/Contents/MacOS/switchkit-daemon"

mkdir -p "$HOME/Desktop" 2>/dev/null || true
exec > >(tee "$OUT") 2>&1

line() { printf '%s\n' "------------------------------------------------------------"; }
show() { line; printf '## %s\n' "$1"; line; }

show "0. 环境"
/usr/bin/sw_vers
printf 'uname -m        : %s\n' "$(uname -m)"
printf 'proc_translated : %s (1 = Rosetta 中运行)\n' "$(/usr/sbin/sysctl -n sysctl.proc_translated 2>/dev/null || echo n/a)"
printf 'arm64 支持      : %s\n' "$(/usr/sbin/sysctl -n hw.optional.arm64 2>/dev/null || echo 'n/a (Intel)')"
printf '报告时间        : %s\n' "$(/bin/date '+%Y-%m-%d %H:%M:%S %z')"
printf 'App 路径        : %s\n' "$APP"
# macOS 27 仅支持 Apple Silicon；若此处显示 27 + x86_64 则为异常组合。
if [ -d "$APP" ]; then
  /usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$APP/Contents/Info.plist" 2>/dev/null \
    | /usr/bin/sed 's/^/最低系统版本    : /' || true
fi
printf '\n-- MDM 受管状态（macOS 27 原生二进制白名单只对受管机生效）--\n'
/usr/bin/profiles status -type enrollment 2>&1 | /usr/bin/head -12 || true
printf '说明：ad-hoc 签名没有 Team ID，受管机的允许列表只能按 CDHash 放行；\n'
printf '      若该机受管且启用了 AllowedBinaries，内嵌守护会被单独拦截，主 App 却可能正常。\n'

show "1. App / 守护二进制：架构"
if [ -d "$APP" ]; then
  printf '主程序 : %s\n' "$(/usr/bin/lipo -archs "$MAIN" 2>&1 || echo '读取失败')"
  printf '守护   : %s\n' "$(/usr/bin/lipo -archs "$DAEMON" 2>&1 || echo '读取失败')"
  /usr/bin/file "$DAEMON" 2>&1 || true
else
  printf '❌ 找不到 %s\n' "$APP"
fi

show "2. 代码签名（ad-hoc 换包后 cdhash 变化，系统策略缓存会失效）"
for p in "$APP" "$MAIN" "$DAEMON"; do
  [ -e "$p" ] || continue
  printf '\n-- %s --\n' "$p"
  /usr/bin/codesign -dv --verbose=2 "$p" 2>&1 | /usr/bin/grep -E 'Identifier=|Signature=|CDHash=|TeamIdentifier=|code object is not signed' || printf '(无签名信息)\n'
done
printf '\n-- 整体校验 --\n'
/usr/bin/codesign --verify --deep --strict "$APP" 2>&1 || true
printf '\n-- 蓝牙 TCC 声明（macOS 14+ IOBluetooth 权限；缺失会让守护被 tccd 拦）--\n'
if [ -f "$APP/Contents/Info.plist" ]; then
  /usr/libexec/PlistBuddy -c 'Print :NSBluetoothAlwaysUsageDescription' "$APP/Contents/Info.plist" 2>&1 \
    || printf '❌ 缺少 NSBluetoothAlwaysUsageDescription\n'
else
  printf '(无 Info.plist)\n'
fi

show "3. 下载隔离属性（com.apple.quarantine）"
for p in "$APP" "$DAEMON"; do
  [ -e "$p" ] || continue
  printf '\n-- %s --\n' "$p"
  /usr/bin/xattr -l "$p" 2>&1 | /usr/bin/grep -i 'quarantine' || printf '(无 quarantine)\n'
done
printf '\n-- Gatekeeper 评估 --\n'
/usr/sbin/spctl -a -vvv "$APP" 2>&1 || true

show "4. 端口 9876 与进程"
/usr/sbin/lsof -nP -iTCP:9876 -sTCP:LISTEN 2>/dev/null || printf '(9876 无监听者)\n'
printf '\n-- 相关进程 --\n'
/bin/ps -axo pid,ppid,arch,comm 2>/dev/null | /usr/bin/grep -iE 'switchkit|unoswitchkit' | /usr/bin/grep -v grep || printf '(无)\n'

show "5. 主程序生命周期日志（最近 60 行）"
if [ -f "$RUST_LOG" ]; then
  /usr/bin/tail -60 "$RUST_LOG"
else
  printf '❌ 无 %s —— App 可能从未运行到 setup 阶段\n' "$RUST_LOG"
fi

show "5.5 守护启动诊断日志（结构化；单次启动的完整决策链）"
if [ -f "$DIAG_LOG" ]; then
  printf '文件大小 : %s\n' "$(/usr/bin/du -h "$DIAG_LOG" | /usr/bin/awk '{print $1}')"
  printf '\n-- 最近一次会话的结论行 --\n'
  /usr/bin/grep 'event=conclusion' "$DIAG_LOG" 2>/dev/null | /usr/bin/tail -5 || printf '(尚无结论行)\n'
  printf '\n-- 最近 80 行 --\n'
  /usr/bin/tail -80 "$DIAG_LOG"
else
  printf '❌ 无 %s\n' "$DIAG_LOG"
  printf '   当前安装版本尚未包含结构化诊断日志；需发版后复现才会生成。\n'
fi

show "6. 守护日志（最近 40 行；完整文件可能很大）"
if [ -f "$DAEMON_LOG" ]; then
  printf '文件大小 : %s\n' "$(/usr/bin/du -h "$DAEMON_LOG" | /usr/bin/awk '{print $1}')"
  /usr/bin/tail -40 "$DAEMON_LOG"
else
  printf '❌ 无 %s —— 守护进程从未写第一行日志（exec 之前就被拦截，或 stderr 重定向失败）\n' "$DAEMON_LOG"
fi

show "7. 系统策略日志（最近 2 小时；AMFI / Gatekeeper / TCC / taskgated）"
# taskgated-helper 是 macOS 27 上受限 entitlement 被拒的实际输出者
# （见 Tart 2.33.0 在 27/arm64 上 SIGKILL 的案例），旧版只查 amfid 会漏。
/usr/bin/log show --last 2h --style compact \
  --predicate 'process == "amfid" || process == "syspolicyd" || process == "taskgated-helper" || process == "tccd" || process == "kernel"' 2>/dev/null \
  | /usr/bin/grep -iE 'switchkit|daemon|AMFI|deny|reject|entitlement|quarantine' \
  | /usr/bin/tail -60 || printf '(无匹配，或当前账户无权读取统一日志)\n'

show "7.5 慢启动证据（macOS 27 签名评估延迟是自愈反复的高发诱因）"
if [ -f "$RUST_LOG" ]; then
  printf 'still initializing 次数 : %s\n' "$(/usr/bin/grep -c 'still initializing' "$RUST_LOG" 2>/dev/null || printf 0)"
  printf 'STILL ALIVE 次数        : %s\n' "$(/usr/bin/grep -c 'STILL ALIVE' "$RUST_LOG" 2>/dev/null || printf 0)"
  printf 'respawning 次数         : %s\n' "$(/usr/bin/grep -c 'respawning' "$RUST_LOG" 2>/dev/null || printf 0)"
  printf 'kill_port_holder 次数   : %s\n' "$(/usr/bin/grep -c 'killed pids' "$RUST_LOG" 2>/dev/null || printf 0)"
  printf '\n-- 最近一次 spawn 时间线 --\n'
  /usr/bin/grep -E 'spawned pid|ping OK|still initializing|NOT responding|STILL ALIVE|exited early|respawning|ping OK on entry' "$RUST_LOG" 2>/dev/null | /usr/bin/tail -30 || true
else
  printf '(无主程序日志)\n'
fi

show "8. 自动分类"
RUST_TAIL=""
[ -f "$RUST_LOG" ] && RUST_TAIL="$(/usr/bin/tail -80 "$RUST_LOG")"
DAEMON_TAIL=""
[ -f "$DAEMON_LOG" ] && DAEMON_TAIL="$(/usr/bin/tail -200 "$DAEMON_LOG")"
DIAG_TAIL=""
[ -f "$DIAG_LOG" ] && DIAG_TAIL="$(/usr/bin/tail -200 "$DIAG_LOG")"
PORT_UP=0
/usr/sbin/lsof -nP -iTCP:9876 -sTCP:LISTEN >/dev/null 2>&1 && PORT_UP=1

if [ "$PORT_UP" = "1" ]; then
  printf '【A】9876 正在监听 → 守护本身活着。\n'
  printf '     若面板仍灰，属于状态探测层：检查 sidecar_status 的 ping 是否被超时/蓝牙拖住，或前端轮询是否停了。\n'
  printf '     对应历史根因：0.4.5（bluetooth:status 挂起误判）。不要再去修签名/隔离。\n'
elif printf '%s' "$RUST_TAIL" | /usr/bin/grep -q 'candidate MISSING'; then
  printf '【B】包内找不到守护二进制 → externalBin 未生效 / 包被破坏 / 路径不对。\n'
  printf '     对应历史根因：0.4.0（externalBin 漏配）。检查 .app/Contents/MacOS/ 内容。\n'
elif printf '%s' "$RUST_TAIL" | /usr/bin/grep -q 'spawn FAILED'; then
  printf '【C】exec/spawn 直接失败 → 架构不匹配（x86_64 包跑在 Silicon 且无 Rosetta）、\n'
  printf '     文件不可执行、或 AMFI 在 exec 瞬间拒绝。看上面第 1/2/7 节与 spawn FAILED 后的 errno。\n'
  printf '     macOS 27 受管机注意：原生 AllowedBinaries 按二进制逐个匹配，ad-hoc 无 Team ID，\n'
  printf '     只能按 CDHash 放行；换包后 CDHash 变化即被拦，需 MDM 侧更新允许列表。\n'
elif printf '%s' "$RUST_TAIL" | /usr/bin/grep -qE 'NOT responding after|still initializing'; then
  printf '【D2】守护进程已拉起但签名/Gatekeeper 评估超过 30s（macOS 27 高发）。\n'
  printf '     现版本不再 kill 慢启动进程：日志应有 "STILL ALIVE ... keep it" + 看门狗宽限。\n'
  printf '     若最终仍不亮，查第 7 节 amfid/taskgated-helper 的具体拒绝原因。\n'
elif printf '%s' "$RUST_TAIL" | /usr/bin/grep -q 'NOT up after'; then
  if printf '%s' "$RUST_TAIL" | /usr/bin/grep -q 'alive=true'; then
    printf '【D】进程活着但就绪窗口内没 listen → 卡在 Swift 第一行之前：\n'
    printf '     代码签名/Gatekeeper 首次评估、AMFI 校验、或换包后系统策略缓存失效。\n'
    printf '     对应历史根因：0.4.12（启动竞态 + OTA 包未签名）。看门狗应会自动重试；若一直失败，看第 7 节。\n'
  else
    printf '【E】进程立刻退出 → 看第 6 节守护日志的 FATAL / bind failed / parent exited。\n'
    printf '     常见：9876 被别的进程占用、父进程 PID 已退出触发自杀、蓝牙框架加载失败。\n'
  fi
elif [ -z "$RUST_TAIL" ]; then
  printf '【F】主程序日志为空 → App 根本没走到 spawn_sidecar，或日志目录不可写。\n'
  printf '     先确认 App 是否真的启动、~/Library/Logs 是否可写。\n'
elif printf '%s' "$DAEMON_TAIL" | /usr/bin/grep -q 'listening on 127.0.0.1:9876'; then
  printf '【G】守护曾成功 listen，但当前端口无监听 → 守护随后退出了。\n'
  printf '     查守护日志里的 "parent ... exited, shutting down"：OTA relaunch 时旧父进程退出会带走了守护。\n'
else
  printf '【H】未匹配到已知模式 → 把本报告整份回传，重点看第 5/6/7 节的时间线对齐。\n'
fi

if [ -n "$DIAG_TAIL" ] && printf '%s' "$DIAG_TAIL" | /usr/bin/grep -q 'event=conclusion'; then
  printf '\n-- 结构化日志里的最近结论（优先以此为准）--\n'
  printf '%s\n' "$DIAG_TAIL" | /usr/bin/grep 'event=conclusion' | /usr/bin/tail -5
  printf 'cause 取值：none / slow_start / port_busy / spawn_or_exec_failed / binary_missing\n'
fi

printf '\n'
show "9. 换包后必须做的两步（ad-hoc 签名固有行为）"
printf '① 每次新包 / OTA 落地后，到「系统设置 → 隐私与安全性」重新点「仍要打开」。\n'
printf '   Gatekeeper 豁免按 cdhash 记录，换包即失效；OTA relaunch 往往不会主动弹提示。\n'
printf '② 完全退出 App 后执行：tccutil reset All com.switchkit.app\n'
printf '   再重开 App，逐项触发对应功能让授权弹窗重新出现。\n'
printf '   （部分系统级服务需 sudo 或重启 macOS；若 reset 报 code object is not signed at all，\n'
printf '     先给报错路径的二进制补 ad-hoc 签名：codesign --force --deep --sign - <.app>）\n'

line
printf '报告已保存：%s\n' "$OUT"
printf '本脚本只读，未修改、未删除、未重启任何进程。\n'
