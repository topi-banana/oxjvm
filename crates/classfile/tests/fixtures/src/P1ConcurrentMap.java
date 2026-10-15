package demo;

// Provenance for P1ConcurrentMap.class — a regression fixture for
// java.util.concurrent.ConcurrentHashMap (+ ConcurrentMap / Map$Entry / newKeySet).
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1ConcurrentMap.java
//     cp out/demo/P1ConcurrentMap.class crates/classfile/tests/fixtures/
public class P1ConcurrentMap {
    public static String concurrent() {
        java.util.concurrent.ConcurrentMap<String, Integer> map =
                new java.util.concurrent.ConcurrentHashMap<>();
        map.put("a", 1);
        Integer prev = map.putIfAbsent("a", 2);
        int computed = map.computeIfAbsent("b", k -> 40);
        int merged = map.merge("b", 2, (a, b) -> a + b);
        map.computeIfPresent("a", (k, v) -> v + 10);
        boolean removed = map.remove("b", 42);
        java.util.Set<String> keys = java.util.concurrent.ConcurrentHashMap.newKeySet();
        keys.add("x");
        java.util.Map<String, Integer> copy = new java.util.concurrent.ConcurrentHashMap<>(map);
        StringBuilder entries = new StringBuilder();
        for (var e : map.entrySet()) {
            entries.append(e.getKey()).append('=').append(e.getValue()).append(';');
        }
        boolean replaced = map.replace("a", 11, 12);
        int def = map.getOrDefault("zz", -1);
        int[] seen = {0};
        map.forEach((k, v) -> seen[0] += v);
        return map.size() + ":" + prev + ":" + computed + ":" + merged + ":" + map.get("a")
                + ":" + removed + ":" + keys.size() + ":" + copy.get("a") + ":" + entries
                + ":" + replaced + ":" + def + ":" + seen[0] + ":" + map.get("a");
    }
}
