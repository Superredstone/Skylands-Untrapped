#!/usr/bin/env python3
"""Minimal PowerPC / Gekko disassembler over a Wii `main.dol`, for code stock Ghidra
cannot decode (paired-single ops cut its analysis short, e.g. the slider tick).

Usage: python3 -I tools/rea-bridge/ppcdis.py <main.dol> <hex address> <count> [nostop]

Covers the loads/stores, integer and float arithmetic, compares, branches and
paired-single load/store forms seen in script op code; anything else prints as
`.word`. `lfs`/`lfd` relative to r2 print the SDA2 constant (r2 = 0x806f5a40, set at
0x8000654c). Stops at the first `blr` unless `nostop` is given.
"""
import struct
import sys

R2 = 0x806F5A40

dol = open(sys.argv[1], "rb").read()
offs = struct.unpack(">18I", dol[0:72])
addrs = struct.unpack(">18I", dol[72:144])
sizes = struct.unpack(">18I", dol[144:216])


def rd(a):
    for o, ad, sz in zip(offs, addrs, sizes):
        if ad <= a < ad + sz:
            return struct.unpack(">I", dol[o + a - ad:o + a - ad + 4])[0]
    return None


def s16(v):
    return v - 0x10000 if v & 0x8000 else v


def sx(v, bits):
    return v - (1 << bits) if v & (1 << (bits - 1)) else v


MEM = {32: "lwz", 33: "lwzu", 34: "lbz", 36: "stw", 37: "stwu", 38: "stb", 40: "lhz",
       44: "sth", 48: "lfs", 49: "lfsu", 50: "lfd", 52: "stfs", 53: "stfsu", 54: "stfd"}


