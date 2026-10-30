#!/usr/bin/env python3
"""Map a script op trace (`ActorScriptState::trace`) to listing lines.

Input: lines `TRACE <name> L1a2b3c G4d5e6f ...` (L = level op, G = `global.bld` op,
hex op offsets), as a test prints them. Listings come from
`cargo test -p skylands-formats dump_scripts -- --ignored` (`SKY_BLD=dir/name`), which
writes `/tmp/skylands-re/<name>/<script>.txt` with `@offset` per op.

Usage: python3 -I tools/rea-bridge/maptrace.py <trace file> <level dump dir> [global dump dir]
Consecutive repeats of the same line are folded.
"""
import glob
import re
import sys


def load(kind, d, idx):
    for f in glob.glob(d + "/*.txt"):
        name = f.rsplit("/", 1)[1][:-4]
        for line in open(f, errors="replace"):
            m = re.search(r"@([0-9a-f]+) ", line)
            if m:
                idx.setdefault((kind, int(m.group(1), 16)), f"{name}: {line.strip()[:160]}")


def main():
    idx = {}
    load("L", sys.argv[2], idx)
    if len(sys.argv) > 3:
        load("G", sys.argv[3], idx)
    for line in open(sys.argv[1]):
        parts = line.split()
        if len(parts) < 2 or parts[0] != "TRACE":
            continue
        print("==", parts[1], len(parts) - 2, "ops")
        prev = None
        for tok in parts[2:]:
            text = idx.get((tok[0], int(tok[1:], 16)), f"? {tok}")
            if text != prev:
                print("  ", tok[0], text)
            prev = text


if __name__ == "__main__":
    main()
