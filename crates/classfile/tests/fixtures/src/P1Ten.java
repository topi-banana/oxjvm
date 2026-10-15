package demo;

// Provenance for P1Ten.class — a regression fixture for EnumMap, StackWalker
// ($Option/$StackFrame) and ThreadFactory.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1Ten.java
//     cp out/demo/P1Ten.class crates/classfile/tests/fixtures/
public class P1Ten {
    enum Color { RED, GREEN, BLUE }

    public static String ten() {
        java.util.EnumMap<Color, String> map = new java.util.EnumMap<>(Color.class);
        map.put(Color.RED, "r");
        java.util.Map<Color, String> asMap = map;
        asMap.put(Color.GREEN, "g");
        String walk = java.lang.StackWalker.getInstance(
                java.lang.StackWalker.Option.RETAIN_CLASS_REFERENCE)
            .walk(frames -> frames.map(f -> f.getMethodName()).limit(2).toList().toString());
        java.util.concurrent.ThreadFactory factory = r -> new Thread(r);
        Thread thread = factory.newThread(() -> { });
        return map.size() + ":" + map.get(Color.GREEN) + ":" + asMap.get(Color.RED)
                + ":" + java.lang.StackWalker.Option.values().length + ":" + walk + ":"
                + (thread != null);
    }
}
