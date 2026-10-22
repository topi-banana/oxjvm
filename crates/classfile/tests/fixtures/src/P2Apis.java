package demo;

// Provenance for P2Apis.class — a regression fixture for the 2026-10-22 P1 batch: the
// java.base APIs that were missing and that a guest program reaches for right away.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P2Apis.java
//     cp out/demo/P2Apis.class crates/classfile/tests/fixtures/
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.OutputStream;
import java.nio.ByteBuffer;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.EnumSet;
import java.util.List;
import java.util.Map;
import java.util.Properties;
import java.util.stream.Collectors;
import java.util.stream.Stream;

public class P2Apis {
    enum Day { MONDAY, TUESDAY, WEDNESDAY }

    public static String paths() {
        Path path = Paths.get("/a/b/c");
        return path.subpath(0, 1) + "|" + path.subpath(1, 3) + "|"
                + Paths.get("/a/b/c").resolveSibling("d") + "|"
                + Paths.get("/a/b/c").resolveSibling("/x/y") + "|"
                + Paths.get("c").resolveSibling("d");
    }

    public static String grouping() {
        Map<String, List<String>> grouped =
                Stream.of("aa", "ab", "bc").collect(Collectors.groupingBy(s -> s.substring(0, 1)));
        Map<String, Long> counts = Stream.of("aa", "ab", "bc")
                .collect(Collectors.groupingBy(s -> s.substring(0, 1), Collectors.counting()));
        return grouped + "|" + counts;
    }

    public static String bits() {
        return Integer.numberOfLeadingZeros(1) + ":" + Integer.numberOfLeadingZeros(256) + ":"
                + Integer.numberOfLeadingZeros(-1) + ":" + Long.numberOfLeadingZeros(1L) + ":"
                + Integer.numberOfTrailingZeros(8) + ":" + Integer.highestOneBit(0x0f) + ":"
                + Integer.lowestOneBit(0x0f) + ":" + Long.bitCount(255L);
    }

    public static String enums() {
        return EnumSet.complementOf(EnumSet.of(Day.MONDAY)) + ":"
                + EnumSet.complementOf(EnumSet.of(Day.MONDAY, Day.WEDNESDAY)) + ":"
                + EnumSet.complementOf(EnumSet.noneOf(Day.class));
    }

    public static String hex() {
        return Double.toHexString(1.0d) + ":" + Double.toHexString(0.5d) + ":"
                + Float.toHexString(1.0f) + ":" + Double.toHexString(1.5d) + ":"
                + Double.toHexString(0.0d) + ":" + Double.toHexString(-0.0d) + ":"
                + Double.toHexString(Double.NaN) + ":" + Double.toHexString(Double.POSITIVE_INFINITY);
    }

    public static String transfer() throws Exception {
        File file = File.createTempFile("oxjvm", ".bin");
        try (FileOutputStream out = new FileOutputStream(file)) {
            out.write("hello".getBytes());
        }
        try (FileInputStream in = new FileInputStream(file)) {
            ByteArrayOutputStream sink = new ByteArrayOutputStream();
            long copied = in.transferTo(sink);
            return copied + ":" + sink;
        }
    }

    public static String entries() {
        Map.Entry<String, Integer> entry = Map.entry("k", 7);
        return entry.getKey() + ":" + entry.getValue() + ":" + entry;
    }

    public static String properties() throws Exception {
        File file = File.createTempFile("oxjvm", ".properties");
        Properties properties = new Properties();
        properties.setProperty("a", "1");
        try (OutputStream out = new FileOutputStream(file)) {
            properties.store(out, "comment");
        }
        String text = new String(java.nio.file.Files.readAllBytes(file.toPath()));
        return text.contains("a=1") + ":" + text.contains("comment");
    }

    public static String chars() {
        ByteBuffer buffer = ByteBuffer.allocate(8);
        buffer.put((byte) 0).put((byte) 'a').put((byte) 0).put((byte) 'b');
        buffer.position(0);
        return buffer.asCharBuffer().length() + ":" + buffer.asCharBuffer().toString().trim();
    }

    public static String redirects() {
        ProcessBuilder.Redirect from = ProcessBuilder.Redirect.from(new File("/tmp/x"));
        ProcessBuilder.Redirect to = ProcessBuilder.Redirect.to(new File("/tmp/y"));
        return from.type() + ":" + from.file() + ":" + from + ":" + to.type() + ":" + to + ":"
                + ProcessBuilder.Redirect.INHERIT.type() + ":"
                + ProcessBuilder.Redirect.PIPE.type() + ":"
                + ProcessBuilder.Redirect.DISCARD.type();
    }

    public static void main(String[] args) throws Exception {
        System.out.println("paths=" + paths());
        System.out.println("grouping=" + grouping());
        System.out.println("bits=" + bits());
        System.out.println("enums=" + enums());
        System.out.println("hex=" + hex());
        System.out.println("transfer=" + transfer());
        System.out.println("entries=" + entries());
        System.out.println("properties=" + properties());
        System.out.println("chars=" + chars());
        System.out.println("redirects=" + redirects());
    }
}
