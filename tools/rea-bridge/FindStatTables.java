// Dumps decompiled candidates for gameplay stat tables to /tmp/skylands-re/stats_dump.txt.
// Output is local-only (decompiler text is never committed); record addresses in Rust/notes.
// Args (optional): extra function addresses (hex) to decompile.
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.data.StringDataInstance;
import ghidra.program.model.listing.Data;
import ghidra.program.model.listing.DataIterator;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;

import java.io.PrintWriter;
import java.util.*;

public class FindStatTables extends GhidraScript {
    private static final String[] KEYWORDS = {
        "groundmaxspeed", "jumpinitialspeed", "gravityacceleration", "modelparameters",
        "maxhealth", "health", "hitpoints", "experience", "xp", "levelup", "damage",
        "speed", "elemental", "affinity", "hpmultiplier", "damagemultiplier",
    };
    private static final long MODEL_PARAMETERS_CTOR = 0x805a89fcL;

    private DecompInterface decomp;
    private final Set<Address> done = new HashSet<>();
    private PrintWriter out;

    @Override
    public void run() throws Exception {
        decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        new java.io.File("/tmp/skylands-re").mkdirs();
        out = new PrintWriter("/tmp/skylands-re/stats_dump.txt");

        Address ctor = toAddr(MODEL_PARAMETERS_CTOR);
        section("ModelParameters ctor");
        dump(ctor);
        section("ModelParameters ctor callers");
        for (Reference r : getReferencesTo(ctor)) {
            Function f = getFunctionContaining(r.getFromAddress());
            if (f != null) dump(f.getEntryPoint());
        }

        for (String a : getScriptArgs()) {
            section("arg " + a);
            dump(toAddr(Long.parseLong(a.replace("0x", ""), 16)));
        }

        section("stat strings");
        DataIterator it = currentProgram.getListing().getDefinedData(true);
        List<Address> fns = new ArrayList<>();
        while (it.hasNext() && !monitor.isCancelled()) {
            Data d = it.next();
            if (!d.hasStringValue()) continue;
            String s = StringDataInstance.getStringDataInstance(d).getStringValue();
            if (s == null) continue;
            String l = s.toLowerCase();
            if (Arrays.stream(KEYWORDS).noneMatch(l::contains)) continue;
            StringBuilder refs = new StringBuilder();
            for (Reference r : getReferencesTo(d.getAddress())) {
                Function f = getFunctionContaining(r.getFromAddress());
                refs.append(' ').append(r.getFromAddress());
                if (f != null) {
                    refs.append('(').append(f.getEntryPoint()).append(')');
                    if (fns.size() < 200) fns.add(f.getEntryPoint());
                }
            }
            out.println(d.getAddress() + "\t\"" + s + "\"\t" + refs);
        }
        section("string xref functions");
        for (Address a : fns) dump(a);
        out.close();
        println("wrote /tmp/skylands-re/stats_dump.txt");
    }

    private void section(String name) {
        out.println("\n======== " + name + " ========");
    }

    private void dump(Address a) {
        Function f = getFunctionAt(a);
        if (f == null) f = getFunctionContaining(a);
        if (f == null || !done.add(f.getEntryPoint())) return;
        DecompileResults r = decomp.decompileFunction(f, 60, monitor);
        out.println("\n// ---- " + f.getEntryPoint() + " " + f.getName());
        out.println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "// decompile failed");
    }
}
