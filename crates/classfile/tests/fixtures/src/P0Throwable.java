package demo;

// Provenance for P0Throwable.class — regression fixture for Throwable(Throwable):
// the cause-only constructor must store its argument as the cause, not read past
// the argument array. Compiled with `-parameters -g --release 25` (major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P0Throwable.java
//     cp out/demo/P0Throwable.class crates/classfile/tests/fixtures/
public class P0Throwable {
    public static String causeMessage() {
        Throwable throwable = new RuntimeException(new IllegalStateException("boom"));
        return throwable.getCause().getMessage();
    }

    public static String causeType() {
        Throwable throwable = new Throwable(new RuntimeException("inner"));
        return throwable.getCause().getClass().getName();
    }

    public static String noCause() {
        return String.valueOf(new RuntimeException("plain").getCause());
    }
}