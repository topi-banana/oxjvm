package demo;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.CharBuffer;
import java.nio.charset.Charset;
import java.nio.charset.StandardCharsets;

// Provenance for P1Buffer.class — java.nio buffers and charset codecs added in P1-b.
// Compiled with `-parameters -g --release 25` (major 69), then copied into
// crates/classfile/tests/fixtures/.
public class P1Buffer {
    public static String byteBuffer() {
        ByteBuffer buffer = ByteBuffer.allocate(8);
        buffer.putInt(0x01020304).putShort((short) 0x0506);
        buffer.flip();
        int first = buffer.getInt();
        int second = buffer.getShort() & 0xffff;
        ByteBuffer little = ByteBuffer.allocate(4).order(ByteOrder.LITTLE_ENDIAN);
        little.putInt(0x01020304);
        return Integer.toHexString(first) + ":" + Integer.toHexString(second)
                + ":" + Integer.toHexString(little.getInt(0));
    }

    public static String slices() {
        ByteBuffer buffer = ByteBuffer.wrap(new byte[] {1, 2, 3, 4, 5});
        buffer.position(1);
        buffer.limit(4);
        ByteBuffer slice = buffer.slice();
        int first = slice.get() & 0xff;
        int second = slice.get() & 0xff;
        ByteBuffer duplicate = buffer.duplicate();
        return first + ":" + second + ":" + duplicate.remaining() + ":" + duplicate.get(0);
    }

    public static String charset() {
        Charset utf8 = StandardCharsets.UTF_8;
        ByteBuffer encoded = utf8.encode("h\u00e9");
        int length = encoded.remaining();
        CharBuffer decoded = utf8.decode(encoded);
        return length + ":" + decoded.toString();
    }
}
