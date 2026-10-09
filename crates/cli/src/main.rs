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
use oxjvm_platform::{Host, HostError, Stream};
use oxjvm_vm::{Vm, VmError};

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
         \x20 oxjvm run [-cp <path>] [-D name=value]... <MainClass> [args...]\n\
         \x20 oxjvm disasm <file.class>\n\
         \x20 oxjvm inspect <file.class>\n\
         \x20 oxjvm version",
        env!("CARGO_PKG_VERSION")
    );
}

/// A host that loads classes from `-cp` directories and jars and writes to the real streams.
struct StdHost {
    classpath: Vec<PathBuf>,
    properties: BTreeMap<String, String>,
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
        properties.insert(
            "file.separator".into(),
            std::path::MAIN_SEPARATOR.to_string(),
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
            properties.insert(key.clone(), value.clone());
        }
        Self {
            classpath,
            properties,
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
}

fn run(args: &[String]) -> ExitCode {
    let mut classpath: Vec<PathBuf> = Vec::new();
    let mut overrides: Vec<(String, String)> = Vec::new();
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
            other if !other.starts_with('-') => break,
            other => {
                eprintln!("oxjvm: unknown option `{other}`");
                return ExitCode::FAILURE;
            }
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
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let argument_refs: Vec<&str> = program_args.iter().map(String::as_str).collect();
    match vm.run_main(main_class, &argument_refs) {
        Ok(status) => ExitCode::from((status & 0xFF) as u8),
        Err(VmError::Exit(status)) => ExitCode::from((status & 0xFF) as u8),
        Err(error) => {
            eprintln!("oxjvm: {error}");
            ExitCode::FAILURE
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
