package demo;

// Provenance for P1Latch.class — a regression fixture for
// java.util.concurrent.CountDownLatch and TimeUnit.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1Latch.java
//     cp out/demo/P1Latch.class 'out/demo/P1Latch$Worker.class' crates/classfile/tests/fixtures/
public class P1Latch {
    static class Worker implements Runnable {
        final java.util.concurrent.CountDownLatch latch;

        Worker(java.util.concurrent.CountDownLatch latch) {
            this.latch = latch;
        }

        public void run() {
            latch.countDown();
        }
    }

    public static String counts() throws Exception {
        java.util.concurrent.CountDownLatch latch = new java.util.concurrent.CountDownLatch(2);
        String initial = "" + latch.getCount();
        latch.countDown();
        latch.countDown();
        latch.await();
        return initial + ":" + latch.getCount();
    }

    public static String threads() throws Exception {
        java.util.concurrent.CountDownLatch latch = new java.util.concurrent.CountDownLatch(2);
        Thread a = new Thread(new Worker(latch));
        Thread b = new Thread(new Worker(latch));
        a.start();
        b.start();
        latch.await();
        a.join();
        b.join();
        return "done:" + latch.getCount();
    }

    public static String timeUnit() {
        long millis = java.util.concurrent.TimeUnit.SECONDS.toMillis(3);
        long nanos = java.util.concurrent.TimeUnit.MILLISECONDS.toNanos(2);
        String name = java.util.concurrent.TimeUnit.SECONDS.name();
        long converted = java.util.concurrent.TimeUnit.MINUTES.convert(
                120, java.util.concurrent.TimeUnit.SECONDS);
        return millis + ":" + nanos + ":" + name + ":" + converted;
    }
}
