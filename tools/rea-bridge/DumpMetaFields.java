import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.mem.Memory;
import ghidra.program.model.symbol.Reference;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;

import java.io.File;
import java.io.PrintWriter;
import java.util.*;
import java.util.regex.*;

public class DumpMetaFields extends GhidraScript {
    private static final Pattern PAT_4E38 = Pattern.compile(
        "FUN_80104e38\\s*\\([^,]+,\\s*([^,]+),\\s*[^,]+,\\s*([^\\);]+)\\)"
    );
    private static final Pattern PAT_5130 = Pattern.compile(
        "FUN_80105130\\s*\\([^,]+,\\s*([^,]+),\\s*[^,]+,\\s*([^,]+),"
    );

    private Memory mem;
    private Map<Long, String> regSymbols = new HashMap<>();
    private Map<Long, String> typeCtorMap = new HashMap<>();

    @Override
    public void run() throws Exception {
        mem = currentProgram.getMemory();

        File symFile = new File("tools/rea-bridge/symbols.txt");
        if (symFile.exists()) {
            Scanner s = new Scanner(symFile);
            while (s.hasNextLine()) {
                String line = s.nextLine().trim();
                String[] parts = line.split("\t");
                if (parts.length >= 2 && parts[0].startsWith("0x")) {
                    try {
                        long addr = Long.parseLong(parts[0].substring(2), 16);
                        regSymbols.put(addr, parts[1]);
                    } catch (Exception e) {}
                }
            }
            s.close();
        }

        File metaFile = new File("tools/rea-bridge/meta_objects.tsv");
        if (!metaFile.exists()) {
            printerr("meta_objects.tsv not found!");
            return;
        }

        List<ClassObject> classList = new ArrayList<>();
        Scanner s = new Scanner(metaFile);
        if (s.hasNextLine()) s.nextLine();
        while (s.hasNextLine()) {
            String line = s.nextLine().trim();
            String[] p = line.split("\t");
            if (p.length >= 5) {
                String name = p[0];
                long size = parseHex(p[1]);
                String ctor = p[2];
                if (ctor.startsWith("0x")) {
                    classList.add(new ClassObject(name, size, parseHex(ctor)));
                }
            }
        }
        s.close();

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        File outFile = new File("tools/rea-bridge/meta_fields.tsv");
        PrintWriter writer = new PrintWriter(outFile);
        writer.println("class_name\tfield_name\tfield_type\toffset\tsize");

        int totalFields = 0;
        int classesWithFields = 0;

        for (ClassObject cls : classList) {
            Address ctorAddr = currentProgram.getAddressFactory().getAddress(String.format("0x%08x", cls.ctorAddr));
            Address typesAddr = null;
            Address namesAddr = null;
            Address offsetsAddr = null;
            int count = 0;

            if (cls.name.equals("igImage2")) {
                typesAddr = currentProgram.getAddressFactory().getAddress("0x8060ba74");
                namesAddr = currentProgram.getAddressFactory().getAddress("0x8060bab8");
                offsetsAddr = currentProgram.getAddressFactory().getAddress("0x8060bb40");
                count = 17;
            } else {
                Function func = currentProgram.getFunctionManager().getFunctionAt(ctorAddr);
                if (func == null) {
                    try {
                        func = createFunction(ctorAddr, cls.name + "_ctor");
                    } catch (Exception e) {}
                }
                if (func == null) {
                    func = currentProgram.getFunctionManager().getFunctionContaining(ctorAddr);
                }
                if (func == null) continue;

                DecompileResults res = decomp.decompileFunction(func, 10, monitor);
                if (!res.decompileCompleted()) continue;

                String code = res.getDecompiledFunction().getC();
                Matcher m4 = PAT_4E38.matcher(code);
                Matcher m5 = PAT_5130.matcher(code);

                if (m4.find() && m5.find()) {
                    String typeArg = m4.group(1).trim();
                    String countArg = m4.group(2).trim();
                    String namesArg = m5.group(1).trim();
                    String offsetsArg = m5.group(2).trim();

                    count = (int) parseHex(countArg);
                    if (count <= 0 || count > 256) continue;

                    typesAddr = resolveAddress(typeArg);
                    namesAddr = resolveAddress(namesArg);
                    offsetsAddr = resolveAddress(offsetsArg);
                }
            }

            if (namesAddr == null || offsetsAddr == null || count <= 0) continue;

            classesWithFields++;
            for (int i = 0; i < count; i++) {
                Address namePtrAddr = namesAddr.add(i * 4);
                int nameStrAddr = mem.getInt(namePtrAddr);
                String fieldName = readString(nameStrAddr);
                if (fieldName == null || fieldName.isEmpty()) continue;

                Address offsetPtrAddr = offsetsAddr.add(i * 2);
                int offset = mem.getShort(offsetPtrAddr) & 0xFFFF;

                String typeName = "unknown";
                int fieldSize = 0;
                if (typesAddr != null) {
                    Address typeCtorPtr = typesAddr.add(i * 4);
                    long typeCtorAddr = mem.getInt(typeCtorPtr) & 0xFFFFFFFFL;
                    typeName = resolveTypeName(typeCtorAddr, decomp);
                }

                fieldSize = deriveFieldSize(typeName);

                writer.printf("%s\t%s\t%s\t0x%x\t0x%x\n",
                    cls.name, fieldName, typeName, offset, fieldSize);
                totalFields++;
            }
        }

        writer.flush();
        writer.close();
        println(String.format("Successfully dumped %d fields across %d classes to %s",
            totalFields, classesWithFields, outFile.getAbsolutePath()));
    }

