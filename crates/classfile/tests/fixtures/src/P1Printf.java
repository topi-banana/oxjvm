package demo;

// Provenance for P1Printf.class — a regression fixture for PrintStream/PrintWriter
// printf/format.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1Printf.java
//     cp out/demo/P1Printf.class crates/classfile/tests/fixtures/
public class P1Printf {
    public static String formats() {
        java.io.StringWriter sw = new java.io.StringWriter();
        java.io.PrintWriter pw = new java.io.PrintWriter(sw);
        pw.printf("%s=%d", "x", 42);
        pw.format("/%x/%b/%c", 255, true, '!');
        return sw.toString();
    }
}
