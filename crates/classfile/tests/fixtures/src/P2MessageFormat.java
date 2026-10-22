package demo;

// Provenance for P2MessageFormat.class — a regression fixture for the 2026-10-22 P2 batch:
// java.text.MessageFormat reads a pattern, writes a message from it, writes the pattern back and
// reads a message back into its arguments.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P2MessageFormat.java
//     cp out/demo/P2MessageFormat.class crates/classfile/tests/fixtures/
import java.text.ChoiceFormat;
import java.text.DecimalFormat;
import java.text.MessageFormat;
import java.text.NumberFormat;
import java.util.Date;
import java.util.Locale;

public class P2MessageFormat {
    public static String patterns() {
        return new MessageFormat("There are {1} files on {2} at {3}").toPattern() + "|"
                + new MessageFormat("On {2}, there are {1} files, with {0,number,currency}.")
                        .toPattern() + "|"
                + new MessageFormat("'{1,number,percent}', {1,number,percent}, ").toPattern() + "|"
                + new MessageFormat("Quotes '', '{', 'a' {0} '{0}'").toPattern();
    }

    public static String formats() {
        Object[] arguments = {1D, 3456D, "Disk", new Date(0L)};
        return new MessageFormat("There are {1} files on {2}").format(arguments) + "|"
                + new MessageFormat("On {2}, there are {1} files, with {0,number,currency}.")
                        .format(arguments) + "|"
                + new MessageFormat("{1,number,percent}").format(arguments) + "|"
                + new MessageFormat("{1,number}").format(arguments) + "|"
                + new MessageFormat("{1,number,#,##}").format(arguments) + "|"
                + new MessageFormat("{0,number,integer}").format(arguments) + "|"
                + new MessageFormat("{3,date,full}").format(arguments);
    }

    public static String quoting() {
        // The quotes of a pattern: a doubled quote is a quote, a quoted brace is a brace.
        return MessageFormat.format("''{0}''", "x") + "|"
                + MessageFormat.format("'{'{0}'}'", "x") + "|"
                + MessageFormat.format("{0}", "y");
    }

    public static String choice() {
        ChoiceFormat choice = new ChoiceFormat("0#none|1#one");
        return choice.format(0.5) + ":" + choice.format(1.5) + ":" + choice.toPattern() + ":"
                + MessageFormat.format("{0,choice,0#no|1#yes}", 1);
    }

    public static String numbers() {
        NumberFormat number = NumberFormat.getInstance(Locale.US);
        DecimalFormat decimal = new DecimalFormat("#,##0.00");
        return number.format(3456.789d) + ":" + decimal.format(3456.789d) + ":"
                + NumberFormat.getIntegerInstance(Locale.US).format(3456.789d) + ":"
                + NumberFormat.getPercentInstance(Locale.US).format(0.25d) + ":"
                + NumberFormat.getCurrencyInstance(Locale.US).format(1234.5d);
    }

    public static String parsed() throws Exception {
        MessageFormat format = new MessageFormat("There are {1} files on {2} at {3}");
        Object[] values = format.parse("There are 3,456 files on Disk at 4/27/70, 2:46 AM");
        return values.length + ":" + values[1] + ":" + values[2];
    }

    public static String failures() {
        return failure(() -> new MessageFormat(null)) + ":"
                + failure(() -> MessageFormat.format("x {1,bogus}", new Object[] {"a"})) + ":"
                + failure(() -> new MessageFormat("{10000}"));
    }

    interface Action {
        void run() throws Exception;
    }

    static String failure(Action action) {
        try {
            action.run();
            return "none";
        } catch (Throwable failure) {
            return failure.getClass().getSimpleName();
        }
    }

    public static void main(String[] args) throws Exception {
        System.out.println("patterns=" + patterns());
        System.out.println("formats=" + formats());
        System.out.println("quoting=" + quoting());
        System.out.println("choice=" + choice());
        System.out.println("numbers=" + numbers());
        System.out.println("parsed=" + parsed());
        System.out.println("failures=" + failures());
    }
}
