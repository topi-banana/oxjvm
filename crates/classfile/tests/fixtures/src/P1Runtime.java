package demo;

// Provenance for P1Runtime.class — a regression fixture for Runtime.version() and
// java.lang.Runtime$Version.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1Runtime.java
//     cp out/demo/P1Runtime*.class crates/classfile/tests/fixtures/
public class P1Runtime {
    public static String versionBits() {
        Runtime.Version version = Runtime.version();
        return version.feature() + ":" + version.interim() + ":" + version.update() + ":"
                + version.patch() + ":" + version.pre().isPresent() + ":"
                + version.build().isPresent() + ":" + version;
    }

    public static String versionList() {
        java.util.List<Integer> components = Runtime.version().version();
        return components.size() + ":" + components.get(0) + ":" + components.get(3);
    }

    public static String compare() {
        Runtime.Version a = Runtime.version();
        Runtime.Version b = Runtime.version();
        return a.equals(b) + ":" + (a.compareTo(b) == 0) + ":" + (a.hashCode() == b.hashCode());
    }
}
