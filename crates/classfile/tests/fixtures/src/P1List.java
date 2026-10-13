package demo;

// Provenance for P1List.class — a regression fixture for List.addAll(Collection) and
// List.addAll(int, Collection) on ArrayList and LinkedList.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1List.java
//     cp out/demo/P1List.class crates/classfile/tests/fixtures/
public class P1List {
    public static String addAll() {
        java.util.ArrayList<String> list = new java.util.ArrayList<>();
        list.add("a");
        boolean changed = list.addAll(java.util.Arrays.asList("b", "c"));
        boolean empty = list.addAll(java.util.Arrays.asList());
        list.addAll(1, java.util.Arrays.asList("x"));
        java.util.LinkedList<String> linked = new java.util.LinkedList<>();
        linked.addAll(java.util.Arrays.asList("q"));
        return changed + ":" + empty + ":" + list.size() + ":" + list.get(1) + list.get(3)
                + ":" + linked.get(0);
    }
}
