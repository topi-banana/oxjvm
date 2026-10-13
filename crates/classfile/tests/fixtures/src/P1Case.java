package demo;

// Provenance for P1Case.class — a regression fixture for String.toLowerCase(Locale) /
// String.toUpperCase(Locale), including the Turkish dotted/dotless I rules.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1Case.java
//     cp out/demo/P1Case.class crates/classfile/tests/fixtures/
public class P1Case {
    public static String cases() {
        return "ABC".toLowerCase(java.util.Locale.ROOT) + ":"
                + "abc".toUpperCase(java.util.Locale.US) + ":"
                + "I i İ ı".toLowerCase(java.util.Locale.forLanguageTag("tr")) + ":"
                + "iı".toUpperCase(java.util.Locale.forLanguageTag("tr"));
    }
}
