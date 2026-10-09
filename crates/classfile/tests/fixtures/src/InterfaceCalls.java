package demo;

// Provenance for InterfaceCalls.class and its interfaces — regression for the invokeinterface
// count operand: javac emits the receiver plus the argument slots (()I -> 1, (D)I -> 3,
// (JJ)J -> 5) and the verifier must accept all of them. Compiled with `-parameters -g --release 25`
// (class-file major 69) so the fixtures parse on VMs that cap at Java 26:
//     javac -parameters -g --release 25 -d out jals-classpath/tests/fixtures/src/InterfaceCalls.java
//     cp out/demo/InterfaceCalls.class out/demo/UnitCounter.class out/demo/DoubleScaler.class \
//        out/demo/LongSummer.class jals-classpath/tests/fixtures/

interface UnitCounter {
    int id();
}

interface DoubleScaler {
    double scale(double value);
}

interface LongSummer {
    long sum(long a, long b);
}

public class InterfaceCalls implements UnitCounter, DoubleScaler, LongSummer {
    @Override
    public int id() {
        return 7;
    }

    @Override
    public double scale(double value) {
        return value * 2.0;
    }

    @Override
    public long sum(long a, long b) {
        return a + b;
    }

    static int id(UnitCounter counter) {
        return counter.id();
    }

    static double twice(DoubleScaler scaler, double value) {
        return scaler.scale(value);
    }

    static long sum(LongSummer summer, long a, long b) {
        return summer.sum(a, b);
    }
}
