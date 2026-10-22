package demo;

// Provenance for P2Process.class — a regression fixture for the 2026-10-22 P2 batch: a guest
// program starts another program and reads what it produced.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P2Process.java
//     cp out/demo/P2Process.class crates/classfile/tests/fixtures/
import java.io.InputStream;

public class P2Process {
    // The command that prints a line, spelled the way the host spells it.
    static String[] echoCommand() {
        String os = System.getProperty("os.name").toLowerCase();
        if (os.contains("win")) {
            return new String[] {"cmd", "/c", "echo hello"};
        }
        return new String[] {"sh", "-c", "echo hello"};
    }

    public static String started() throws Exception {
        Process process = new ProcessBuilder(echoCommand()).start();
        InputStream stream = process.getInputStream();
        String output = new String(stream.readAllBytes()).trim();
        return output + ":" + process.waitFor();
    }

    public static String executed() throws Exception {
        Process process = Runtime.getRuntime().exec(echoCommand());
        String output = new String(process.getInputStream().readAllBytes()).trim();
        return output + ":" + process.waitFor();
    }

    public static String handles() {
        ProcessHandle current = ProcessHandle.current();
        return current.pid() + ":" + current.isAlive() + ":"
                + ProcessHandle.allProcesses().count();
    }

    public static void main(String[] args) throws Exception {
        System.out.println("started=" + started());
        System.out.println("executed=" + executed());
        System.out.println("handles=" + handles());
    }
}
