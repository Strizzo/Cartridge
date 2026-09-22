#!/bin/bash
# ARM Linux compatibility checks; never mounts the handheld/recovery disks.
# ./sim/vm.sh start | check <device-bundle-dir> | check --run <GitHub-run-id> | stop | status
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STATE="$ROOT/.sim/vm"
VM=cartridge-arm64
mkdir -p "$STATE"
case "${1:-help}" in
    start)
        # curl uses macOS's resolver; Lima's Go resolver can time out on VPN DNS.
        IMAGE="$STATE/images/ubuntu-arm64.img"
        mkdir -p "$(dirname "$IMAGE")"
        if [[ ! -f "$IMAGE" ]]; then
            curl --fail --location --retry 2 --connect-timeout 15 \
                https://cloud-images.ubuntu.com/minimal/releases/noble/release-20260716/ubuntu-24.04-minimal-cloudimg-arm64.img \
                -o "$IMAGE.part"
            mv "$IMAGE.part" "$IMAGE"
        fi
        python3 - "$IMAGE" <<'PY'
import hashlib,sys
from pathlib import Path
if hashlib.file_digest(open(sys.argv[1],'rb'),'sha256').hexdigest() != '7e938df669e3b1923595eeda97aa28569350c5283e05a835cc912a2486a54934':
    raise SystemExit('VM image checksum failed; image was not booted')
PY
        IMAGE_EXPR="$(python3 - "$IMAGE" <<'PY'
import json,sys
print('.images[0].location = '+json.dumps(sys.argv[1]))
PY
)"
        if limactl list --json "$VM" 2>/dev/null | python3 -c 'import sys,json; sys.exit(0 if any(json.loads(s).get("name")=="cartridge-arm64" for s in sys.stdin if s.strip()) else 1)'; then
            limactl edit --tty=false --set "$IMAGE_EXPR" "$VM"
            limactl start --tty=false --timeout=6m "$VM"
        else
            limactl start --tty=false --name="$VM" --set "$IMAGE_EXPR" --timeout=6m "$ROOT/sim/vm/lima.yaml"
        fi
        ;;
    check)
        if [[ "${2:-}" == --run ]]; then
            RUN="${3:-}"
            [[ "$RUN" =~ ^[0-9]+$ ]] || { echo 'Expected a numeric GitHub run ID' >&2; exit 2; }
            BUNDLE="$STATE/bundles/$RUN"
            if [[ ! -f "$BUNDLE/Cartridge/dev/sim-check" ]]; then
                gh run download "$RUN" --repo Strizzo/Cartridge --name cartridge-device-bundle-aarch64 --dir "$BUNDLE"
            fi
        else
            BUNDLE="${2:?Supply the extracted device bundle directory or --run ID}"
        fi
        [[ -f "$BUNDLE/Cartridge/dev/sim-check" ]] || { echo 'Bundle lacks ARM dev/sim-check; use a current CI bundle.' >&2; exit 1; }
        # Check the guest identity before transferring or changing its fixture.
        limactl shell --workdir=/ "$VM" sh -c 'test "$(uname -m)" = aarch64 && test -f /etc/cartridge-compat-vm'
        python3 - "$ROOT" "$BUNDLE" "$STATE/payload.tar.gz" <<'PY'
from pathlib import Path
import sys,tarfile
root,bundle,out=map(Path,sys.argv[1:])
with tarfile.open(out,'w:gz') as t:
    t.add(bundle/'Cartridge',arcname='bundle/Cartridge')
    for name in ['deploy','tests','sim']:
        t.add(root/name,arcname=name,filter=lambda info: None if '__pycache__' in info.name else info)
PY
        limactl copy "$STATE/payload.tar.gz" "$VM:/tmp/cartridge-vm-payload.tar.gz"
        limactl shell --workdir=/ "$VM" sh -c 'mkdir -p /tmp/cartridge-vm && tar -xzf /tmp/cartridge-vm-payload.tar.gz -C /tmp/cartridge-vm'
        limactl shell --workdir=/ "$VM" sudo python3 /tmp/cartridge-vm/sim/vm/guest-check.py
        mkdir -p "$STATE/results"
        limactl copy -r "$VM:/tmp/cartridge-vm/results/." "$STATE/results/"
        echo "ARM VM results and screenshots: $STATE/results"
        ;;
    stop) limactl stop "$VM" ;;
    status) limactl list "$VM" ;;
    *) sed -n '2,3p' "$0" ;;
esac
