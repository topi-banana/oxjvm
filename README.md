# oxjvm

A JVM (class-file runtime) written from scratch in pure Rust. No C, no FFI, and **zero
dependencies**: every crate in the workspace builds with `--offline` against an empty registry.

The runtime implements the JVM specification **1:1**: every structure of JVMS chapter 4 is parsed
and re-emitted byte-exactly, and every instruction of JVMS chapter 6 has its own interpreter arm
with the specified stack effects, numeric semantics, and exceptions.

```console
$ oxjvm run -cp build/classes Hello
Hello, world!
$ oxjvm disasm build/classes/Hello.class
main([Ljava/lang/String;)V
      0: getstatic        b2 00 07
      3: ldc              12 08
      5: invokevirtual    b6 00 0d
      8: return           b1
```

## Workspace

| Crate              | Description                                                                                    | `no_std` |
| ------------------ | ---------------------------------------------------------------------------------------------- | -------- |
| `oxjvm-classfile`  | Complete, byte-exact model of the `.class` file format (JVMS ch. 4), parse **and** write.       | yes      |
| `oxjvm-platform`   | The `Host` boundary (class bytes, output, clocks) plus pure-Rust DEFLATE and ZIP readers.       | yes      |
| `oxjvm-vm`         | Class loading/linking, resolution, verification, the interpreter, cooperative threads, GC.      | yes      |
| `oxjvm-java`       | The native `java.*` library: 118 classes (`Object`, `String`, `StringBuilder`, `System`, …).    | yes      |
| `oxjvm-wasm`       | In-memory wasm facade; the whole core builds for `wasm32-unknown-unknown`.                      | yes      |
| `oxjvm-cli`        | The command-line interface (`run`, `disasm`, `inspect`, `version`).                             | no (host)|

Everything except the CLI is `#![no_std]` + `alloc` and free of I/O: class bytes, output, clocks,
and process exit all pass through `oxjvm_platform::Host`. The VM never spawns an OS thread — Java
threads are scheduled cooperatively inside one host thread — so the same runtime runs on a
single-threaded wasm engine, and keeps working when the engine provides shared-memory threads.

## What is implemented

* **Class files** (JVMS ch. 4): every constant-pool tag, every attribute — `Code`, `StackMapTable`,
  `BootstrapMethods`, `Record`, `Module`, annotations, type annotations, `NestHost`, and the rest —
  with unknown attributes preserved verbatim. Reading and writing round-trips byte-for-byte, which
  the test suite asserts over a corpus of real `javac` output.
* **Linking** (JVMS ch. 5): loading, field layout, constant-pool resolution, member resolution,
  access-relevant error types, and lazy class initialization with `ExceptionInInitializerError`
  wrapping.
* **Verification** (JVMS 4.10): structural checks are exhaustive (instruction boundaries, branch
  targets, exception ranges, constant-pool kinds, `max_stack`); the dataflow pass tracks the
  operand stack and local categories. Reference types are merged into one category, so the
  verifier is intentionally less precise than HotSpot's type checker while still rejecting
  malformed bytecode before it reaches the interpreter. `jsr`/`ret` are rejected for class files at
  version 50+ exactly as HotSpot does.
* **Interpreter** (JVMS ch. 6): all opcodes, including `invokedynamic`,
  `tableswitch`/`lookupswitch`, `multianewarray`, monitors, and the category-2 stack discipline.
  Java numeric semantics are preserved deliberately: wrapping integer arithmetic, `MIN_VALUE / -1`,
  IEEE NaN comparison modes, saturating float-to-int conversions, and `%` on negatives.
* **Dynamic calls**: `StringConcatFactory` recipes are evaluated natively; `LambdaMetafactory`
  call sites synthesize a real implementing class at runtime, so lambdas capture, dispatch, and
  call through method handles correctly.
* **Threads**: cooperative scheduling with monitors, `wait`/`notify`, `sleep`, `yield`, `join`,
  `interrupt`, daemons, and `synchronized` methods. Deterministic; no host threads required.
* **GC**: precise mark-sweep over frames, statics, interned strings, class objects, and method
  handles.
* **Core library**: the `java.*` classes listed in `oxjvm-java` — strings, builders, boxed
  primitives, `Math`/`StrictMath` (including a correctly rounded `sqrt` and polynomial
  transcendentals), exceptions with stack traces, `System`, `Thread`, `Class`, `Runtime`,
  `Objects`, `Arrays`, `Random` (bit-compatible with `java.util.Random`), `ArrayList`, `HashMap`,
  `Collections`, method handles, and the functional interfaces.

## Known deviations

* Reflection is limited to `Class`/`StackTraceElement`/method handles; `java.lang.reflect`,
  serialization, NIO, regex (`String.matches` only handles literals/anchors), and `java.util`
  streams are not implemented. Missing classes surface as `NoClassDefFoundError`, exactly as the
  JVM reports them.
* `Reference` types are erased in the verifier (see above) and `StackOverflowError` is raised from
  a frame limit rather than a host stack overflow.
* `Object.wait` wakes on `notify`/timeout without modelling spurious wakeups; interruption during
  `sleep`/`wait` wakes the thread rather than throwing `InterruptedException` mid-park.
* Records' generated `equals`/`hashCode`/`toString` (the `ObjectMethods` bootstrap) are not
  synthesised; the bootstrap class exists so linking succeeds, and calling it raises
  `BootstrapMethodError`.

## Building and testing

```console
$ cargo test --workspace          # codec round-trips, interpreter fixtures, CLI integration
$ cargo run -p oxjvm-cli -- run -cp path/to/classes Main arg1 arg2
$ cargo build -p oxjvm-classfile -p oxjvm-platform -p oxjvm-vm -p oxjvm-java -p oxjvm-wasm \
      --target wasm32-unknown-unknown
```

The interpreter tests execute real `javac` (JDK 25) class files — loops, switches, arrays,
virtual/interface dispatch, string builders, int-carried booleans — and compare the results with
the Java sources they were compiled from, alongside generated classes that exercise
`System.out.println`, native exceptions, and exception-table dispatch.

## CI

`.github/workflows/ci.yml` runs:

* `rustfmt`, `taplo fmt --check`, `typos`, and `cargo machete` on every push and pull request.
* `cargo clippy --all-targets -- -D warnings` and `cargo nextest run --workspace --all-features`
  on **Linux, macOS, and Windows × x86_64 and arm64**.
* The **no_std** proof: clippy with `-D warnings` for the portable core on
  `wasm32-unknown-unknown`, where any `std` dependency fails the build.
* The **wasm** test run: the same class-file, zip, and interpreter tests executed as
  `wasm32-wasip1` inside wasmtime, driven by nextest's target-runner support.


## Design notes

* `Vm` is a value, not a global: construct one over a `Host`, run a main class, and drop it. The
  same class bytes can feed several VMs.
* The heap owns objects behind `ObjectRef(u32)`; null is `0`. Interpreter frames are ordinary
  vectors, which is what makes cooperative suspension (`Resume` actions) and GC roots simple.
* The class-file codec derives every count and length from the model on write, so an edited class
  file still serialises consistently.
* `oxjvm-java` classes are real runtime classes: the verifier, resolver, and dispatcher cannot tell
  them apart from loaded bytecode, except that their method bodies are Rust functions.

## License

MIT OR Apache-2.0.

