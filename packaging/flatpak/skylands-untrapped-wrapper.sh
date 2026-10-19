#!/bin/sh
set -e

DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/skylands-untrapped"
ASSETS_DIR="$DATA_DIR/assets"

mkdir -p "$ASSETS_DIR"

# If assets-dir is not specified in arguments, provide default data dir
has_assets_arg=0
for arg in "$@"; do
    case "$arg" in
        --assets-dir*|-a*)
            has_assets_arg=1
            ;;
    esac
done

if [ "$has_assets_arg" -eq 0 ]; then
    set -- "$@" --assets-dir "$ASSETS_DIR"
fi

exec /app/bin/skylands-app "$@"
