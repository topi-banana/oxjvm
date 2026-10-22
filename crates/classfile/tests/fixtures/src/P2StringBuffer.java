package demo;

// Provenance for P2StringBuffer.class — a regression fixture for the 2026-10-22 P1 batch:
// java.lang.StringBuffer used to implement only two of the append overloads, so a guest that
// appended an int or a long died with NoSuchMethodError.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P2StringBuffer.java
//     cp out/demo/P2StringBuffer.class crates/classfile/tests/fixtures/
public class P2StringBuffer {
    public static String appends() {
        StringBuffer buffer = new StringBuffer();
        buffer.append(1).append("x").append('c').append(true).append(2L).append(1.5f)
                .append(2.5d).append((Object) "o").append(new char[] {'a', 'b'});
        return buffer + ":" + buffer.length() + ":" + buffer.charAt(0);
    }

    public static String edits() {
        StringBuffer buffer = new StringBuffer("hello");
        buffer.insert(0, ">").deleteCharAt(0);
        return buffer + ":" + buffer.reverse() + ":" + buffer.indexOf("l") + ":"
                + new StringBuffer("abc").replace(0, 2, "XY");
    }

    public static void main(String[] args) {
        System.out.println("appends=" + appends());
        System.out.println("edits=" + edits());
    }
}
