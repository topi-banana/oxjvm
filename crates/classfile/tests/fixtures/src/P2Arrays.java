package demo;

// Provenance for P2Arrays.class — a regression fixture for the 2026-10-22 P0/P1 batch:
// java.util.Arrays.hashCode for every primitive component type and for Object[],
// plus Arrays.deepEquals/deepHashCode (which the JDK declares on Arrays, not on Objects).
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P2Arrays.java
//     cp out/demo/P2Arrays.class crates/classfile/tests/fixtures/
public class P2Arrays {
    public static String hashCodes() {
        int[] ints = {1, 2, 3};
        long[] longs = {1L, 2L};
        double[] doubles = {1.0, 2.0};
        float[] floats = {1.0f, 2.0f};
        byte[] bytes = {1, 2};
        char[] chars = {'a', 'b'};
        short[] shorts = {1, 2};
        boolean[] booleans = {true, false};
        Object[] objects = {"a", null, "b"};
        return java.util.Arrays.hashCode(ints) + ":"
                + java.util.Arrays.hashCode(longs) + ":"
                + java.util.Arrays.hashCode(doubles) + ":"
                + java.util.Arrays.hashCode(floats) + ":"
                + java.util.Arrays.hashCode(bytes) + ":"
                + java.util.Arrays.hashCode(chars) + ":"
                + java.util.Arrays.hashCode(shorts) + ":"
                + java.util.Arrays.hashCode(booleans) + ":"
                + java.util.Arrays.hashCode(objects) + ":"
                + java.util.Arrays.hashCode((Object[]) null) + ":"
                + java.util.Arrays.hashCode(new int[0]);
    }

    public static String deep() {
        Object[] nested = {new int[] {1, 2}, new String[] {"x"}};
        return java.util.Arrays.deepEquals(nested, new Object[] {new int[] {1, 2}, new String[] {"x"}}) + ":"
                + java.util.Arrays.deepHashCode(nested) + ":"
                + java.util.Arrays.deepEquals(nested, new Object[] {new int[] {1, 3}, new String[] {"x"}});
    }
}
