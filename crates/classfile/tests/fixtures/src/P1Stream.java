package demo;

// Provenance for P1Stream.class — a regression fixture for Arrays.stream(Object[]),
// Stream.of/empty, filter/map/sorted/skip/limit/count/toArray/allMatch/toList and the
// Comparable bridges on the boxed classes.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1Stream.java
//     cp out/demo/P1Stream.class crates/classfile/tests/fixtures/
public class P1Stream {
    public static String pipeline() {
        Object[] input = {"a", "bb", "ccc"};
        long count = java.util.Arrays.stream(input).filter(o -> ((String) o).length() > 1).count();
        Object[] mapped = java.util.Arrays.stream(input).map(o -> ((String) o).toUpperCase()).toArray();
        boolean all = java.util.stream.Stream.of("x", "y").allMatch(o -> ((String) o).length() == 1);
        String sorted = java.util.Arrays.stream(new Integer[]{3, 1, 2}).sorted().toList().toString();
        java.util.List<Object> list = java.util.Arrays.stream(input).skip(1).limit(1).toList();
        return count + ":" + mapped.length + ":" + mapped[2] + ":" + all + ":" + sorted + ":" + list;
    }
}
