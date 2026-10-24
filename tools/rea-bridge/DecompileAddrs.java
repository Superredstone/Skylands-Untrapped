// Decompiles the functions at the given hex addresses into /tmp/skylands-re/decomp.txt.
// Output is local-only (decompiler text is never committed); record addresses in notes.
// Usage: rea_analyze.sh-style -process run with `-postScript DecompileAddrs.java 0x8008fd70 ...`.
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;

import java.io.PrintWriter;

public class DecompileAddrs extends GhidraScript {
    @Override
    public void run() throws Exception {
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        new java.io.File("/tmp/skylands-re").mkdirs();
        try (PrintWriter out = new PrintWriter("/tmp/skylands-re/decomp.txt")) {
            for (String a : getScriptArgs()) {
                Address addr = toAddr(Long.parseLong(a.replace("0x", ""), 16));
                Function f = getFunctionAt(addr);
                if (f == null) {
                    f = createFunction(addr, null);
                }
                if (f == null) {
                    out.println("// no function at " + a);
                    continue;
                }
                DecompileResults r = decomp.decompileFunction(f, 60, monitor);
                out.println("// ===== " + a + " " + f.getName());
                out.println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "// failed");
            }
        }
    }
}
