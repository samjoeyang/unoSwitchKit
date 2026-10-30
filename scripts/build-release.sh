#!/usr/bin/env bash
# SwitchKit 发布构建脚本
#
# 用法：
#   ./scripts/build-release.sh              # 当前架构
#   ./scripts/build-release.sh --universal  # Apple Silicon + Intel 通用包
#
# 签名策略（自动检测，也可用 APPLE_SIGNING_IDENTITY 显式指定）：
#   1) 钥匙串里存在「Developer ID Application」证书 → 用它签名；
#      若同时配置了公证凭据（APPLE_CERTIFICATE 或 APPLE_ID+APPLE_PASSWORD+APPLE_TEAM_ID），
#      Tauri 会自动完成 notarization + staple。
#   2) 无证书 → 降级为 ad-hoc 签名（codesign -s -）：包行为一致、
#      守护进程不会被系统拦截，但 Gatekeeper 会弹「未验证开发者」，
#      用户需「右键 → 打开」（macOS 13+ 为 系统设置 → 隐私与安全性 → 仍要打开）。
#
# 产物：src-tauri/target/<target>/release/bundle/{macos,dmg}/
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$(pwd)"

# cargo 通常装在 ~/.cargo/bin（见安装器写入 ~/.zshrc 的 source 行）
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
command -v cargo >/dev/null 2>&1 || { echo "❌ 找不到 cargo，请先安装 Rust：https://rustup.rs"; exit 1; }

MODE="current"
[ "${1:-}" = "--universal" ] && MODE="universal"

ARCH="$(uname -m)"
if [ "$MODE" = "universal" ]; then
  TARGET="universal-apple-darwin"
  SWIFT_TARGETS="x86_64-apple-darwin arm64-apple-darwin"
else
  TARGET="${ARCH}-apple-darwin"
  SWIFT_TARGETS="$TARGET"
fi

echo "== [1/4] 编译 Swift 守护进程 ($MODE) =="
DAEMON_BIN="src-tauri/binaries/switchkit-daemon-$TARGET"
if [ "$MODE" = "universal" ]; then
  # Tauri 的 universal 构建会按架构分别打包 externalBin，因此必须提供
  # 每个架构的独立二进制（aarch64 / x86_64）；再 lipo 合并出 universal 供兼容。
  rm -f src-tauri/binaries/switchkit-daemon-{aarch64,x86_64,universal}-apple-darwin
  # arm64 切片：交叉编译需显式部署目标（Apple Silicon 最低 11.0）
  (cd src-tauri/swift && swiftc -O -target arm64-apple-macosx11.0 \
    -framework AppKit -framework ApplicationServices \
    -framework Network -framework IOBluetooth \
    main.swift bridge.swift menubar.swift bluetooth.swift \
    -o "../binaries/switchkit-daemon-aarch64-apple-darwin")
  # x86_64 切片：沿用主机默认 target（macOS 12），不传 -target
  (cd src-tauri/swift && swiftc -O \
    -framework AppKit -framework ApplicationServices \
    -framework Network -framework IOBluetooth \
    main.swift bridge.swift menubar.swift bluetooth.swift \
    -o "../binaries/switchkit-daemon-x86_64-apple-darwin")
  lipo -create src-tauri/binaries/switchkit-daemon-aarch64-apple-darwin \
    src-tauri/binaries/switchkit-daemon-x86_64-apple-darwin \
    -output "$DAEMON_BIN"
else
  (cd src-tauri/swift && swiftc -O \
    -framework AppKit -framework ApplicationServices \
    -framework Network -framework IOBluetooth \
    main.swift bridge.swift menubar.swift bluetooth.swift \
    -o "../binaries/switchkit-daemon-$TARGET")
fi

# ad-hoc 签名守护进程：Apple Silicon 上无签名二进制直接被杀；Intel 上
# 未签名进程在签名过的 App 内也可能被拒。有真证书时 Tauri 稍后会重签。
codesign --force --sign - "$DAEMON_BIN"
echo "   daemon: $DAEMON_BIN ($(lipo -info "$DAEMON_BIN" 2>/dev/null || file "$DAEMON_BIN"))"

echo "== [2/4] 检测签名身份 =="
IDENTITY="${APPLE_SIGNING_IDENTITY:-}"
if [ -z "$IDENTITY" ]; then
  IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
    | grep 'Developer ID Application' | head -1 \
    | sed -E 's/^[[:space:]]*[0-9A-Fa-f]+[[:space:]]+"(.*)"/\1/' || true)"
fi

# 关键：无证书时必须 unset 而不是设为空串——Tauri 见到该变量（即使为空）
# 就会进入签名流程，用 "" 当 identity 去 codesign 必然失败。
if [ -n "$IDENTITY" ]; then
  echo "   找到 Developer ID 证书：$IDENTITY（将正式签名）"
  export APPLE_SIGNING_IDENTITY="$IDENTITY"
  if [ -z "${APPLE_CERTIFICATE:-}" ] && [ -z "${APPLE_ID:-}" ]; then
    echo "   ⚠️ 未配置公证凭据（APPLE_CERTIFICATE 或 APPLE_ID/APPLE_PASSWORD/APPLE_TEAM_ID）"
    echo "      本次只签名不公证。公证配置见 docs/构建指南.md §6。"
  fi
