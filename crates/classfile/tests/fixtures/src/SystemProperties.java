package demo;

// Provenance for SystemProperties.class — exercises system-property semantics: the two-argument
// `System.getProperty` overload with its null paths, and `Boolean.getBoolean`/`Boolean.parseBoolean`,
// which interpret a property value as a boolean. Compiled with `-parameters -g --release 25`
// (class-file major 69) so the fixture parses on VMs that cap at Java 26:
//     javac -parameters -g --release 25 -d out jals-classpath/tests/fixtures/src/SystemProperties.java
//     cp out/demo/SystemProperties.class jals-classpath/tests/fixtures/
public class SystemProperties {
    // A property the VM defines: the supplied default must not be used.
    public static String vmName() {
        return System.getProperty("java.vm.name", "fallback");
    }

    // A property the VM does not define: the supplied default is returned.
    public static String missingWithDefault() {
        return System.getProperty("oxjvm.no.such.property", "fallback");
    }

    // A missing property with a null default returns null.
    public static String missingWithNullDefault() {
        return System.getProperty("oxjvm.no.such.property", null);
    }

    // The one-argument overload returns null for a missing property.
    public static String missing() {
        return System.getProperty("oxjvm.no.such.property");
    }

    // A null key throws NullPointerException in both overloads.
    public static String nullKey() {
        return System.getProperty(null, "fallback");
    }

    // `Boolean.getBoolean` parses the system property with this name.
    public static boolean getBoolean(String key) {
        return Boolean.getBoolean(key);
    }

    // A null key throws NullPointerException, just like System.getProperty.
    public static boolean getBooleanNullKey() {
        return Boolean.getBoolean(null);
    }

    // `Boolean.parseBoolean` is case-insensitive and accepts null (false).
    public static boolean parseBoolean(String value) {
        return Boolean.parseBoolean(value);
    }
}
