package demo;

// Provenance for P2Regex.class — a regression fixture for the 2026-10-22 P1 batch:
// Matcher.group(String) has to find a named capturing group, an exact repetition count such as
// `\d{4}` has to be honoured (the parser used to drop it), and an unknown group name has to be
// reported the way the JDK reports it.
// Compiled with `-parameters -g --release 25` (class-file major 69):
//     javac -parameters -g --release 25 -d out crates/classfile/tests/fixtures/src/P2Regex.java
//     cp out/demo/P2Regex.class crates/classfile/tests/fixtures/
public class P2Regex {
    public static String namedGroups() {
        java.util.regex.Matcher matcher =
                java.util.regex.Pattern.compile("(?<year>\\d{4})-(?<month>\\d{2})").matcher("2026-10");
        boolean matched = matcher.matches();
        return matched + ":" + matcher.group("year") + ":" + matcher.group("month")
                + ":" + matcher.group(0) + ":" + matcher.group(2);
    }

    public static String repetitions() {
        return java.util.regex.Pattern.compile("\\d{4}").matcher("2026").matches() + ":"
                + java.util.regex.Pattern.compile("a{4}").matcher("aaaa").matches() + ":"
                + java.util.regex.Pattern.compile("[0-9]{2,4}").matcher("123").matches() + ":"
                + java.util.regex.Pattern.compile("\\d{4}").matcher("20").matches();
    }

    public static String unknownGroup() {
        java.util.regex.Matcher matcher =
                java.util.regex.Pattern.compile("(?<x>a)(b)").matcher("ab");
        matcher.matches();
        try {
            matcher.group("nope");
            return "none";
        } catch (IllegalArgumentException failure) {
            return failure.getMessage();
        }
    }

    public static void main(String[] args) {
        System.out.println("namedGroups=" + namedGroups());
        System.out.println("repetitions=" + repetitions());
        System.out.println("unknownGroup=" + unknownGroup());
    }
}
