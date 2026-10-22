package demo;

// Provenance for P2Checks.class — a regression fixture for the 2026-10-22 P0 batch: the VM has to
// raise the JDK's exceptions instead of an internal error when a guest reaches a null reference
// or a reference that is not an array, and when a guest writes a final field outside its
// declaring class.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P2Checks.java
//     cp out/demo/P2Checks.class crates/classfile/tests/fixtures/
public class P2Checks {
    static String outcome(Runnable action) {
        try {
            action.run();
            return "none";
        } catch (Throwable failure) {
            return failure.getClass().getName();
        }
    }

    public static String arrayLength() {
        return outcome(() -> java.lang.reflect.Array.getLength(null)) + ":"
                + outcome(() -> java.lang.reflect.Array.getLength("not an array")) + ":"
                + outcome(() -> java.lang.reflect.Array.getLength(new int[] {1})) + ":"
                + outcome(() -> {
                    int[] values = {1, 2, 3};
                    System.out.println(values.length);
                });
    }

    public static void main(String[] args) {
        System.out.println("arrayLength=" + arrayLength());
    }
}
