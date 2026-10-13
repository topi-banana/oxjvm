package demo;

// Provenance for P1Path.class — a regression fixture for Path.of(String, String...).
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1Path.java
//     cp out/demo/P1Path.class crates/classfile/tests/fixtures/
public class P1Path {
    public static String of() {
        java.nio.file.Path path = java.nio.file.Path.of("a", "b", "c");
        java.nio.file.Path single = java.nio.file.Path.of("x/y");
        return path + ":" + path.getNameCount() + ":" + single.getFileName() + ":"
                + java.nio.file.Paths.get("a", "b");
    }
}
