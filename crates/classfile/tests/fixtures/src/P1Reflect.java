package demo;

// Provenance for P1Reflect.class — a regression fixture for the minimal reflection:
// Class.getDeclaredMethod/getDeclaredField, Method.invoke/getName/getReturnType and
// Field.get/set/getInt/setInt/static access.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1Reflect.java
//     cp out/demo/P1Reflect.class crates/classfile/tests/fixtures/
public class P1Reflect {
    public static int staticInt = 7;
    public int instanceInt = 3;

    public int add(int value) {
        return value + instanceInt;
    }

    public static String reflect() throws Exception {
        java.lang.reflect.Method add = P1Reflect.class.getDeclaredMethod("add", int.class);
        java.lang.reflect.Field instance = P1Reflect.class.getDeclaredField("instanceInt");
        java.lang.reflect.Field statik = P1Reflect.class.getDeclaredField("staticInt");
        statik.setInt(null, 9);
        P1Reflect target = new P1Reflect();
        instance.setInt(target, 4);
        Object invoked = add.invoke(target, 5);
        boolean isStatic = (statik.getModifiers() & 0x8) != 0;
        return invoked + ":" + P1Reflect.staticInt + ":" + target.instanceInt + ":" + add.getName()
                + ":" + add.getReturnType().getName() + ":" + statik.getName() + ":" + isStatic;
    }
}
