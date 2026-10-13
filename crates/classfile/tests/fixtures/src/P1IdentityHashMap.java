package demo;

// Provenance for P1IdentityHashMap.class — a regression fixture for java.util.IdentityHashMap
// (key identity semantics: equal but distinct keys are separate entries).
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1IdentityHashMap.java
//     cp out/demo/P1IdentityHashMap.class crates/classfile/tests/fixtures/
public class P1IdentityHashMap {
    public static String identity() {
        java.util.IdentityHashMap<String, Integer> map = new java.util.IdentityHashMap<>();
        String a = new String("k");
        String b = new String("k");
        map.put(a, 1);
        map.put(b, 2);
        String before = map.size() + ":" + map.get(a) + ":" + map.get(b) + ":"
                + map.containsKey(new String("k")) + ":" + map.getOrDefault(a, 9) + ":"
                + map.containsValue(map.get(b));
        map.remove(a);
        return before + ":" + map.size() + ":" + map;
    }
}
