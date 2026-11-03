#!/usr/bin/env bash
# 生成自签代码签名证书（免费分发路线用）
#
# 与 Developer ID 的核心区别：
#   自签证书不会被 Apple 信任，Gatekeeper 仍会弹「无法验证开发者」，
#   用户首次打开需「仍要打开」。但它给出【固定】的 designated requirement
#   （基于证书叶子哈希 + identifier），所以辅助功能/输入监控等权限在
#   「检查更新」就地升级后不会丢失——这正是 ad-hoc 签名做不到的。
#
# 与旧版的关键改动：证书直接导入【登录钥匙串】（本机已解锁），
# 不给 codesign 制造「专用钥匙串锁死 → 反复弹密码」的坑。
# 导入时通过 -T /usr/bin/codesign 把 codesign 加进私钥 ACL，
# 之后构建签名不再弹窗（若仍弹，点「始终允许」一次即可）。
#
# 一次性运行。证书私钥只留在本机登录钥匙串，切勿外传。
#
# 用法：
#   ./scripts/setup-selfsign.sh
#   SWITCHKIT_SELFSIGN_NAME="My Cert" ./scripts/setup-selfsign.sh
set -euo pipefail

CERT_NAME="${SWITCHKIT_SELFSIGN_NAME:-SwitchKit Self-Sign}"
LOGIN="$HOME/Library/Keychains/login.keychain-db"
VALID_DAYS="${SWITCHKIT_SELFSIGN_DAYS:-7300}"   # 默认 20 年
P12_PASS="${SWITCHKIT_SELFSIGN_P12_PASS:-switchkit}"  # 仅导入瞬间用，导入后即丢弃

# 幂等：已存在同名证书则跳过（注意不能用 find-identity -v，
# 因为自签证书不被系统信任会被 -v 过滤掉，导致重复建证）
if security find-certificate -c "$CERT_NAME" -a "$LOGIN" >/dev/null 2>&1; then
  echo "✅ 自签证书已存在：$CERT_NAME"
  echo "   构建前设置：export SWITCHKIT_SELFSIGN_NAME='$CERT_NAME'"
  exit 0
fi

echo "== 生成自签证书：$CERT_NAME =="
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cd "$WORK"

# 1) openssl 造一张带 codeSigning EKU 的自签证书（无 CSR、无 CA）
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout key.pem -out cert.pem -days "$VALID_DAYS" \
  -subj "/CN=$CERT_NAME/O=SwitchKit" \
  -addext "basicConstraints=critical,CA:FALSE" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=codeSigning"

# 2) 转 p12：macOS 的 security 不认 OpenSSL 3 默认 p12 的 MAC 算法，
#    必须 -legacy -macalg sha1 才能被 import 接受
openssl pkcs12 -export -legacy -macalg sha1 \
  -out cert.p12 -inkey key.pem -in cert.pem -passout "pass:$P12_PASS"

# 3) 导入登录钥匙串，并把 codesign / productsign 加入私钥 ACL
#    （首次导入会弹一次登录钥匙串密码——那是你 Mac 的登录密码，正常输入即可）
security import cert.p12 -k "$LOGIN" -P "$P12_PASS" \
  -T /usr/bin/codesign -T /usr/bin/productsign

# 4) 不加入系统信任：保留 Gatekeeper 告警（正是「首启仍要打开」的来源）
echo "✅ 已生成自签证书：$CERT_NAME"
echo "   位置：登录钥匙串（$LOGIN）"
echo "   构建前设置：export SWITCHKIT_SELFSIGN_NAME='$CERT_NAME'"
echo "   ⚠️ 该证书私钥仅留本机登录钥匙串，切勿提交或外传。"
echo "   ⚠️ 分发时用户首次打开仍会见到「无法验证开发者」，需引导「仍要打开」。"
