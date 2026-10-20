package demo;

import java.lang.ModuleLayer;
import java.lang.reflect.Array;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Locale;
import java.util.ResourceBundle;
import java.util.TimeZone;
import java.util.logging.LogManager;

// Provenance for P1Classes.class — missing classes, file/system/time/locale and
// logging APIs added in P1-a. Compiled with `-parameters -g --release 25`
// (major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P1Classes.java crates/classfile/tests/fixtures/src/P1Bundle.java
//     cp out/demo/P1Classes.class out/demo/P1Bundle.class crates/classfile/tests/fixtures/
public class P1Classes {
    public static String instantiation() throws Exception {
        return Class.forName("java.lang.InstantiationException").getSuperclass().getName();
    }

    public static String moduleLayer() {
        return String.valueOf(ModuleLayer.boot().findLoader("demo") != null);
    }

    public static String reflectArray() {
        Object array = Array.newInstance(int.class, 3);
        Array.setInt(array, 0, 7);
        return Array.getLength(array) + ":" + Array.getInt(array, 0);
    }

    public static String files() throws Exception {
        Path file = Files.createTempFile("p1a", ".txt");
        Files.writeString(file, "a\nb");
        String text = Files.readString(file);
        List<String> lines = Files.readAllLines(file, StandardCharsets.UTF_8);
        Files.delete(file);
        return text.replace("\n", "|") + ":" + lines.size();
    }

    public static String system() {
        return System.mapLibraryName("demo").contains("demo") + ":"
                + System.getLogger("demo.logger").getName();
    }

    public static String timeZone() {
        TimeZone.setDefault(TimeZone.getTimeZone("UTC"));
        return TimeZone.getDefault().getID() + ":" + TimeZone.getAvailableIDs().length;
    }

    public static String locale() {
        return String.valueOf(Locale.getAvailableLocales().length > 0);
    }

    public static String resourceBundle() {
        ResourceBundle bundle = ResourceBundle.getBundle("demo.P1Bundle");
        return bundle.getString("key");
    }

    public static String loggingMxBean() {
        return String.valueOf(LogManager.getLoggingMXBean().getLoggerNames().size());
    }
}