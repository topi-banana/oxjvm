//! The oxjvm command-line interface: `run`, `disasm`, `inspect`, and `version`.
//!
//! This is the only crate in the workspace that uses `std`; everything below it is `no_std` and
//! talks to the outside world exclusively through [`oxjvm_platform::Host`].

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use oxjvm_classfile::attribute::AttributeData;
use oxjvm_classfile::{ClassFile, opcode};
use oxjvm_platform::{Host, HostError, ProcessInfo, ProcessResult, Stream};
use oxjvm_vm::{Vm, VmError, VmOptions};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("run") => run(&args[1..]),
        Some("disasm") => disasm(&args[1..]),
        Some("inspect") => inspect(&args[1..]),
        Some("version") | Some("--version") | Some("-V") => {
            println!("oxjvm {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("help") | Some("--help") | Some("-h") | None => {
            usage();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("oxjvm: unknown command `{other}`");
            usage();
            ExitCode::FAILURE
        }
    }
}

fn usage() {
    println!(
        "oxjvm {}\n\
         Usage:\n\
         \x20 oxjvm run [-cp <path>] [-D name=value]... [-Xms <size>] [-Xmx <size>]\n\
         \x20\x20\x20\x20\x20\x20\x20\x20 [-Xss <size>] <MainClass> [args...]\n\
         \x20 oxjvm disasm <file.class>\n\
         \x20 oxjvm inspect <file.class>\n\
         \x20 oxjvm version\n\
         \n\
         Memory sizes accept a k, m, or g suffix (for example -Xmx256m).",
        env!("CARGO_PKG_VERSION")
    );
}

/// A host that loads classes from `-cp` directories and jars and writes to the real streams.
struct PassthroughFileSystem;

impl PassthroughFileSystem {
    fn path(path: &str) -> PathBuf {
        PathBuf::from(path)
    }
}

impl oxjvm_platform::FileSystem for PassthroughFileSystem {
    fn metadata(&mut self, path: &str) -> Option<oxjvm_platform::FileMetadata> {
        let metadata = fs::metadata(Self::path(path)).ok()?;
        let kind = if metadata.is_dir() {
            oxjvm_platform::FileKind::Directory
        } else if metadata.is_file() {
            oxjvm_platform::FileKind::File
        } else {
            oxjvm_platform::FileKind::Other
        };
        let modified_millis = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(0);
        Some(oxjvm_platform::FileMetadata {
            kind,
            len: metadata.len(),
            modified_millis,
            readonly: metadata.permissions().readonly(),
        })
    }

    fn read(&mut self, path: &str) -> Option<Vec<u8>> {
        fs::read(Self::path(path)).ok()
    }

    fn write(&mut self, path: &str, bytes: &[u8], append: bool) -> bool {
        if append {
            use std::io::Write;
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(Self::path(path))
                .and_then(|mut file| file.write_all(bytes))
                .is_ok()
        } else {
            fs::write(Self::path(path), bytes).is_ok()
        }
    }

    fn list(&mut self, path: &str) -> Option<Vec<String>> {
        let entries = fs::read_dir(Self::path(path)).ok()?;
        let mut names = Vec::new();
        for entry in entries.flatten() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
        names.sort();
        Some(names)
    }

    fn create_dir(&mut self, path: &str) -> bool {
        fs::create_dir(Self::path(path)).is_ok()
    }

    fn create_dir_all(&mut self, path: &str) -> bool {
        fs::create_dir_all(Self::path(path)).is_ok()
    }

    fn remove_file(&mut self, path: &str) -> bool {
        fs::remove_file(Self::path(path)).is_ok()
    }

    fn remove_dir(&mut self, path: &str) -> bool {
        fs::remove_dir(Self::path(path)).is_ok()
    }

    fn rename(&mut self, from: &str, to: &str) -> bool {
        fs::rename(Self::path(from), Self::path(to)).is_ok()
    }

    fn canonicalize(&mut self, path: &str) -> Option<String> {
        fs::canonicalize(Self::path(path))
            .ok()
            .map(|path| path.display().to_string())
    }

