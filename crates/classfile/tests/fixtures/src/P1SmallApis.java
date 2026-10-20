package demo;

import java.math.BigDecimal;
import java.math.BigInteger;
import java.util.Arrays;
import java.util.Date;
import java.util.HexFormat;
import java.util.Map;
import java.util.Random;
import java.util.StringTokenizer;
import java.util.stream.Stream;

// Provenance for P1SmallApis.class — the small java.base APIs added in P1-a.
// Compiled with `-parameters -g --release 25` (major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1SmallApis.java
//     cp out/demo/P1SmallApis.class crates/classfile/tests/fixtures/
public class P1SmallApis {
    private static final StringBuilder CLOSED = new StringBuilder();

    public static String arraysStream() {
        int[] values = {1, 2, 3, 4};
        return String.valueOf(Arrays.stream(values).sum());
    }

    public static String mapOf() {
        Map<String, Integer> map = Map.of(
                "a", 1,
                "b", 2,
                "c", 3);
        Map<String, Integer> pair = Map.of("x", 7);
        return map.size() + ":" + map.get("b") + ":" + pair.get("x");
    }

    public static String streamClose() {
        CLOSED.setLength(0);
        Stream<String> stream = Stream.of("a");
        stream.onClose(new Runnable() {
            public void run() {
                CLOSED.append("closed");
            }
        });
        stream.close();
        return CLOSED.toString();
    }

    public static String random() {
        float value = new Random(1).nextFloat();
        return String.valueOf(value >= 0.0f && value < 1.0f);
    }

    public static String hexUpper() {
        return HexFormat.of().withUpperCase().formatHex(new byte[] {0x0a, (byte) 0xff});
    }

    public static String assertionError() {
        AssertionError text = new AssertionError((Object) "msg");
        AssertionError cause = new AssertionError(new RuntimeException("inner"));
        return text.getMessage() + ":" + (cause.getCause() != null);
    }

    public static String stringBufferLength() {
        StringBuffer buffer = new StringBuffer("abcdef");
        buffer.setLength(3);
        return buffer.toString();
    }

    public static String date() {
        return String.valueOf(new Date(70, 0, 1).getTime());
    }

    public static String numbers() {
        return Integer.bitCount(7) + ":" + Integer.divideUnsigned(-1, 2)
                + ":" + Character.toChars(0x1F600).length
                + ":" + Double.isNaN(Double.NaN);
    }

    public static String arraysCopies() {
        int[] source = {1, 2, 3};
        int[] copy = Arrays.copyOf(source, 5);
        byte[] left = {1, 2};
        byte[] right = {1, 3};
        byte[] range = Arrays.copyOfRange(new byte[] {1, 2, 3}, 1, 3);
        return copy.length + ":" + copy[2]
                + ":" + Arrays.equals(left, right)
                + ":" + Arrays.mismatch(left, right)
                + ":" + range.length + ":" + range[1];
    }

    public static String bigNumbers() {
        BigInteger hex = new BigInteger("ff", 16);
        BigInteger negative = new BigInteger(new byte[] {(byte) 0xff});
        BigInteger power = BigInteger.valueOf(10).pow(2);
        BigInteger shifted = BigInteger.valueOf(10).shiftLeft(2);
        BigInteger divided = BigInteger.valueOf(10).divide(BigInteger.valueOf(2));
        BigDecimal decimal = BigDecimal.valueOf(12345, 2);
        BigDecimal scaled = decimal.scaleByPowerOfTen(2);
        return hex + ":" + negative + ":" + power + ":" + shifted + ":" + divided
                + ":" + decimal.toPlainString() + ":" + scaled.toPlainString()
                + ":" + decimal.precision() + ":" + decimal.negate().toPlainString()
                + ":" + BigInteger.valueOf(13).isProbablePrime(10)
                + ":" + BigInteger.valueOf(15).isProbablePrime(10);
    }

    public static String tokenizer() {
        StringTokenizer commas = new StringTokenizer("a,b,c", ",");
        StringTokenizer chars = new StringTokenizer("a b", " ", true);
        return commas.countTokens() + ":" + commas.nextToken() + ":"
                + commas.nextToken() + ":" + commas.nextToken() + ":"
                + chars.countTokens() + ":" + chars.nextToken() + ":"
                + "[" + chars.nextToken() + "]";
    }
}