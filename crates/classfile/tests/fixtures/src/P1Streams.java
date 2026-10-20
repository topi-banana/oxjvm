package demo;

import java.util.Arrays;
import java.util.OptionalDouble;
import java.util.function.DoublePredicate;
import java.util.function.DoubleUnaryOperator;
import java.util.function.LongUnaryOperator;
import java.util.function.ToIntFunction;
import java.util.stream.DoubleStream;
import java.util.stream.LongStream;
import java.util.stream.Stream;

// Provenance for P1Streams.class — DoubleStream/LongStream and Stream.mapTo*
// added in P1-b. Compiled with `-parameters -g --release 25` (major 69).
public class P1Streams {
    public static String doubles() {
        double sum = DoubleStream.of(1, 2, 3, 4)
                .map(new DoubleUnaryOperator() {
                    public double applyAsDouble(double value) {
                        return value * 2;
                    }
                })
                .filter(new DoublePredicate() {
                    public boolean test(double value) {
                        return value > 4;
                    }
                })
                .sum();
        OptionalDouble average = DoubleStream.of(2, 4).average();
        return sum + ":" + average.getAsDouble() + ":"
                + Arrays.stream(new double[] {1, 2, 3}).boxed().count();
    }

    public static String longs() {
        long sum = LongStream.rangeClosed(1, 4)
                .map(new LongUnaryOperator() {
                    public long applyAsLong(long value) {
                        return value * value;
                    }
                })
                .sum();
        return sum + ":" + LongStream.of(5, 6).max().getAsLong();
    }

    public static void main(String[] args) {
        System.out.println(doubles());
        System.out.println(longs());
        System.out.println(maps());
    }

    public static String maps() {
        int mapped = Stream.of("a", "bb", "ccc")
                .mapToInt(new ToIntFunction<String>() {
                    public int applyAsInt(String value) {
                        return value.length();
                    }
                })
                .sum();
        int iterated = Stream.iterate(1, value -> value + 1)
                .limit(5)
                .mapToInt(value -> value)
                .sum();
        return mapped + ":" + iterated;
    }
}
