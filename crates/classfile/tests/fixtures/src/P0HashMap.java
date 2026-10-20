package demo;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

// Provenance for P0HashMap.class — regression fixture for HashMap table growth.
// A real HashMap resizes when the 13th entry is inserted into the default
// 16-bucket table, which changes keySet() order.  Compiled with
// `-parameters -g --release 25` (major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P0HashMap.java
//     cp out/demo/P0HashMap.class crates/classfile/tests/fixtures/
public class P0HashMap {
    public static String resize() {
        List<Object> oldOrder = new ArrayList<>();
        Map<Object, Object> map = new HashMap<>(16);
        int number = 0;
        while (number < 100000) {
            map.put(null, null);
            map.remove(null);
            Integer adding = (number += 100);
            map.put(adding, null);
            List<Object> newOrder = new ArrayList<>();
            newOrder.addAll(map.keySet());
            newOrder.remove(adding);
            if (!oldOrder.equals(newOrder)) {
                return "resized:" + (number / 100);
            }
            oldOrder.clear();
            oldOrder.addAll(map.keySet());
        }
        return "never";
    }
}