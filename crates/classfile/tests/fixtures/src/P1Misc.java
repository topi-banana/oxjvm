package demo;

// Provenance for P1Misc.class — a regression fixture for the 2026-10-13 P1 batch:
// List.of, ArrayList(Collection)/Collections.synchronizedList, Locale.of/setDefault,
// Double.valueOf, Random.nextBytes, StringBuilder/StringBuffer.append(char),
// MethodType.methodType and Class.getModule.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1Misc.java
//     cp out/demo/P1Misc.class crates/classfile/tests/fixtures/
public class P1Misc {
    public static String misc() {
        java.util.List<String> of = java.util.List.of("a", "b", "c");
        java.util.List<String> sync = java.util.Collections.synchronizedList(
                new java.util.ArrayList<>(of));
        java.util.Locale.setDefault(java.util.Locale.of("fr", "FR"));
        java.util.Random random = new java.util.Random(42);
        byte[] bytes = new byte[4];
        random.nextBytes(bytes);
        StringBuilder sb = new StringBuilder();
        sb.append('x');
        StringBuffer sbf = new StringBuffer();
        sbf.append('y');
        return of.size() + ":" + sync.size() + ":" + java.util.Locale.getDefault() + ":"
                + bytes.length + ":" + sb + ":" + sbf + ":"
                + java.lang.invoke.MethodType.methodType(String.class) + ":"
                + P1Misc.class.getModule().isNamed() + ":" + Double.valueOf(1.5);
    }
}
