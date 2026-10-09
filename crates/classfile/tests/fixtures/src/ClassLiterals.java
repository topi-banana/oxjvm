package demo;

// Provenance for ClassLiterals.class — regression for `ldc` of a class literal: the constant's
// entry holds the name index of a Utf8 entry, and the runtime must not re-dereference it as a
// class index. Covers a literal in <clinit>, in a method body, a JDK class, and an array class.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out jals-classpath/tests/fixtures/src/ClassLiterals.java
//     cp out/demo/ClassLiterals.class jals-classpath/tests/fixtures/
public class ClassLiterals {
    static final Class<?> SELF = ClassLiterals.class;

    public static String selfName() {
        return ClassLiterals.class.getName();
    }

    public static String staticFieldName() {
        return SELF.getName();
    }

    public static String stringName() {
        return String.class.getName();
    }

    public static String arrayName() {
        return int[].class.getName();
    }
}
