package demo;

import java.util.concurrent.Callable;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.ThreadLocalRandom;

// Provenance for P1Concurrent.class — Executors/Future and Thread additions in P1-b.
// Compiled with `-parameters -g --release 25` (major 69).
public class P1Concurrent {
    public static String executor() throws Exception {
        ExecutorService service = Executors.newFixedThreadPool(1);
        Future<Integer> callable = service.submit(new Callable<Integer>() {
            public Integer call() {
                return 42;
            }
        });
        Future<String> runnable = service.submit(new Runnable() {
            public void run() {
            }
        }, "done");
        int value = callable.get();
        String text = runnable.get();
        service.shutdown();
        return value + ":" + text + ":" + service.isShutdown();
    }

    public static String thread() {
        Thread current = Thread.currentThread();
        int random = ThreadLocalRandom.current().nextInt(10);
        return current.getThreadGroup().getName()
                + ":" + (current.threadId() > 0)
                + ":" + (current.getState() == Thread.State.RUNNABLE)
                + ":" + (current.getStackTrace().length >= 0)
                + ":" + (random >= 0 && random < 10);
    }
}
