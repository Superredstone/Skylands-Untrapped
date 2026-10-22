import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.Symbol;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.mem.Memory;

import java.io.File;
import java.io.PrintWriter;
import java.util.*;
import java.util.regex.*;

public class DumpMetaObjects extends GhidraScript {
    private static final Pattern CALL_PAT = Pattern.compile(
        "FUN_801062e0\\s*\\([^,]+,[^,]+,[^,]+,[^,]+,([^,]+),([^,]+),[^,]+,([^,]+),([^\\);]+)\\)"
    );

    @Override
    public void run() throws Exception {
        Address target = currentProgram.getAddressFactory().getAddress("0x801062e0");
        Reference[] refs = getReferencesTo(target);
        println("Found " + refs.length + " references to igMetaObject::Register (0x801062e0)");

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        Memory mem = currentProgram.getMemory();
        File outFile = new File("tools/rea-bridge/meta_objects.tsv");
        PrintWriter writer = new PrintWriter(outFile);
        writer.println("class_name\tsize\tctor_addr\tvtable_addr\tcaller_addr");

        int success = 0;
        Set<Address> processedFuncs = new HashSet<>();

        for (Reference ref : refs) {
            Address from = ref.getFromAddress();
            Function func = currentProgram.getFunctionManager().getFunctionContaining(from);
            if (func == null || processedFuncs.contains(func.getEntryPoint())) {
                continue;
            }
            processedFuncs.add(func.getEntryPoint());

            DecompileResults res = decomp.decompileFunction(func, 10, monitor);
            if (!res.decompileCompleted()) {
                continue;
            }

            String cCode = res.getDecompiledFunction().getC();
            Matcher m = CALL_PAT.matcher(cCode);
            while (m.find()) {
                String strArg = m.group(1).trim();
                String sizeArg = m.group(2).trim();
                String ctorArg = m.group(3).trim();
                String vtableArg = m.group(4).trim();

                String className = resolveString(strArg, mem);
                long size = parseSize(sizeArg);
                String ctorAddr = resolveAddr(ctorArg);
                String vtableAddr = resolveAddr(vtableArg);

                if (className != null && !className.isEmpty()) {
                    writer.printf("%s\t0x%x\t%s\t%s\t%s\n",
                        className, size, ctorAddr, vtableAddr, func.getEntryPoint().toString());
                    success++;
                }
            }
        }

        writer.flush();
        writer.close();
        println("Successfully dumped " + success + " meta objects to " + outFile.getAbsolutePath());

        File symFile = new File("tools/rea-bridge/symbols.txt");
        PrintWriter symWriter = new PrintWriter(symFile);
        List<Map.Entry<Long, String>> symList = new ArrayList<>();
        symList.add(new AbstractMap.SimpleEntry<>(0x80006310L, "_start"));
        symList.add(new AbstractMap.SimpleEntry<>(0x800064d0L, "__init_registers"));
        symList.add(new AbstractMap.SimpleEntry<>(0x801062e0L, "igMetaObject::Register"));

        Scanner scanner = new Scanner(outFile);
        if (scanner.hasNextLine()) scanner.nextLine();
        while (scanner.hasNextLine()) {
            String[] parts = scanner.nextLine().split("\t");
            if (parts.length >= 5) {
                String name = parts[0];
                String ctor = parts[2];
                String vtable = parts[3];
                String caller = parts[4];
                if (ctor.startsWith("0x")) {
                    symList.add(new AbstractMap.SimpleEntry<>(Long.parseLong(ctor.substring(2), 16), name + "::" + name));
                }
                if (vtable.startsWith("0x")) {
                    symList.add(new AbstractMap.SimpleEntry<>(Long.parseLong(vtable.substring(2), 16), name + "::vftable"));
                }
                if (caller.startsWith("0x")) {
                    symList.add(new AbstractMap.SimpleEntry<>(Long.parseLong(caller.substring(2), 16), "register_" + name));
                } else if (caller.length() == 8) {
                    symList.add(new AbstractMap.SimpleEntry<>(Long.parseLong(caller, 16), "register_" + name));
                }
            }
        }
        scanner.close();

        symList.sort(Comparator.comparingLong(Map.Entry::getKey));
        Set<Long> seen = new HashSet<>();
        for (Map.Entry<Long, String> entry : symList) {
            if (seen.add(entry.getKey())) {
                symWriter.printf("0x%08x\t%s\n", entry.getKey(), entry.getValue());
            }
        }
        symWriter.flush();
        symWriter.close();
        println("Successfully wrote " + seen.size() + " symbols to " + symFile.getAbsolutePath());
    }

    private String resolveString(String arg, Memory mem) {
        try {
            if (arg.startsWith("&") || arg.startsWith("*")) {
                arg = arg.substring(1).trim();
            }
            if (arg.startsWith("\"") && arg.endsWith("\"")) {
                return arg.substring(1, arg.length() - 1);
            }
            Address addr = null;
            if (arg.contains("_80") || arg.contains("_0x80")) {
                int idx = arg.lastIndexOf("_80");
                if (idx != -1) {
                    String hexStr = arg.substring(idx + 1);
                    addr = currentProgram.getAddressFactory().getAddress("0x" + hexStr);
                }
            }
            if (addr == null) {
                List<Symbol> syms = currentProgram.getSymbolTable().getGlobalSymbols(arg);
                if (!syms.isEmpty()) {
                    addr = syms.get(0).getAddress();
                }
            }
            if (addr != null) {
                byte[] buf = new byte[128];
                int read = mem.getBytes(addr, buf);
                for (int i = 0; i < read; i++) {
                    if (buf[i] == 0) {
                        return new String(buf, 0, i, "UTF-8");
                    }
                }
            }
        } catch (Exception e) {
            // ignore
        }
        return arg;
    }

    private long parseSize(String sizeStr) {
        try {
            sizeStr = sizeStr.trim();
            if (sizeStr.startsWith("0x") || sizeStr.startsWith("0X")) {
                return Long.parseLong(sizeStr.substring(2), 16);
            }
            return Long.parseLong(sizeStr);
        } catch (Exception e) {
            return 0;
        }
    }

    private String resolveAddr(String arg) {
        arg = arg.trim();
        if (arg.startsWith("&")) arg = arg.substring(1).trim();
        int idx = arg.lastIndexOf("_80");
        if (idx != -1) {
            return "0x" + arg.substring(idx + 1);
        }
        return arg;
    }
}
