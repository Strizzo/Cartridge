#!/bin/bash
# Compatibility entry point. Configure next boot without stopping this session.
set -euo pipefail
CARTRIDGE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ "$(id -u)" -eq 0 ]]; then
    exec /usr/bin/python3 "$CARTRIDGE_DIR/setup-primary.py" enable --cartridge-dir "$CARTRIDGE_DIR"
fi
exec sudo /usr/bin/python3 "$CARTRIDGE_DIR/setup-primary.py" enable --cartridge-dir "$CARTRIDGE_DIR"
