package demo;

// Provenance for P1Unsafe.class — a regression fixture for the sun.misc.Unsafe /
// jdk.internal.misc.Unsafe stubs (theUnsafe, array offsets, put/get, CAS, getAndAdd).
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1Unsafe.java
//     cp out/demo/P1Unsafe.class crates/classfile/tests/fixtures/
@SuppressWarnings("removal")
public class P1Unsafe {
    public static String unsafe() throws Exception {
        java.lang.reflect.Field field = Class.forName("sun.misc.Unsafe").getDeclaredField("theUnsafe");
        field.setAccessible(true);
        sun.misc.Unsafe unsafe = (sun.misc.Unsafe) field.get(null);
        int[] array = {1, 2, 3};
        long base = unsafe.arrayBaseOffset(int[].class);
        int scale = unsafe.arrayIndexScale(int[].class);
        unsafe.putInt(array, base + 2L * scale, 42);
        boolean cas = unsafe.compareAndSwapInt(array, base, 1, 7);
        int added = unsafe.getAndAddInt(array, base, 5);
        return array[2] + ":" + array[0] + ":" + unsafe.addressSize() + ":" + cas + ":"
                + added + ":" + unsafe.pageSize();
    }
}
