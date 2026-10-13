package demo;

// Provenance for P1Charset.class — a regression fixture for java.nio.charset.Charset /
// StandardCharsets and the Charset-based String codec methods.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1Charset.java
//     cp out/demo/P1Charset.class crates/classfile/tests/fixtures/
public class P1Charset {
    public static String charset() {
        java.nio.charset.Charset utf8 = java.nio.charset.Charset.forName("UTF-8");
        java.nio.charset.Charset latin = java.nio.charset.Charset.forName("ISO-8859-1");
        byte[] latinBytes = "café".getBytes(latin);
        String roundTrip = new String(latinBytes, latin);
        return (utf8 == java.nio.charset.StandardCharsets.UTF_8) + ":"
                + utf8.name() + ":" + java.nio.charset.Charset.defaultCharset().name() + ":"
                + java.nio.charset.Charset.isSupported("us-ascii") + ":"
                + java.nio.charset.Charset.isSupported("nope") + ":"
                + latinBytes.length + ":" + roundTrip + ":"
                + java.nio.charset.StandardCharsets.US_ASCII.name() + ":"
                + java.nio.charset.Charset.availableCharsets().keySet().contains("UTF-8");
    }
}
