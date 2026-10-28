package demo;

import java.util.Arrays;
import java.util.HashSet;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.TreeMap;

/** Collection/array behaviours fixed in Phase 0; exercised by `run_fixtures.rs`. */
public class P0Arrays {
    /** `Set.toArray(T[])` must allocate the caller's array type. */
    public static String setToArrayType() {
        Set<String> set = new HashSet<>(Arrays.asList("a", "b"));
        return set.toArray(new String[0]).getClass().getName();
    }

    /** `Arrays.copyOf(Object[], int, Class)` must honour the Class argument. */
    public static String copyOfType() {
        Object[] objects = {"a", "b"};
        return Arrays.copyOf(objects, 3, String[].class).getClass().getName();
    }

    /** `Objects.deepEquals(Object, Object)` must exist and compare arrays by content. */
    public static boolean objectsDeepEquals() {
        return Objects.deepEquals(new int[] {1, 2}, new int[] {1, 2});
    }

    /** Comparing maps whose keys have incompatible orderings must return false, not throw. */
    public static boolean incompatibleMapKeys() {
        Map<Integer, String> first = new TreeMap<>();
        first.put(42, "answer");
        Map<String, Integer> second = new TreeMap<>();
        second.put("answer", 42);
        return !first.equals(second);
    }
}
