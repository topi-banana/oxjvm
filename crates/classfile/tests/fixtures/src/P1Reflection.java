package demo;

import java.lang.reflect.Method;
import java.net.URL;

// Provenance for P1Reflection.class — Class/Parameter/URL APIs added in P1-a.
// Compiled with `-parameters -g --release 25` (major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1Reflection.java
//     cp out/demo/P1Reflection.class crates/classfile/tests/fixtures/
public class P1Reflection {
    public static void sample(String value) {
    }

    public static String parameter() throws Exception {
        Method method = P1Reflection.class.getDeclaredMethod("sample", String.class);
        return method.getParameters()[0].getDeclaringExecutable().getName();
    }

    public static String classApi() {
        return String.class.describeConstable().isPresent()
                + ":" + String.class.getTypeParameters().length
                + ":" + (String.class.getProtectionDomain() != null)
                + ":" + ((Class<?>) Integer.class.getGenericSuperclass()).getName();
    }

    public static String url() throws Exception {
        return new URL(new URL("file:/tmp/a/b.txt"), "c.txt").toString();
    }

    public static void main(String[] args) throws Exception {
        System.out.println("parameter=" + parameter());
        System.out.println("classApi=" + classApi());
        System.out.println("url=" + url());
    }
}