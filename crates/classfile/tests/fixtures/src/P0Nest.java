package demo;

import java.lang.reflect.Method;

// Provenance for P0Nest.class — regression fixture for nestmate reflection:
// private members of the same nest are accessible through Method.invoke,
// while a private member of an unrelated class is still rejected.
// Compiled with `-parameters -g --release 25` (major 69):
//     javac -parameters -g --release 25 -d out \
//         crates/classfile/tests/fixtures/src/P0Nest.java \
//         crates/classfile/tests/fixtures/src/P0Stranger.java
//     cp out/demo/P0Nest*.class out/demo/P0Stranger.class crates/classfile/tests/fixtures/
public class P0Nest {
    private String secret() {
        return "secret";
    }

    static class Inner {
        private String hidden() {
            return "hidden";
        }
    }

    public static String invokeOuter() throws Exception {
        Method method = P0Nest.class.getDeclaredMethod("secret");
        return (String) method.invoke(new P0Nest());
    }

    public static String invokeInner() throws Exception {
        Inner inner = new Inner();
        Method method = Inner.class.getDeclaredMethod("hidden");
        return (String) method.invoke(inner);
    }

    public static String denied() {
        try {
            Method method = P0Stranger.class.getDeclaredMethod("hidden");
            return (String) method.invoke(new P0Stranger());
        } catch (IllegalAccessException expected) {
            return "denied";
        } catch (ReflectiveOperationException other) {
            return other.getClass().getName();
        }
    }
}