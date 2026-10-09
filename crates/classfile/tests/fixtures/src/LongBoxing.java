package demo;

// Provenance for LongBoxing.class — exercises `Long.valueOf(String)`: decimal parsing with signs
// and the extremes, the boxed result's class and value, invalid input (NumberFormatException), and
// a null string (NumberFormatException, "Cannot parse null string"). Compiled with
// `-parameters -g --release 25` (class-file major 69) so the fixture parses on VMs that cap at
// Java 26:
//     javac -parameters -g --release 25 -d out jals-classpath/tests/fixtures/src/LongBoxing.java
//     cp out/demo/LongBoxing.class jals-classpath/tests/fixtures/
public class LongBoxing {
    // Auto-unboxing on return proves the value round-trips through the wrapper.
    public static long parse(String text) {
        return Long.valueOf(text);
    }

    // Returns the boxed object so tests can inspect its class and value.
    public static Long boxed(String text) {
        return Long.valueOf(text);
    }

    public static long invalid() {
        return Long.valueOf("not-a-number");
    }

    public static long nullText() {
        return Long.valueOf(null);
    }

    public static long parseNull() {
        return Long.parseLong(null);
    }
}
