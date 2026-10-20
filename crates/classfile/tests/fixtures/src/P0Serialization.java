package demo;

import java.io.ObjectStreamClass;

// Provenance for P0Serialization.class — regression fixture for
// ObjectStreamClass.lookup on a class whose static initializer fails.  JDK
// lookup() initializes the target; a failed initializer must leave no partial
// descriptor.  Compiled with `-parameters -g --release 25` (major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P0Serialization.java
//     cp out/demo/P0Serialization*.class crates/classfile/tests/fixtures/
public class P0Serialization {
    static class Failing {
        private static final long serialVersionUID = 0L;

        static {
            if ("foo".equals("foo")) {
                throw new Error("uninitializable");
            }
        }
    }

    public static String lookup() {
        try {
            Class<?> type = Class.forName(
                    "demo.P0Serialization$Failing", false, P0Serialization.class.getClassLoader());
            ObjectStreamClass descriptor = null;
            try {
                descriptor = ObjectStreamClass.lookup(type);
            } catch (Throwable expected) {
            }
            try {
                descriptor = ObjectStreamClass.lookup(type);
            } catch (Throwable expected) {
            }
            return descriptor == null ? "null" : "descriptor";
        } catch (Throwable other) {
            return other.getClass().getName();
        }
    }
}