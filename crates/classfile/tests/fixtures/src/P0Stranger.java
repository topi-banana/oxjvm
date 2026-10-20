package demo;

// Provenance for P0Stranger.class — a class outside P0Nest's nest, used to
// prove that the nestmate rule does not open every private member.
// Compiled with P0Nest.java (class-file major 69).
public class P0Stranger {
    private String hidden() {
        return "hidden";
    }
}