    fn set_modified(&mut self, path: &str, millis: i64) -> bool {
        let time = if millis >= 0 {
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(millis as u64)
        } else {
            std::time::UNIX_EPOCH - std::time::Duration::from_millis((-millis) as u64)
        };
        fs::File::open(Self::path(path))
            .and_then(|file| file.set_modified(time))
            .is_ok()
    }

    fn symlink(&mut self, link: &str, target: &str) -> bool {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(Self::path(target), Self::path(link)).is_ok()
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(Self::path(target), Self::path(link)).is_ok()
                || std::os::windows::fs::symlink_dir(Self::path(target), Self::path(link)).is_ok()
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (link, target);
            false
        }
    }

    fn read_link(&mut self, path: &str) -> Option<String> {
        fs::read_link(Self::path(path))
            .ok()
            .map(|path| path.display().to_string())
    }

    fn hard_link(&mut self, link: &str, target: &str) -> bool {
        fs::hard_link(Self::path(target), Self::path(link)).is_ok()
    }
}

struct StdHost {
    classpath: Vec<PathBuf>,
    properties: BTreeMap<String, String>,
    fs: PassthroughFileSystem,
}

impl StdHost {
    fn new(classpath: Vec<PathBuf>, overrides: &[(String, String)]) -> Self {
        let mut properties = BTreeMap::new();
        properties.insert("java.version".into(), "17".into());
        properties.insert("java.vm.name".into(), "oxjvm".into());
        properties.insert("java.vm.version".into(), env!("CARGO_PKG_VERSION").into());
        properties.insert("os.name".into(), std::env::consts::OS.into());
        properties.insert("os.arch".into(), std::env::consts::ARCH.into());
        properties.insert("line.separator".into(), "\n".into());
        // The VM's path helpers only understand "/", so `file.separator` stays the VM default
        // rather than the platform's own separator.
        //
        // The temporary directory, on the other hand, has to be the platform's: the VM's default
        // is "/tmp", which does not exist on Windows, and `Files.createTempFile` would fail there.
        properties.insert(
            "java.io.tmpdir".into(),
            std::env::temp_dir().display().to_string(),
        );
        properties.insert(
            "user.dir".into(),
            std::env::current_dir()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
        );
        properties.insert(
            "java.class.path".into(),
            std::env::join_paths(&classpath)
                .map(|paths| paths.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        for (key, value) in overrides {
            // JEP 223 pins the platform properties to the values the platform reports, so a
            // `-D` that tries to replace one of them is dropped instead of applied.
            if oxjvm_java::is_non_writable_property(key) {
                continue;
            }
            properties.insert(key.clone(), value.clone());
        }
        Self {
            classpath,
            properties,
            fs: PassthroughFileSystem,
        }
    }

    fn load_from_path(&self, internal_name: &str, path: &Path) -> Option<Vec<u8>> {
        if path.is_dir() {
            let candidate = path.join(format!("{internal_name}.class"));
            return fs::read(candidate).ok();
        }
        if path.extension().is_some_and(|extension| extension == "jar") {
            let bytes = fs::read(path).ok()?;
            let archive = oxjvm_platform::zip::ZipArchive::new(&bytes).ok()?;
            let entry = format!("{internal_name}.class");
            return archive.read(&entry).ok().flatten();
        }
        None
    }
}

impl Host for StdHost {
    fn load_class(&mut self, internal_name: &str) -> Option<Vec<u8>> {
        for path in &self.classpath {
            if let Some(bytes) = self.load_from_path(internal_name, path) {
                return Some(bytes);
            }
        }
        None
    }

    fn filesystem(&mut self) -> Option<&mut dyn oxjvm_platform::FileSystem> {
        Some(&mut self.fs)
    }

    fn run_process(
        &mut self,
        program: &str,
        args: &[String],
        directory: Option<&str>,
    ) -> Result<ProcessResult, HostError> {
        let mut command = std::process::Command::new(program);
        command.args(args);
        if let Some(directory) = directory {
            command.current_dir(directory);
        }
        let output = command
            .output()
            .map_err(|error| HostError::Io(format!("cannot run {program}: {error}")))?;
        Ok(ProcessResult {
            pid: 0,
            status: output.status.code().unwrap_or(-1),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }

    fn processes(&mut self) -> Vec<ProcessInfo> {
        vec![ProcessInfo {
            pid: std::process::id(),
            command: std::env::args().collect::<Vec<String>>().join(" "),
        }]
    }

    fn current_process_id(&mut self) -> u32 {
        std::process::id()
    }

    fn load_resource(&mut self, resource: &str) -> Option<Vec<u8>> {
        for path in &self.classpath {
            if path.is_dir() {
                let candidate = path.join(resource);
                if let Ok(bytes) = fs::read(candidate) {
                    return Some(bytes);
                }
            } else if path.extension().is_some_and(|extension| extension == "jar") {
                let bytes = fs::read(path).ok()?;
                let archive = oxjvm_platform::zip::ZipArchive::new(&bytes).ok()?;
                if let Ok(Some(entry)) = archive.read(resource) {
                    return Some(entry);
                }
            }
        }
        None
    }

    fn find_resources(&mut self, resource: &str) -> Vec<oxjvm_platform::ResourceHit> {
        let mut hits = Vec::new();
        for path in &self.classpath {
            if path.is_dir() {
                let candidate = path.join(resource);
                let absolute = fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
                if let Ok(bytes) = fs::read(&candidate) {
                    let url = format!("file:{}", absolute.display());
                    hits.push(oxjvm_platform::ResourceHit { url, bytes });
                } else if candidate.is_dir() {
                    // Class-path scanners ask for directories; the JDK names them with a
                    // trailing slash and no content.
                    let url = format!("file:{}/", absolute.display());
                    hits.push(oxjvm_platform::ResourceHit {
                        url,
                        bytes: Vec::new(),
                    });
                }
            } else if path.extension().is_some_and(|extension| extension == "jar") {
                let Ok(bytes) = fs::read(path) else {
                    continue;
                };
                let Ok(archive) = oxjvm_platform::zip::ZipArchive::new(&bytes) else {
                    continue;
                };
                if let Ok(Some(entry)) = archive.read(resource) {
                    let absolute = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
                    let url = format!("jar:file:{}!/{resource}", absolute.display());
                    hits.push(oxjvm_platform::ResourceHit { url, bytes: entry });
                }
            }
        }
        hits
    }

    fn write(&mut self, stream: Stream, bytes: &[u8]) -> Result<(), HostError> {
        match stream {
            Stream::Stdout => {
                let mut stdout = std::io::stdout();
                stdout
                    .write_all(bytes)
                    .map_err(|error| HostError::Io(error.to_string()))?;
            }
            Stream::Stderr => {
                let mut stderr = std::io::stderr();
                stderr
                    .write_all(bytes)
                    .map_err(|error| HostError::Io(error.to_string()))?;
            }
        }
        Ok(())
    }

    fn flush(&mut self, stream: Stream) -> Result<(), HostError> {
        match stream {
            Stream::Stdout => std::io::stdout()
                .flush()
                .map_err(|error| HostError::Io(error.to_string())),
            Stream::Stderr => std::io::stderr()
                .flush()
                .map_err(|error| HostError::Io(error.to_string())),
        }
    }

    fn current_time_millis(&mut self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(0)
    }

    fn nano_time(&mut self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as i64)
            .unwrap_or(0)
    }

    fn sleep_millis(&mut self, millis: i64) -> Result<(), HostError> {
        if millis > 0 {
            std::thread::sleep(std::time::Duration::from_millis(millis as u64));
        }
        Ok(())
    }

    fn random_seed(&mut self) -> u64 {
        let elapsed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        elapsed ^ (std::process::id() as u64).rotate_left(32)
    }

    fn property(&mut self, key: &str) -> Option<String> {
        self.properties.get(key).cloned()
    }

    fn properties(&mut self) -> Vec<(String, String)> {
        self.properties
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }

    fn environment(&mut self) -> Vec<(String, String)> {
        std::env::vars().collect()
    }
}

fn run(args: &[String]) -> ExitCode {
    let mut classpath: Vec<PathBuf> = Vec::new();
    let mut overrides: Vec<(String, String)> = Vec::new();
    let mut options = VmOptions::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-cp" | "-classpath" | "--class-path" => {
                index += 1;
                let Some(path) = args.get(index) else {
                    eprintln!("oxjvm: -cp requires a value");
                    return ExitCode::FAILURE;
                };
                classpath.extend(std::env::split_paths(path));
                index += 1;
            }
            other if other.starts_with("-D") => {
                let body = other.strip_prefix("-D").unwrap_or_default();
                let (key, value) = body.split_once('=').unwrap_or((body, ""));
                overrides.push((key.to_string(), value.to_string()));
                index += 1;
            }
            other if is_x_option(other) => {
                let (name, inline) = split_x_option(other);
                let value = match inline {
                    Some(value) => value,
                    None => {
                        index += 1;
                        let Some(value) = args.get(index) else {
                            eprintln!("oxjvm: {name} requires a value");
                            return ExitCode::FAILURE;
                        };
                        value.clone()
                    }
                };
                if !apply_x_option(name, &value, &mut options) {
                    return ExitCode::FAILURE;
                }
                index += 1;
            }
            other if !other.starts_with('-') => break,
            other => {
                eprintln!("oxjvm: unknown option `{other}`");
                return ExitCode::FAILURE;
            }
        }
    }
    // The launcher imposes an ergonomic maximum when `-Xmx` is absent, like the `java` launcher.
    // The VM itself keeps the unbounded default for embedders and tests. An explicit `-Xms`
    // above that maximum raises it, so the request is honoured instead of rejected.
    if options.max_heap_bytes.is_none() {
        let initial = options.initial_heap_bytes.unwrap_or(0);
        options.max_heap_bytes = Some(ergonomic_max_heap_bytes().max(initial));
    }
    if let (Some(initial), Some(max)) = (options.initial_heap_bytes, options.max_heap_bytes) {
        if initial > max {
            eprintln!("oxjvm: -Xms must not be larger than -Xmx");
            return ExitCode::FAILURE;
        }
    }
    let Some(main_class) = args.get(index) else {
        eprintln!("oxjvm: run requires a main class");
        return ExitCode::FAILURE;
    };
    if classpath.is_empty() {
        classpath.push(PathBuf::from("."));
    }
    let program_args: Vec<String> = args[index + 1..].to_vec();
    let mut host = StdHost::new(classpath, &overrides);
    let mut vm = Vm::with_options(&mut host, oxjvm_java::natives(), options);
    let argument_refs: Vec<&str> = program_args.iter().map(String::as_str).collect();
    match vm.run_main(main_class, &argument_refs) {
        Ok(status) => ExitCode::from((status & 0xFF) as u8),
        Err(VmError::Exit(status)) => ExitCode::from((status & 0xFF) as u8),
        Err(VmError::Thrown(exception)) => {
            vm.print_uncaught("main", exception);
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("oxjvm: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The maximum heap the launcher picks when `-Xmx` is absent.
///
/// HotSpot uses a quarter of the physical memory (clamped by the platform); oxjvm does the
/// same on Linux, with a 1 GiB floor and a 4 GiB ceiling so one JVM cannot claim the whole
/// machine. Other platforms fall back to the floor.
fn ergonomic_max_heap_bytes() -> u64 {
    let quarter = fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("MemTotal:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|kilobytes| kilobytes.parse::<u64>().ok())
        })
        .map(|kilobytes| kilobytes.saturating_mul(1024) / 4);
    quarter
        .map(|bytes| bytes.clamp(oxjvm_vm::options::DEFAULT_MAX_HEAP_BYTES, 4 << 30))
        .unwrap_or(oxjvm_vm::options::DEFAULT_MAX_HEAP_BYTES)
}

/// The `-X` options the launcher understands.
const X_OPTIONS: [&str; 3] = ["-Xms", "-Xmx", "-Xss"];

/// Whether the argument is one of the `-X` options.
fn is_x_option(argument: &str) -> bool {
    X_OPTIONS
        .iter()
        .any(|option| argument == *option || argument.starts_with(option))
}

/// Split a `-X` option into its name and the value attached to it.
///
/// `-Xmx256m` and `-Xmx=256m` carry their value; a bare `-Xmx` takes the next argument.
fn split_x_option(argument: &str) -> (&str, Option<String>) {
    if let Some((name, value)) = argument.split_once('=') {
        return (name, Some(value.to_string()));
    }
    for option in X_OPTIONS {
        if argument.len() > option.len() && argument.starts_with(option) {
            return (option, Some(argument[option.len()..].to_string()));
        }
    }
    (argument, None)
}

/// Apply a `-X` option to the VM configuration.
fn apply_x_option(name: &str, value: &str, options: &mut VmOptions) -> bool {
    match name {
        "-Xms" | "-Xmx" => {
            let Some(bytes) = oxjvm_vm::parse_memory_size(value) else {
                eprintln!("oxjvm: {name} expects a size such as 512m, got `{value}`");
                return false;
            };
            if name == "-Xms" {
                options.initial_heap_bytes = Some(bytes);
            } else {
                options.max_heap_bytes = Some(bytes);
            }
            true
        }
        "-Xss" => {
            let Some(bytes) = oxjvm_vm::parse_memory_size(value) else {
                eprintln!("oxjvm: {name} expects a size such as 1m, got `{value}`");
                return false;
            };
            let frames = bytes / oxjvm_vm::options::STACK_FRAME_BYTES;
            options.max_stack_frames = usize::try_from(frames).unwrap_or(usize::MAX).max(1);
            true
        }
        _ => {
            eprintln!("oxjvm: unknown option `{name}`");
            false
        }
    }
}

fn read_class(path: Option<&String>) -> Result<ClassFile, String> {
    let Some(path) = path else {
        return Err("missing class file".into());
    };
    let bytes = fs::read(path).map_err(|error| format!("{path}: {error}"))?;
    ClassFile::read(&bytes).map_err(|error| format!("{path}: {error}"))
}

fn disasm(args: &[String]) -> ExitCode {
    let class = match read_class(args.first()) {
        Ok(class) => class,
        Err(message) => {
            eprintln!("oxjvm: {message}");
            return ExitCode::FAILURE;
        }
    };
    for method in &class.methods {
        let name = class
            .constant_pool
            .utf8(method.name_index)
            .unwrap_or("<bad>");
        let descriptor = class
            .constant_pool
            .utf8(method.descriptor_index)
            .unwrap_or("<bad>");
        println!("{name}{descriptor}");
        let Some(code) = method
            .attributes
            .iter()
            .find_map(|attribute| match &attribute.data {
                AttributeData::Code(code) => Some(code),
                _ => None,
            })
        else {
            continue;
        };
        let mut pc = 0;
        while pc < code.code.len() {
            let Some(decoded) = opcode::decode(&code.code, pc) else {
                println!("  {pc:5}: <invalid>");
                break;
            };
            let operands: Vec<String> = decoded
                .operands
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            println!(
                "  {:5}: {:<16} {}",
                decoded.pc,
                decoded.name.unwrap_or("??"),
                operands.join(" ")
            );
            pc = decoded.next_pc;
        }
    }
    ExitCode::SUCCESS
}

fn inspect(args: &[String]) -> ExitCode {
    let class = match read_class(args.first()) {
        Ok(class) => class,
        Err(message) => {
            eprintln!("oxjvm: {message}");
            return ExitCode::FAILURE;
        }
    };
    let name = class.this_name().unwrap_or("<bad>");
    println!(
        "class {name} version {}.{}",
        class.major_version, class.minor_version
    );
    if let Ok(Some(super_name)) = class.super_name() {
        println!("extends {super_name}");
    }
    if let Ok(interfaces) = class.interface_names() {
        if !interfaces.is_empty() {
            println!("implements {}", interfaces.join(", "));
        }
    }
    println!("constant pool: {} entries", class.constant_pool.count() - 1);
    println!("fields:");
    for field in &class.fields {
        let field_name = class
            .constant_pool
            .utf8(field.name_index)
            .unwrap_or("<bad>");
        let descriptor = class
            .constant_pool
            .utf8(field.descriptor_index)
            .unwrap_or("<bad>");
        println!("  {field_name}:{descriptor}");
    }
    println!("methods:");
    for method in &class.methods {
        let method_name = class
            .constant_pool
            .utf8(method.name_index)
            .unwrap_or("<bad>");
        let descriptor = class
            .constant_pool
            .utf8(method.descriptor_index)
            .unwrap_or("<bad>");
        println!("  {method_name}{descriptor}");
    }
    ExitCode::SUCCESS
}
