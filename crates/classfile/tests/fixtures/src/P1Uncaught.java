package demo;

// Provenance for P1Uncaught.class — regression fixture for the exit-code fix: an
// uncaught exception whose catch type cannot be resolved must still make the
// launcher exit non-zero.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1Uncaught.java
//     cp out/demo/P1Uncaught.class crates/classfile/tests/fixtures/
public class P1Uncaught {
    public static void main(String[] args) {
        try {
            throw new RuntimeException("boom");
        } catch (java.lang.reflect.UndeclaredThrowableException e) {
            System.out.println("wrong catch");
        }
    }
}
