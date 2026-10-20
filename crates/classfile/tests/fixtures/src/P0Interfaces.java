package demo;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.EnumSet;
import java.util.List;
import java.util.Set;
import java.util.function.Consumer;
import java.util.function.Function;

// Provenance for P0Interfaces.class — regression fixture for inherited
// interface methods: Function.andThen/compose, Collection/List/Set.forEach,
// and CharSequence.length/charAt on StringBuffer. Compiled with
// `-parameters -g --release 25` (major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P0Interfaces.java
//     cp out/demo/P0Interfaces*.class crates/classfile/tests/fixtures/
public class P0Interfaces {
    enum Color {
        RED, GREEN
    }

    public static String andThen() {
        Function<String, String> first = new Function<String, String>() {
            public String apply(String value) {
                return value + "a";
            }
        };
        Function<String, String> second = new Function<String, String>() {
            public String apply(String value) {
                return value + "b";
            }
        };
        return first.andThen(second).apply("x");
    }

    public static String compose() {
        Function<String, String> first = new Function<String, String>() {
            public String apply(String value) {
                return value + "a";
            }
        };
        Function<String, String> second = new Function<String, String>() {
            public String apply(String value) {
                return value + "b";
            }
        };
        return first.compose(second).apply("x");
    }

    public static String setForEach() {
        Set<Color> colors = EnumSet.of(Color.RED, Color.GREEN);
        StringBuilder out = new StringBuilder();
        colors.forEach(new Consumer<Color>() {
            public void accept(Color color) {
                out.append(color);
            }
        });
        return out.toString();
    }

    public static String listForEach() {
        List<String> values = new ArrayList<>(Arrays.asList("x", "y"));
        StringBuilder out = new StringBuilder();
        values.forEach(new Consumer<String>() {
            public void accept(String value) {
                out.append(value);
            }
        });
        return out.toString();
    }

    public static String bufferInterface() {
        CharSequence sequence = new StringBuffer("abc");
        return sequence.length() + ":" + sequence.charAt(1);
    }
}