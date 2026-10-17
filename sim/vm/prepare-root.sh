#!/bin/bash
# Prepare a disposable host-staged ext4 clone in a separate, narrow-mount VM.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STAGING="${1:?Supply an absolute, already validated staging directory}"
BUNDLE="${2:?Supply a verified device bundle /Cartridge directory}"
APP_PATH="${3:-/roms/Cartridge}"
[[ "$STAGING" = /* && -d "$STAGING" && ! -L "$STAGING" ]] || { echo 'Staging must be an existing absolute directory' >&2; exit 2; }
[[ -f "$STAGING/prepared-root.ext4" && ! -L "$STAGING/prepared-root.ext4" ]] || { echo 'Disposable prepared-root.ext4 is missing' >&2; exit 2; }
[[ -f "$STAGING/host-report.json" && ! -L "$STAGING/host-report.json" ]] || { echo 'Validated host preparation report is missing' >&2; exit 2; }
[[ "$APP_PATH" == /roms/Cartridge || "$APP_PATH" == /roms2/Cartridge ]] || { echo 'Unsupported device app path' >&2; exit 2; }
[[ -f "$BUNDLE/dev/build-revision" ]] || { echo 'Expected a CI device bundle' >&2; exit 2; }
VM=cartridge-prep-arm64
IMAGE="$ROOT/.sim/vm/images/ubuntu-arm64.img"
if [[ ! -f "$IMAGE" ]]; then
    mkdir -p "$(dirname "$IMAGE")"
    curl --fail --location --retry 2 --connect-timeout 15 \
        https://cloud-images.ubuntu.com/minimal/releases/noble/release-20260716/ubuntu-24.04-minimal-cloudimg-arm64.img \
        -o "$IMAGE.part"
    mv "$IMAGE.part" "$IMAGE"
fi
python3 - "$IMAGE" <<'PY'
import hashlib,sys
value=hashlib.sha256()
with open(sys.argv[1],'rb') as stream:
    for block in iter(lambda:stream.read(1024*1024),b''):value.update(block)
if value.hexdigest() != '7e938df669e3b1923595eeda97aa28569350c5283e05a835cc912a2486a54934':
    raise SystemExit('Pinned VM image hash differs; refusing to boot')
PY
IMAGE_EXPR="$(python3 - "$IMAGE" <<'PY'
import json,sys
print('.images[0].location = '+json.dumps(sys.argv[1]))
PY
)"
if limactl list --json "$VM" 2>/dev/null | python3 -c 'import sys,json; sys.exit(0 if any(json.loads(s).get("name")=="cartridge-prep-arm64" for s in sys.stdin if s.strip()) else 1)'; then
    # Never silently inherit a previous host mount. Replace it while stopped.
    limactl stop "$VM" 2>/dev/null || true
    limactl edit --tty=false --set "$IMAGE_EXPR" --mount-only "$STAGING:w" "$VM"
    limactl start --tty=false --timeout=6m "$VM"
else
    limactl start --tty=false --name="$VM" --set "$IMAGE_EXPR" --mount-only "$STAGING:w" \
        --timeout=6m "$ROOT/sim/vm/prep-lima.yaml"
fi
cleanup() {
    local status=$?
    limactl stop "$VM" || status=1
    # The staging directory may be deleted after a successful smoke check.
    # Do not retain its writable host mount in the stopped VM configuration.
    limactl edit --tty=false --mount-none "$VM" || status=1
    exit "$status"
}
trap cleanup EXIT
limactl shell --workdir=/ "$VM" sh -c 'test "$(uname -m)" = aarch64 && test -f /etc/cartridge-prep-vm'
limactl copy "$ROOT/installer/offline_prepare.py" "$VM:/tmp/cartridge-prep-offline_prepare.py"
limactl copy "$ROOT/sim/vm/prepare-root-guest.py" "$VM:/tmp/cartridge-prep-guest.py"
limactl copy -r "$BUNDLE" "$VM:/tmp/cartridge-prep-bundle"
limactl shell --workdir=/ "$VM" sudo python3 /tmp/cartridge-prep-guest.py \
    "$STAGING" /tmp/cartridge-prep-bundle "$APP_PATH"
