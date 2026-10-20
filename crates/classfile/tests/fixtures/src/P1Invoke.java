package demo;

import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import java.lang.invoke.VarHandle;

// Provenance for P1Invoke.class — MethodHandles/VarHandle additions in P1-b.
// Compiled with `-parameters -g --release 25` (major 69).
public class P1Invoke {
    public static class Holder {
        public int value;
    }

    public static String methodHandle() throws Throwable {
        MethodHandles.Lookup lookup = MethodHandles.lookup();
        MethodHandle length = lookup.findVirtual(
                String.class, "length", MethodType.methodType(int.class));
        int size = (int) length.invokeExact("hello");
        MethodHandle concat = lookup.findVirtual(
                String.class, "concat", MethodType.methodType(String.class, String.class));
        String text = (String) concat.bindTo("a").invokeExact("b");
        return size + ":" + text + ":" + (lookup.lookupClass() != null);
    }

    public static void main(String[] args) throws Throwable {
        System.out.println(methodHandle());
        System.out.println(varHandle());
    }

    public static String varHandle() throws Throwable {
        MethodHandles.Lookup lookup = MethodHandles.lookup();
        VarHandle handle = lookup.findVarHandle(Holder.class, "value", int.class);
        Holder holder = new Holder();
        handle.set((Object) holder, 41);
        int before = (Integer) handle.get((Object) holder);
        handle.set((Object) holder, 42);
        int after = (Integer) handle.get((Object) holder);
        return before + ":" + after;
    }
}
