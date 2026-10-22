#!/usr/bin/env python3
"""
Convert Nintendo GameCube/Wii DOL binary to standard 32-bit Big-Endian PowerPC ELF.
Allows Ghidra, REA, IDA, and standard binary analysis tools to load Wii main.dol directly.
"""
import sys
import struct
from pathlib import Path

def dol_to_elf(dol_bytes: bytes) -> bytes:
    if len(dol_bytes) < 0x100:
        raise ValueError("DOL file too small for header")

    text_offsets = struct.unpack(">7I", dol_bytes[0x00:0x1c])
    data_offsets = struct.unpack(">11I", dol_bytes[0x1c:0x48])
    text_addrs = struct.unpack(">7I", dol_bytes[0x48:0x64])
    data_addrs = struct.unpack(">11I", dol_bytes[0x64:0x90])
    text_lens = struct.unpack(">7I", dol_bytes[0x90:0xac])
    data_lens = struct.unpack(">11I", dol_bytes[0xac:0xd8])
    bss_addr, bss_len, entry = struct.unpack(">3I", dol_bytes[0xd8:0xe4])

    segments = []
    occupied = []
    for i in range(7):
        if text_lens[i] > 0:
            off = text_offsets[i]
            sz = text_lens[i]
            segments.append((text_addrs[i], 5, dol_bytes[off:off+sz], 0))
            occupied.append((text_addrs[i], text_addrs[i] + sz))

    for i in range(11):
        if data_lens[i] > 0:
            off = data_offsets[i]
            sz = data_lens[i]
            segments.append((data_addrs[i], 6, dol_bytes[off:off+sz], 0))
            occupied.append((data_addrs[i], data_addrs[i] + sz))

    occupied.sort()

    if bss_len > 0:
        curr = bss_addr
        bss_end = bss_addr + bss_len
        for s, e in occupied:
            if s >= bss_end:
                break
            if e <= curr:
                continue
            if s > curr:
                segments.append((curr, 6, b"", s - curr))
            curr = max(curr, e)
        if curr < bss_end:
            segments.append((curr, 6, b"", bss_end - curr))

    segments.sort(key=lambda s: s[0])

    # ELF32 Header
    # e_ident: 0x7f, 'E', 'L', 'F', ELFCLASS32 (1), ELFDATA2MSB (2), EV_CURRENT (1), ELFOSABI_NONE (0)
    e_ident = b"\x7fELF\x01\x02\x01\x00" + b"\x00" * 8
    e_type = 2          # ET_EXEC
    e_machine = 20      # EM_PPC
    e_version = 1
    e_entry = entry
    e_phoff = 52        # Right after 52-byte ELF header
    e_shoff = 0
    e_flags = 0
    e_ehsize = 52
    e_phentsize = 32
    e_phnum = len(segments)
    e_shentsize = 0
    e_shnum = 0
    e_shstrndx = 0

    ph_table_size = e_phnum * e_phentsize
    file_offset = e_ehsize + ph_table_size

    prog_headers = []
    payloads = []

    for vaddr, flags, content, mem_sz in segments:
        if mem_sz > 0 and len(content) == 0:
            p_type = 1
            p_offset = 0
            p_vaddr = p_paddr = vaddr
            p_filesz = 0
            p_memsz = mem_sz
            p_flags = flags
            p_align = 0x20
            prog_headers.append(struct.pack(">IIIIIIII", p_type, p_offset, p_vaddr, p_paddr, p_filesz, p_memsz, p_flags, p_align))
        else:
            align_pad = (0x20 - (file_offset % 0x20)) % 0x20
            file_offset += align_pad
            p_type = 1 # PT_LOAD
            p_offset = file_offset
            p_vaddr = p_paddr = vaddr
            p_filesz = len(content)
            p_memsz = len(content)
            p_flags = flags
            p_align = 0x20
            prog_headers.append(struct.pack(">IIIIIIII", p_type, p_offset, p_vaddr, p_paddr, p_filesz, p_memsz, p_flags, p_align))
            payloads.append((align_pad, content))
            file_offset += len(content)

    elf_header = e_ident + struct.pack(
        ">HHIIIIIHHHHHH",
        e_type, e_machine, e_version, e_entry, e_phoff, e_shoff,
        e_flags, e_ehsize, e_phentsize, e_phnum, e_shentsize, e_shnum, e_shstrndx
    )

    out = bytearray()
    out.extend(elf_header)
    for ph in prog_headers:
        out.extend(ph)
    for pad, data in payloads:
        if pad > 0:
            out.extend(b"\x00" * pad)
        out.extend(data)

    return bytes(out)

def main():
    if len(sys.argv) < 3:
        print("Usage: dol2elf.py <input.dol> <output.elf>")
        sys.exit(1)

    in_path = Path(sys.argv[1])
    out_path = Path(sys.argv[2])
    data = in_path.read_bytes()
    elf = dol_to_elf(data)
    out_path.write_bytes(elf)
    print(f"Successfully converted {in_path} to {out_path} ({len(elf)} bytes)")

if __name__ == "__main__":
    main()