def dis(a, w):
    op = w >> 26
    rd_, ra, rb = (w >> 21) & 31, (w >> 16) & 31, (w >> 11) & 31
    imm = s16(w & 0xFFFF)
    if op in MEM:
        f = "f" if MEM[op].startswith(("lf", "stf")) else "r"
        return f"{MEM[op]} {f}{rd_},{imm:#x}(r{ra})"
    if op == 14:
        return f"addi r{rd_},r{ra},{imm:#x}" if ra else f"li r{rd_},{imm:#x}"
    if op == 15:
        return f"addis r{rd_},r{ra},{imm:#x}" if ra else f"lis r{rd_},{imm:#x}"
    if op == 24:
        return f"ori r{ra},r{rd_},{w & 0xFFFF:#x}"
    if op == 28:
        return f"andi. r{ra},r{rd_},{w & 0xFFFF:#x}"
    if op == 11:
        return f"cmpwi cr{rd_ >> 2},r{ra},{imm:#x}"
    if op == 10:
        return f"cmplwi cr{rd_ >> 2},r{ra},{w & 0xFFFF:#x}"
    if op == 7:
        return f"mulli r{rd_},r{ra},{imm:#x}"
    if op == 8:
        return f"subfic r{rd_},r{ra},{imm:#x}"
    if op == 12:
        return f"addic r{rd_},r{ra},{imm:#x}"
    if op == 18:
        t = sx(w & 0x3FFFFFC, 26)
        dest = (a + t) & 0xFFFFFFFF if not w & 2 else t & 0xFFFFFFFF
        return f"{'bl' if w & 1 else 'b'} {dest:#x}"
    if op == 16:
        t = sx(w & 0xFFFC, 16)
        return f"bc bo={rd_} bi={ra} -> {(a + t) & 0xFFFFFFFF:#x}"
    if op == 19:
        xo = (w >> 1) & 1023
        return {16: "bclr (blr)", 528: "bcctr", 449: "cror", 0: "mcrf"}.get(xo, f"op19 xo={xo}")
    if op == 21:
        return f"rlwinm r{ra},r{rd_},{rb},{(w >> 6) & 31},{(w >> 1) & 31}"
    if op == 59:
        xo, rc = (w >> 1) & 31, (w >> 6) & 31
        n = {18: "fdivs", 20: "fsubs", 21: "fadds", 24: "fres", 25: "fmuls", 28: "fmsubs",
             29: "fmadds", 30: "fnmsubs", 31: "fnmadds"}.get(xo, f"op59.{xo}")
        if xo == 25:
            return f"{n} f{rd_},f{ra},f{rc}"
        if xo in (28, 29, 30, 31):
            return f"{n} f{rd_},f{ra},f{rc},f{rb}"
        return f"{n} f{rd_},f{ra},f{rb}"
    if op == 63:
        xo, xo5, rc = (w >> 1) & 1023, (w >> 1) & 31, (w >> 6) & 31
        named = {0: f"fcmpu cr{rd_ >> 2},f{ra},f{rb}", 32: f"fcmpo cr{rd_ >> 2},f{ra},f{rb}",
                 72: f"fmr f{rd_},f{rb}", 40: f"fneg f{rd_},f{rb}", 264: f"fabs f{rd_},f{rb}",
                 12: f"frsp f{rd_},f{rb}", 15: f"fctiwz f{rd_},f{rb}"}
        if xo in named:
            return named[xo]
        n = {18: "fdiv", 20: "fsub", 21: "fadd", 23: "fsel", 25: "fmul", 28: "fmsub",
             29: "fmadd", 30: "fnmsub", 31: "fnmadd"}.get(xo5, f"op63.{xo}")
        if xo5 == 25:
            return f"{n} f{rd_},f{ra},f{rc}"
        if xo5 in (23, 28, 29, 30, 31):
            return f"{n} f{rd_},f{ra},f{rc},f{rb}"
        return f"{n} f{rd_},f{ra},f{rb}"
    if op in (56, 60):
        n = "psq_l" if op == 56 else "psq_st"
        return f"{n} f{rd_},{sx(w & 0xFFF, 12):#x}(r{ra}) w={(w >> 15) & 1} i={(w >> 12) & 7}"
    if op == 4:
        return f"ps op4 xo={(w >> 1) & 1023:#x} f{rd_},f{ra},f{rb}"
    if op == 31:
        xo = (w >> 1) & 1023
        n = {0: "cmpw", 23: "lwzx", 24: "slw", 26: "cntlzw", 28: "and", 32: "cmplw",
             40: "subf", 104: "neg", 151: "stwx", 202: "addze", 235: "mullw", 266: "add",
             316: "xor", 339: "mfspr", 444: "or", 467: "mtspr", 491: "divw", 535: "lfsx",
             536: "srw", 663: "stfsx", 792: "sraw", 824: "srawi", 922: "extsh",
             954: "extsb"}.get(xo, f"op31.{xo}")
        # Logical ops and shifts write rA from rS (the rD field): `or rA,rS,rB`.
        if xo in (24, 26, 28, 316, 444, 536, 792, 824, 922, 954):
            if xo == 444 and rd_ == rb:
                return f"mr r{ra},r{rd_}"
            return f"{n} r{ra},r{rd_},r{rb}"
        return f"{n} r{rd_},r{ra},r{rb}"
    return f".word {w:08x}"


def main():
    a = int(sys.argv[2], 16)
    count = int(sys.argv[3])
    stop = len(sys.argv) < 5
    for _ in range(count):
        w = rd(a)
        if w is None:
            break
        s = dis(a, w)
        if "(r2)" in s and s.startswith(("lfs", "lfd")):
            off = s16(w & 0xFFFF)
            v = rd(R2 + off)
            if v is not None:
                if s.startswith("lfs"):
                    s += f"   ; = {struct.unpack('>f', struct.pack('>I', v))[0]}"
                else:
                    v2 = rd(R2 + off + 4) or 0
                    s += f"   ; = {struct.unpack('>d', struct.pack('>II', v, v2))[0]}"
        print(f"{a:08x}  {w:08x}  {s}")
        if w == 0x4E800020 and stop:
            break
        a += 4


if __name__ == "__main__":
    main()