    private String resolveTypeName(long ctorAddr, DecompInterface decomp) {
        if (typeCtorMap.containsKey(ctorAddr)) {
            return typeCtorMap.get(ctorAddr);
        }
        String name = "unknown";
        try {
            Address a = currentProgram.getAddressFactory().getAddress(String.format("0x%08x", ctorAddr));
            Function f = currentProgram.getFunctionManager().getFunctionAt(a);
            if (f != null) {
                DecompileResults res = decomp.decompileFunction(f, 5, monitor);
                if (res.decompileCompleted()) {
                    String c = res.getDecompiledFunction().getC();
                    Matcher m = Pattern.compile("FUN_80157000\\s*\\([^,]+,\\s*([^\\);]+)\\)").matcher(c);
                    if (m.find()) {
                        String regTarget = m.group(1).trim();
                        Address regAddr = resolveAddress(regTarget);
                        if (regAddr != null) {
                            long rOff = regAddr.getOffset();
                            String sym = regSymbols.get(rOff);
                            if (sym != null && sym.startsWith("register_")) {
                                name = sym.substring("register_".length());
                            }
                        }
                    }
                }
            }
        } catch (Exception e) {}
        typeCtorMap.put(ctorAddr, name);
        return name;
    }

    private int deriveFieldSize(String typeName) {
        if (typeName.equals("igBoolMetaField") || typeName.equals("igCharMetaField") || typeName.equals("igUnsignedCharMetaField")) return 1;
        if (typeName.equals("igShortMetaField") || typeName.equals("igUnsignedShortMetaField")) return 2;
        if (typeName.equals("igIntMetaField") || typeName.equals("igUnsignedIntMetaField") || typeName.equals("igFloatMetaField")) return 4;
        if (typeName.equals("igMemoryRefMetaField") || typeName.equals("igMemoryRefHandleMetaField")) return 8;
        if (typeName.equals("igVec2fMetaField")) return 8;
        if (typeName.equals("igVec3fMetaField") || typeName.equals("igVec3fAlignedMetaField")) return 12;
        if (typeName.equals("igVec4fMetaField") || typeName.equals("igQuaternionfMetaField")) return 16;
        if (typeName.equals("igMatrix44fMetaField")) return 64;
        if (typeName.equals("igObjectRefMetaField") || typeName.equals("igRawRefMetaField") || typeName.equals("igRefMetaField")) return 4;
        if (typeName.equals("igStringMetaField")) return 4;
        return 4;
    }

    private Address resolveAddress(String arg) {
        try {
            arg = arg.trim();
            if (arg.startsWith("&") || arg.startsWith("*")) arg = arg.substring(1).trim();
            int idx = arg.lastIndexOf("_80");
            if (idx != -1) {
                String hex = "0x" + arg.substring(idx + 1);
                return currentProgram.getAddressFactory().getAddress(hex);
            }
            if (arg.startsWith("0x") || arg.startsWith("0X")) {
                return currentProgram.getAddressFactory().getAddress(arg);
            }
        } catch (Exception e) {}
        return null;
    }

    private String readString(int addrVal) {
        if ((addrVal & 0xFF000000) != 0x80000000) return "";
        try {
            Address a = currentProgram.getAddressFactory().getAddress(String.format("0x%08x", addrVal));
            byte[] buf = new byte[128];
            int r = mem.getBytes(a, buf);
            int len = 0;
            while (len < r && buf[len] != 0) len++;
            return new String(buf, 0, len, "UTF-8");
        } catch (Exception e) {
            return "";
        }
    }

    private long parseHex(String s) {
        s = s.trim();
        if (s.startsWith("0x") || s.startsWith("0X")) s = s.substring(2);
        try {
            return Long.parseLong(s, 16);
        } catch (Exception e) {
            try {
                return Long.parseLong(s);
            } catch (Exception e2) {
                return 0;
            }
        }
    }

    private static class ClassObject {
        String name;
        long size;
        long ctorAddr;
        ClassObject(String name, long size, long ctorAddr) {
            this.name = name;
            this.size = size;
            this.ctorAddr = ctorAddr;
        }
    }
}
