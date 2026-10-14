package demo;

// Provenance for P1Legacy.class — a regression fixture for HashSet, Hashtable,
// Vector and Enumeration.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1Legacy.java
//     cp out/demo/P1Legacy.class crates/classfile/tests/fixtures/
public class P1Legacy {
    public static String legacy() {
        java.util.HashSet<String> set = new java.util.HashSet<>();
        boolean first = set.add("x");
        boolean second = set.add("x");
        set.addAll(java.util.Arrays.asList("y", "z"));
        java.util.Hashtable<String, Integer> table = new java.util.Hashtable<>();
        table.put("a", 1);
        table.put("b", 2);
        java.util.Enumeration<String> keys = table.keys();
        int count = 0;
        while (keys.hasMoreElements()) {
            keys.nextElement();
            count++;
        }
        java.util.Vector<Integer> vector = new java.util.Vector<>();
        vector.addElement(1);
        vector.addElement(2);
        vector.insertElementAt(0, 0);
        return first + ":" + second + ":" + set.size() + ":" + set.contains("y") + ":"
                + table.size() + ":" + table.get("b") + ":" + count + ":" + vector.firstElement()
                + vector.lastElement() + ":" + vector.elementAt(1) + ":" + set;
    }
}
