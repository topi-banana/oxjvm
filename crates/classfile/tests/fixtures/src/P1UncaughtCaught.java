package demo;

// Provenance for P1UncaughtCaught.class — regression fixture for the exit-code fix:
// the NoClassDefFoundError raised by an unresolvable catch type must be catchable
// by an outer handler.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1UncaughtCaught.java
//     cp out/demo/P1UncaughtCaught.class crates/classfile/tests/fixtures/
public class P1UncaughtCaught {
    public static void main(String[] args) {
        try {
            try {
                throw new RuntimeException("boom");
            } catch (java.lang.reflect.UndeclaredThrowableException e) {
                System.out.println("wrong catch");
            }
        } catch (NoClassDefFoundError e) {
            System.out.println("caught:" + e.getMessage());
        }
    }
}