else
  unset APPLE_SIGNING_IDENTITY 2>/dev/null || true
  echo "   未找到 Developer ID 证书 → 降级为 ad-hoc 签名（内测分发）"
fi

echo "== [3/4] tauri build ($TARGET) =="
# 在线更新：若构建期导出了 SWITCHKIT_UPDATE_ENDPOINTS，则把 manifest URL 写进
# tauri.conf.json 的 plugins.updater.endpoints（option_env! 同时固化进二进制 enabled），
# 构建后还原该文件，避免把线上地址提交进仓库。
CONF="src-tauri/tauri.conf.json"
UPDATE_BACKUP=""
if [ -n "${SWITCHKIT_UPDATE_ENDPOINTS:-}" ]; then
  UPDATE_BACKUP="$(mktemp)"
  cp "$CONF" "$UPDATE_BACKUP"
  python3 - "$CONF" "${SWITCHKIT_UPDATE_ENDPOINTS}" <<'PY'
import json, sys
p, eps = sys.argv[1], sys.argv[2]
d = json.load(open(p))
d.setdefault("plugins", {}).setdefault("updater", {})["endpoints"] = \
    [e.strip() for e in eps.split(",") if e.strip()]
json.dump(d, open(p, "w"), indent=2, ensure_ascii=False)
PY
  export SWITCHKIT_UPDATE_ENDPOINTS
  echo "   更新服务已启用：endpoints=$SWITCHKIT_UPDATE_ENDPOINTS"
fi

# 产物目录：universal 用 target triple，当前架构直接是 target/release
if [ "$MODE" = "universal" ]; then
  BUNDLE_DIR="src-tauri/target/$TARGET/release/bundle"
else
  BUNDLE_DIR="src-tauri/target/release/bundle"
fi
# 仅构建 .app（跳过 DMG：DMG 需 hdiutil 挂载卷 + 终端「控制 Finder」自动化授权，
# 在 CI/沙箱/无授权环境下会失败；分发用 zip 已足够，后续要出 .dmg 时再开启）。
DMG_OK=0
BUILD_ARGS=()
[ "$MODE" = "universal" ] && BUILD_ARGS+=(--target "$TARGET")
BUILD_ARGS+=(--bundles app)
npm run tauri build -- "${BUILD_ARGS[@]+"${BUILD_ARGS[@]}"}"

# 还原 tauri.conf.json（endpoints 仅用于本次构建）
if [ -n "$UPDATE_BACKUP" ]; then
  cp "$UPDATE_BACKUP" "$CONF"; rm -f "$UPDATE_BACKUP"
  echo "   已还原 tauri.conf.json（更新 endpoints 不入库）"
fi

APP="$BUNDLE_DIR/macos/SwitchKit.app"

echo "== [4/4] 签名收尾与验证 =="
if [ -z "${APPLE_SIGNING_IDENTITY:-}" ]; then
  # Tauri 无身份时可能完全跳过签名；统一 ad-hoc 深签（含嵌套守护进程）
  codesign --force --deep --sign - "$APP"
  echo "   已 ad-hoc 签名：$APP"
  echo ""
  echo "   ⚠️ 此包未经 Apple 公证，分发时请附说明："
  echo "      首次打开：右键 SwitchKit.app → 打开 → 再点「打开」"
  echo "      （macOS 13+：系统设置 → 隐私与安全性 → 仍要打开）"
else
  echo "   正式签名完成，验证结果："
  codesign --verify --deep --strict "$APP" && echo "   codesign verify: OK"
  if spctl -a -vv "$APP" 2>&1 | grep -q Notarized; then
    echo "   notarization: ✓ 已公证"
  else
    echo "   notarization: 未公证（如需公证，配置凭据后重跑）"
  fi
fi

# 守护进程确实进了包（externalBin 生效）
DAEMON_IN_APP="$APP/Contents/MacOS/switchkit-daemon"
[ -x "$DAEMON_IN_APP" ] || { echo "❌ 守护进程未打包进 .app！"; exit 1; }
codesign -dv "$DAEMON_IN_APP" 2>&1 | head -2 || true

# DMG 不可用时提供 zip 分发包（签名后压缩，保留签名）
# 注意：必须用 `zip -r -y`，不能用 `ditto -c -k`——ditto 会把源目录的「内容」
# 直接打进压缩包根，丢弃 .app 包裹层，导致解压后只剩 Contents/ 而非 SwitchKit.app。
# -y 保留可能的符号链接；从 macos 目录内用裸名 SwitchKit.app 打包以保持包裹层。
if [ "$DMG_OK" = 0 ]; then
  ZIP="$BUNDLE_DIR/macos/SwitchKit-adhoc.zip"
  rm -f "$ZIP"
  ( cd "$BUNDLE_DIR/macos" && zip -r -y "SwitchKit-adhoc.zip" "SwitchKit.app" )
  echo "   分发 zip：$ZIP（解压后为 SwitchKit.app，右键 → 打开）"
fi

echo ""
echo "✅ 完成。产物："
ls -d "$APP" "$BUNDLE_DIR"/dmg/*.dmg 2>/dev/null || true
[ "$DMG_OK" = 0 ] && ls -lh "$BUNDLE_DIR/macos/SwitchKit-adhoc.zip"
