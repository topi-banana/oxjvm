package demo;

// Provenance for P2Files.class — a regression fixture for the 2026-10-22 P2 batch:
// java.nio.file.Files.copy(Path, OutputStream) and the files methods around it.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P2Files.java
//     cp out/demo/P2Files.class crates/classfile/tests/fixtures/
import java.io.BufferedReader;
import java.io.ByteArrayOutputStream;
import java.nio.file.Files;
import java.nio.file.Path;

public class P2Files {
    public static String copy() throws Exception {
        Path file = Files.createTempFile("oxjvm", ".bin");
        Files.write(file, "hello world".getBytes());
        ByteArrayOutputStream sink = new ByteArrayOutputStream();
        long copied = Files.copy(file, sink);
        return copied + ":" + sink;
    }

    public static String lines() throws Exception {
        Path file = Files.createTempFile("oxjvm", ".txt");
        Files.writeString(file, "one\ntwo\nthree");
        BufferedReader reader = Files.newBufferedReader(file);
        StringBuilder text = new StringBuilder();
        String line;
        while ((line = reader.readLine()) != null) {
            text.append(line).append('|');
        }
        return Files.readAllLines(file) + ":" + text;
    }

    public static String compare() throws Exception {
        Path first = Files.createTempFile("oxjvm", ".txt");
        Path second = Files.createTempFile("oxjvm", ".txt");
        Files.writeString(first, "one\ntwo");
        Files.writeString(second, "one\ntwo");
        Path third = Files.createTempFile("oxjvm", ".txt");
        Files.writeString(third, "one\nthree");
        return Files.mismatch(first, second) + ":" + Files.mismatch(first, third) + ":"
                + Files.isSameFile(first, first);
    }

    public static void main(String[] args) throws Exception {
        System.out.println("copy=" + copy());
        System.out.println("lines=" + lines());
        System.out.println("compare=" + compare());
    }
}
