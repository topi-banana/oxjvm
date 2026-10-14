package demo;

// Provenance for P1Chars.class — a regression fixture for CharSequence.chars/codePoints
// and the eager IntStream (filter/forEach/map/count/sum/boxed/toArray/mapToObj).
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1Chars.java
//     cp out/demo/P1Chars.class crates/classfile/tests/fixtures/
public class P1Chars {
    public static String chars() {
        StringBuilder collected = new StringBuilder();
        "a1b2".chars().filter(c -> c != '1' && c != '2').forEach(c -> collected.append((char) c));
        long count = "hello".chars().count();
        int sum = "ABC".chars().map(c -> c - 'A').sum();
        Object[] boxed = "xy".chars().boxed().toArray();
        int[] array = "12".codePoints().toArray();
        java.util.List<String> mapped = "ab".chars().mapToObj(c -> "[" + c + "]").toList();
        return collected + ":" + count + ":" + sum + ":" + boxed.length + ":" + boxed[1] + ":"
                + array[0] + array[1] + ":" + mapped.get(1);
    }
}
