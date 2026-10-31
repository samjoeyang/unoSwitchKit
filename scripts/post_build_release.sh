#!/bin/bash
# 等 build.yml 跑完 -> 下载产物 -> 创建对应版本 Release 并上传三个 zip。
# 用途：仓库未配置 TAURI_SIGNING_PRIVATE_KEY 时，build.yml 不会自动发 Release，
#       官网下载按钮(releases/download/vX/...)会 404，故用本脚本补建 Release。
#
# 用法：
#   ./scripts/post_build_release.sh            # 自动取 build.yml 最近一次 run
#   ./scripts/post_build_release.sh <run_id>   # 指定某次 run
set -uo pipefail
REPO="samjoeyang/unoSwitchKit"
LOG=/tmp/post_build_release.log
WORK=/tmp/sk_release
rm -rf "$WORK"; mkdir -p "$WORK"
gettok() { printf 'protocol=https\nhost=github.com\n' | git -C /Users/samjoeyang/workspace/unoSwitchKit credential fill 2>/dev/null | awk -F= '/^password=/{print $2}'; }
api() { curl -s --max-time 30 -H "Authorization: Bearer $TOKEN" -H "Accept: application/vnd.github+json" "$@"; }
TOKEN=$(gettok); echo -n "$TOKEN" > /tmp/sk_tok.txt; chmod 600 /tmp/sk_tok.txt

# 解析本次要发布的版本（取 tauri.conf.json 的 version）
VERSION="$(python3 -c 'import json;print(json.load(open("/Users/samjoeyang/workspace/unoSwitchKit/src-tauri/tauri.conf.json"))["version"])' 2>/dev/null || echo 0.0.0)"
TAG="v$VERSION"
echo "[$(date)] target version=$VERSION tag=$TAG" | tee -a "$LOG"

# 决定要轮询的 run
if [ -n "${1:-}" ]; then
  RUN_ID="$1"
else
  RUN_ID=$(api "https://api.github.com/repos/$REPO/actions/workflows/build.yml/runs?per_page=1" | python3 -c "import json,sys;print(json.load(sys.stdin)['workflow_runs'][0]['id'])")
fi
echo "[$(date)] polling run $RUN_ID" | tee -a "$LOG"

S=""; C=""
for i in $(seq 1 180); do
  OUT=$(api "https://api.github.com/repos/$REPO/actions/runs/$RUN_ID")
  S=$(echo "$OUT" | python3 -c "import json,sys;d=json.load(sys.stdin);print(d['status'])")
  C=$(echo "$OUT" | python3 -c "import json,sys;d=json.load(sys.stdin);print(d.get('conclusion'))")
  echo "[$(date)] run status: $S $C" | tee -a "$LOG"
  [ "$S" = "completed" ] && break
  sleep 60
done
if [ "$S" != "completed" ] || [ "$C" != "success" ]; then
  echo "[$(date)] BUILD NOT SUCCESSFUL (status=$S conclusion=$C); abort." | tee -a "$LOG"; exit 1
fi
echo "[$(date)] build success; fetching artifacts" | tee -a "$LOG"

# 取产物下载地址
ART=$(api "https://api.github.com/repos/$REPO/actions/runs/$RUN_ID/artifacts" | python3 -c "
import json,sys
for a in json.load(sys.stdin).get('artifacts',[]):
    if a['name'].startswith('switchkit-macos-'):
        print(a['id'], a['name'], a['archive_download_url']); break
")
AURL=$(echo "$ART" | awk '{print $3}')
echo "[$(date)] artifact: $ART" | tee -a "$LOG"
curl -sL --max-time 180 -H "Authorization: Bearer $TOKEN" -o "$WORK/art.zip" "$AURL"
echo "[$(date)] artifact size: $(wc -c < "$WORK/art.zip")" | tee -a "$LOG"
( cd "$WORK" && unzip -o -q art.zip )
mapfile -t ZIPS < <(find "$WORK" -name "SwitchKit-$VERSION-*.zip" -type f)
echo "[$(date)] found zips:" | tee -a "$LOG"; printf '  %s\n' "${ZIPS[@]}" | tee -a "$LOG"
if [ "${#ZIPS[@]}" -lt 3 ]; then echo "[$(date)] ERROR: expected 3 zips, got ${#ZIPS[@]}" | tee -a "$LOG"; exit 1; fi

# 用 Python 创建 Release + 上传三个 zip（稳健处理 JSON/二进制）
python3 - "$REPO" "$WORK" "$TAG" "$VERSION" <<'PY' 2>&1 | tee -a "$LOG"
import json, os, sys, re, urllib.request
REPO, WORK, TAG, VERSION = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
TOKEN = open('/tmp/sk_tok.txt').read().strip()
# 取 CHANGELOG 对应版本段落作为 release notes
t = open('/Users/samjoeyang/workspace/unoSwitchKit/CHANGELOG.md', encoding='utf-8').read()
m = re.search(r'##\s*\[' + re.escape(VERSION) + r'\].*?(?=\n##\s|\Z)', t, re.S)
NOTES = (m.group(0).strip() if m else f'SwitchKit v{VERSION}').replace('\r','')
def req(method, url, data=None, binary=None, headers=None):
    h = {"Authorization": f"Bearer {TOKEN}", "Accept": "application/vnd.github+json"}
    if headers: h.update(headers)
    body = binary if binary is not None else (data.encode() if data is not None else None)
    r = urllib.request.Request(url, data=body, method=method, headers=h)
    try:
        with urllib.request.urlopen(r, timeout=300) as resp:
            return resp.read().decode(), resp.status
    except urllib.error.HTTPError as e:
        return e.read().decode(), e.code
# 若已存在同名 tag 的 release 则复用，否则创建
_, st = req("GET", f"https://api.github.com/repos/{REPO}/releases/tags/{TAG}")
if st == 200:
    rid = json.loads(_)["id"]
    print("REUSE_EXISTING_RELEASE", rid)
else:
    body = json.dumps({"tag_name":TAG,"name":TAG,"body":NOTES,"make_latest":"true"}, ensure_ascii=False)
    txt, st = req("POST", f"https://api.github.com/repos/{REPO}/releases", data=body)
    if st >= 400:
        print("RELEASE_CREATE_FAIL", st, txt); sys.exit(1)
    rid = json.loads(txt)["id"]
    print("RELEASE_ID", rid, "status", st)
for root,_,files in os.walk(WORK):
    for f in files:
        if f.startswith(f"SwitchKit-{VERSION}-") and f.endswith(".zip"):
            with open(os.path.join(root, f),"rb") as fh: data = fh.read()
            _, s = req("POST", f"https://api.github.com/repos/{REPO}/releases/{rid}/assets?name={f}",
                       binary=data, headers={"Content-Type":"application/octet-stream"})
            print("UPLOAD", f, "http", s)
print("RELEASE_DONE", rid)
PY
echo "[$(date)] script finished." | tee -a "$LOG"
