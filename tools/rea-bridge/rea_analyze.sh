#!/usr/bin/env bash
set -euo pipefail

ROM="${1:-rom.eu.wbfs}"
WORK_DIR="/tmp/skylands-re"
mkdir -p "$WORK_DIR"

echo "[1/3] Extracting main.dol from $ROM..."
nix-shell -p wiimms-iso-tools --run "wit extract '$ROM' '$WORK_DIR' --overwrite --files +sys/main.dol"

echo "[2/3] Converting main.dol to PowerPC ELF..."
python3 tools/rea-bridge/dol2elf.py "$WORK_DIR/sys/main.dol" "$WORK_DIR/main.elf"

echo "[3/3] Running REA analysis..."
if command -v ghidra &>/dev/null; then
    echo "Ghidra available at $(command -v ghidra)"
    nix-shell -p cargo rustc pkg-config udev --run "echo 'Ghidra and Rust ready'"
fi

echo "Done. Target ELF is ready at $WORK_DIR/main.elf"
