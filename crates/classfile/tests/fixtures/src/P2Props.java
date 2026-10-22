package demo;

// Provenance for P2Props.class — a regression fixture for the 2026-10-22 P0 batch: JEP 223
// pins the platform system properties, so a `-D` override that tries to replace one of them
// has to be dropped while an ordinary property is applied.  The guest may still change a
// pinned property through System.setProperty, exactly as on the JDK.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P2Props.java
//     cp out/demo/P2Props.class crates/classfile/tests/fixtures/
public class P2Props {
    public static String pinned() {
        return System.getProperty("java.vm.name") + ":" + System.getProperty("java.vm.vendor")
                + ":" + System.getProperty("java.vendor") + ":"
                + System.getProperty("java.specification.name");
    }

    public static String plain() {
        return System.getProperty("oxjvm.test.property", "unset");
    }

    public static String setByGuest() {
        System.setProperty("java.vm.name", "guest");
        return System.getProperty("java.vm.name");
    }

    public static void main(String[] args) {
        System.out.println("pinned=" + pinned());
        System.out.println("plain=" + plain());
        System.out.println("setByGuest=" + setByGuest());
    }
}
