#!/usr/bin/env bash
set -euo pipefail

INPUT="${1:-rom.eu.wbfs}"
WORK_DIR="/tmp/skylands-re"
mkdir -p "$WORK_DIR"

find_analyze_headless() {
    local target="$1"
    local dir
    dir="$(dirname "$target")"
    if [ -x "$dir/support/analyzeHeadless" ]; then
        echo "$dir/support/analyzeHeadless"
        return 0
    fi
    if [ -x "$dir/../support/analyzeHeadless" ]; then
        echo "$dir/../support/analyzeHeadless"
        return 0
    fi
    return 1
}

if [ -z "${GHIDRA_HEADLESS:-}" ]; then
    if command -v ghidra &>/dev/null; then
        GHIDRA_BIN="$(readlink -f "$(command -v ghidra)")"
        GHIDRA_HEADLESS="$(find_analyze_headless "$GHIDRA_BIN" || true)"
    fi
    if [ -z "${GHIDRA_HEADLESS:-}" ] && [ -x "/run/current-system/sw/bin/ghidra" ]; then
        GHIDRA_BIN="$(readlink -f /run/current-system/sw/bin/ghidra)"
        GHIDRA_HEADLESS="$(find_analyze_headless "$GHIDRA_BIN" || true)"
    fi
    if [ -z "${GHIDRA_HEADLESS:-}" ] && command -v analyzeHeadless &>/dev/null; then
        GHIDRA_HEADLESS="$(command -v analyzeHeadless)"
    fi
fi

if [ -z "${GHIDRA_HEADLESS:-}" ] || [ ! -x "${GHIDRA_HEADLESS:-}" ]; then
    echo "Error: Ghidra analyzeHeadless not found" >&2
    exit 1
fi

case "$INPUT" in
    *.elf)
        ELF_PATH="$INPUT"
        ;;
    *.dol)
        ELF_PATH="$WORK_DIR/main.elf"
        echo "[1/2] Converting $INPUT to PowerPC ELF..."
        python3 tools/rea-bridge/dol2elf.py "$INPUT" "$ELF_PATH"
        ;;
    *)
        echo "[1/3] Extracting main.dol from $INPUT..."
        if command -v wit >/dev/null 2>&1; then
            wit extract "$INPUT" "$WORK_DIR" --overwrite --files +sys/main.dol
        elif command -v devenv >/dev/null 2>&1; then
            devenv shell -- wit extract "$INPUT" "$WORK_DIR" --overwrite --files +sys/main.dol
        else
            echo "Error: 'wit' not found in PATH or devenv"
            exit 1
        fi
        ELF_PATH="$WORK_DIR/main.elf"
        echo "[2/3] Converting main.dol to PowerPC ELF..."
        python3 tools/rea-bridge/dol2elf.py "$WORK_DIR/sys/main.dol" "$ELF_PATH"
        ;;
esac

PROCESSOR="${GHIDRA_LANG:-PowerPC:BE:32:default}"

PROJECT_DIR="$WORK_DIR/ghidra_project"
mkdir -p "$PROJECT_DIR"
PROJECT_NAME="SkylandsTrapTeam"

EXTRA_ARGS=()
if [ "$#" -gt 1 ]; then
    EXTRA_ARGS=("${@:2}")
else
    EXTRA_ARGS=(-scriptPath tools/rea-bridge -postScript DumpMetaObjects.java -postScript DumpMetaFields.java)
fi

echo "[3/3] Running Ghidra headless analysis..."
"$GHIDRA_HEADLESS" "$PROJECT_DIR" "$PROJECT_NAME" \
    -import "$ELF_PATH" \
    -processor "$PROCESSOR" \
    -cspec default \
    -overwrite \
    "${EXTRA_ARGS[@]}"

echo "Done. Ghidra project at $PROJECT_DIR/$PROJECT_NAME.gpr"
