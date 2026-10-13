package demo;

// Provenance for ApiBatch.class — a regression fixture for the java.base APIs added after the
// 2026-10-12 jtreg run: Arrays.copyOfRange/copyOf/asList/fill, List.toArray(T[]),
// Collections.addAll, Long.getLong/Integer.getInteger, Float/Boolean.valueOf,
// Class.forName(String, boolean, ClassLoader), Enum.valueOf, StringBuffer(int),
// Objects.checkIndex and Integer.max.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/ApiBatch.java
//     cp out/demo/ApiBatch*.class crates/classfile/tests/fixtures/
public class ApiBatch {
    public enum Color { RED, GREEN }

    public static String copyOfRange() {
        Object[] source = {"a", "b", "c", "d"};
        Object[] slice = java.util.Arrays.copyOfRange(source, 1, 6);
        return slice.length + ":" + slice[0] + slice[2] + (slice[4] == null);
    }

    public static String asListToArray() {
        java.util.List<String> list = java.util.Arrays.asList("x", "y", "z");
        String[] out = list.toArray(new String[0]);
        return list.size() + out[0] + out[2];
    }

    public static String fillAndAddAll() {
        int[] ints = new int[3];
        java.util.Arrays.fill(ints, 7);
        java.util.ArrayList<String> list = new java.util.ArrayList<>();
        boolean added = java.util.Collections.addAll(list, "a", "b");
        return ints[2] + ":" + added + list.size();
    }

    public static String boxedFactories() {
        return Long.getLong("demo.missing", 5L) + ":"
                + Integer.getInteger("demo.missing", 3) + ":"
                + Float.valueOf(1.5f) + ":" + Boolean.valueOf("TRUE");
    }

    public static String forNameAndEnum() throws Exception {
        Class<?> stringClass = Class.forName("java.lang.String", true, null);
        return stringClass.getName() + ":" + Enum.valueOf(Color.class, "GREEN");
    }

    public static String misc() {
        StringBuffer buffer = new StringBuffer(64);
        buffer.append("ok");
        return buffer + ":" + java.util.Objects.checkIndex(1, 3) + ":" + Integer.max(3, 9);
    }
}
