//! `java.util.logging`: a compact logging framework sufficient for JUnit/TestNG bootstrap and for
//! tests that only need a working `Logger`.
//!
//! The logger registry lives in a static `HashMap` on `LogManager`, so it is shared across the VM
//! without any global state in the host crate.  `Logger.getLogger` creates loggers lazily, wires
//! them to the root logger, and publishes records through `Handler`s; `ConsoleHandler` writes to
//! `System.err` via the host.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use oxjvm_classfile::flags::*;

use crate::lang::abstract_method_pub;
use crate::{boolean, class, field, int, long, method, object, receiver, ref_arg, void};
use oxjvm_vm::{ArrayComponent, ObjectRef, Value, Vm, VmError};

const LEVEL_CLASS: &str = "java/util/logging/Level";
const LOG_RECORD_CLASS: &str = "java/util/logging/LogRecord";
const LOGGER_CLASS: &str = "java/util/logging/Logger";
const LOG_MANAGER_CLASS: &str = "java/util/logging/LogManager";
const HANDLER_CLASS: &str = "java/util/logging/Handler";

const LEVEL_CONSTANTS: [(&str, i32); 9] = [
    ("ALL", i32::MIN),
    ("SEVERE", 1000),
    ("WARNING", 900),
    ("INFO", 800),
    ("CONFIG", 700),
    ("FINE", 500),
    ("FINER", 400),
    ("FINEST", 300),
    ("OFF", i32::MAX),
];

fn level_int(vm: &Vm<'_>, level: ObjectRef) -> i32 {
    if level.is_null() {
        return 0;
    }
    crate::lang::read_int_field(vm, level, "value")
}

fn level_name(vm: &Vm<'_>, level: ObjectRef) -> String {
    if level.is_null() {
        return String::from("null");
    }
    vm.read_ref_field(level, vm.class_of(level), "name", "Ljava/lang/String;")
        .and_then(|name| vm.string_value(name))
        .unwrap_or_default()
}

fn level_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class(LEVEL_CLASS)?;
    for (name, value) in LEVEL_CONSTANTS {
        let instance = vm.new_instance(class)?;
        let (declaring, field) = vm.find_field(class, "value", "I")?;
        vm.set_instance_int(instance, declaring, field, value);
        let (declaring, field) = vm.find_field(class, "name", "Ljava/lang/String;")?;
        let text = vm.make_string(name)?;
        vm.set_instance_ref(instance, declaring, field, text);
        vm.set_static_value(
            class,
            name,
            &format!("L{LEVEL_CLASS};"),
            Value::Ref(instance),
        );
    }
    Ok(())
}

const LEVEL_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "<init>",
        "(Ljava/lang/String;I)V",
        ACC_PROTECTED,
        |vm, _, args| {
            let this = receiver(args);
            record_set_ref(vm, this, "name", "Ljava/lang/String;", ref_arg(args, 1));
            crate::lang::write_int_field(vm, this, "value", crate::int_arg(args, 2));
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;ILjava/lang/String;)V",
        ACC_PROTECTED,
        |vm, _, args| {
            let this = receiver(args);
            record_set_ref(vm, this, "name", "Ljava/lang/String;", ref_arg(args, 1));
            crate::lang::write_int_field(vm, this, "value", crate::int_arg(args, 2));
            record_set_ref(
                vm,
                this,
                "resourceBundleName",
                "Ljava/lang/String;",
                ref_arg(args, 3),
            );
            void()
        },
    ),
    method(
        "getResourceBundleName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "resourceBundleName",
                "Ljava/lang/String;",
            ))
        },
    ),
    method("intValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(level_int(vm, receiver(args)))
    }),
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(vm.make_string(&level_name(vm, receiver(args)))?),
    ),
    method(
        "getLocalizedName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(vm.make_string(&level_name(vm, receiver(args)))?),
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(vm.make_string(&level_name(vm, receiver(args)))?),
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            boolean(!other.is_null() && level_int(vm, this) == level_int(vm, other))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(level_int(vm, receiver(args)))
    }),
    method(
        "parse",
        "(Ljava/lang/String;)Ljava/util/logging/Level;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = crate::string_arg(vm, args[0])?;
            let class = vm.resolve_class(LEVEL_CLASS)?;
            for (name, _) in LEVEL_CONSTANTS {
                if name == text {
                    return match vm.read_static_ref_if_set(class, name, &format!("L{LEVEL_CLASS};"))
                    {
                        Some(level) => object(level),
                        None => Err(vm.throw_new(
                            "java/lang/IllegalArgumentException",
                            Some("Unknown level"),
                        )),
                    };
                }
            }
            // Numeric strings denote a custom level in the JDK; accept them as SEVERE-less.
            Err(vm.throw_new("java/lang/IllegalArgumentException", Some(&text)))
        },
    ),
];

pub(crate) const LEVEL: oxjvm_vm::NativeClass = class(
    LEVEL_CLASS,
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[
        field("value", "I", ACC_PRIVATE | ACC_FINAL, None),
        field("name", "Ljava/lang/String;", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "resourceBundleName",
            "Ljava/lang/String;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "ALL",
            "Ljava/util/logging/Level;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SEVERE",
            "Ljava/util/logging/Level;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "WARNING",
            "Ljava/util/logging/Level;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "INFO",
            "Ljava/util/logging/Level;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "CONFIG",
            "Ljava/util/logging/Level;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "FINE",
            "Ljava/util/logging/Level;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "FINER",
            "Ljava/util/logging/Level;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "FINEST",
            "Ljava/util/logging/Level;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "OFF",
            "Ljava/util/logging/Level;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &LEVEL_METHODS,
    Some(level_clinit),
);

// -------------------------------------------------------------------------------------------
// LogRecord
// -------------------------------------------------------------------------------------------

fn record_set_ref(
    vm: &mut Vm<'_>,
    record: ObjectRef,
    name: &str,
    descriptor: &str,
    value: ObjectRef,
) {
    let class = vm.class_of(record);
    if let Some((declaring, field)) = vm.find_field_opt(class, name, descriptor) {
        vm.set_instance_ref(record, declaring, field, value);
    }
}

fn record_get_ref(vm: &Vm<'_>, record: ObjectRef, name: &str, descriptor: &str) -> ObjectRef {
    vm.read_ref_field(record, vm.class_of(record), name, descriptor)
        .unwrap_or(ObjectRef::NULL)
}

fn record_message(vm: &Vm<'_>, record: ObjectRef) -> String {
    vm.string_value(record_get_ref(vm, record, "message", "Ljava/lang/String;"))
        .unwrap_or_default()
}

const LOG_RECORD_METHODS: [oxjvm_vm::NativeMethodDef; 27] = [
    method(
        "<init>",
        "(Ljava/util/logging/Level;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            record_set_ref(
                vm,
                this,
                "level",
                "Ljava/util/logging/Level;",
                ref_arg(args, 1),
            );
            record_set_ref(vm, this, "message", "Ljava/lang/String;", ref_arg(args, 2));
            let millis = vm.host_time_millis();
            crate::lang::write_long_field(vm, this, "millis", millis);
            void()
        },
    ),
    method(
        "getLevel",
        "()Ljava/util/logging/Level;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "level",
                "Ljava/util/logging/Level;",
            ))
        },
    ),
    method(
        "setLevel",
        "(Ljava/util/logging/Level;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            record_set_ref(
                vm,
                receiver(args),
                "level",
                "Ljava/util/logging/Level;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
    method(
        "getMessage",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "message",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "setMessage",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            record_set_ref(
                vm,
                receiver(args),
                "message",
                "Ljava/lang/String;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
    method(
        "getLoggerName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "loggerName",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "setLoggerName",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            record_set_ref(
                vm,
                receiver(args),
                "loggerName",
                "Ljava/lang/String;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
    method(
        "getThrown",
        "()Ljava/lang/Throwable;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "thrown",
                "Ljava/lang/Throwable;",
            ))
        },
    ),
    method(
        "setThrown",
        "(Ljava/lang/Throwable;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            record_set_ref(
                vm,
                receiver(args),
                "thrown",
                "Ljava/lang/Throwable;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
    method(
        "getSourceClassName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "sourceClassName",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "setSourceClassName",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            record_set_ref(
                vm,
                receiver(args),
                "sourceClassName",
                "Ljava/lang/String;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
    method(
        "getSourceMethodName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "sourceMethodName",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "setSourceMethodName",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            record_set_ref(
                vm,
                receiver(args),
                "sourceMethodName",
                "Ljava/lang/String;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
    method(
        "getResourceBundleName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "resourceBundleName",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "setResourceBundleName",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            record_set_ref(
                vm,
                receiver(args),
                "resourceBundleName",
                "Ljava/lang/String;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
    method(
        "getResourceBundle",
        "()Ljava/util/ResourceBundle;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "resourceBundle",
                "Ljava/util/ResourceBundle;",
            ))
        },
    ),
    method(
        "setResourceBundle",
        "(Ljava/util/ResourceBundle;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            record_set_ref(
                vm,
                receiver(args),
                "resourceBundle",
                "Ljava/util/ResourceBundle;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
    method("getMillis", "()J", ACC_PUBLIC, |vm, _, args| {
        long(crate::lang::read_long_field(vm, receiver(args), "millis"))
    }),
    method("setMillis", "(J)V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_long_field(vm, receiver(args), "millis", crate::long_arg(args, 1));
        void()
    }),
    method("getSequenceNumber", "()J", ACC_PUBLIC, |vm, _, args| {
        long(crate::lang::read_long_field(vm, receiver(args), "sequence"))
    }),
    method("setSequenceNumber", "(J)V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_long_field(vm, receiver(args), "sequence", crate::long_arg(args, 1));
        void()
    }),
    method("getThreadID", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "threadID"))
    }),
    method("setThreadID", "(I)V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_int_field(vm, receiver(args), "threadID", crate::int_arg(args, 1));
        void()
    }),
    method("getLongThreadID", "()J", ACC_PUBLIC, |vm, _, args| {
        long(crate::lang::read_long_field(vm, receiver(args), "threadID"))
    }),
    method("setLongThreadID", "(J)V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_long_field(vm, receiver(args), "threadID", crate::long_arg(args, 1));
        void()
    }),
    method(
        "getParameters",
        "()[Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "parameters",
                "[Ljava/lang/Object;",
            ))
        },
    ),
    method(
        "setParameters",
        "([Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            record_set_ref(
                vm,
                receiver(args),
                "parameters",
                "[Ljava/lang/Object;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
];

pub(crate) const LOG_RECORD: oxjvm_vm::NativeClass = class(
    LOG_RECORD_CLASS,
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[
        field("level", "Ljava/util/logging/Level;", ACC_PRIVATE, None),
        field("message", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("loggerName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("thrown", "Ljava/lang/Throwable;", ACC_PRIVATE, None),
        field("sourceClassName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("sourceMethodName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "resourceBundleName",
            "Ljava/lang/String;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "resourceBundle",
            "Ljava/util/ResourceBundle;",
            ACC_PRIVATE,
            None,
        ),
        field("millis", "J", ACC_PRIVATE, None),
        field("sequence", "J", ACC_PRIVATE, None),
        field("threadID", "J", ACC_PRIVATE, None),
        field("parameters", "[Ljava/lang/Object;", ACC_PRIVATE, None),
    ],
    &LOG_RECORD_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// ResourceBundle (minimal)
// -------------------------------------------------------------------------------------------

const RESOURCE_BUNDLE_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "getBundle",
        "(Ljava/lang/String;)Ljava/util/ResourceBundle;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| resource_bundle_get(vm, ref_arg(args, 0), ObjectRef::NULL),
    ),
    method(
        "getBundle",
        "(Ljava/lang/String;Ljava/util/Locale;)Ljava/util/ResourceBundle;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| resource_bundle_get(vm, ref_arg(args, 0), ref_arg(args, 1)),
    ),
    method(
        "getBundle",
        "(Ljava/lang/String;Ljava/util/Locale;Ljava/lang/Module;)Ljava/util/ResourceBundle;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| resource_bundle_get(vm, ref_arg(args, 0), ref_arg(args, 1)),
    ),
    method(
        "getObject",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| resource_bundle_object(vm, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "getString",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = resource_bundle_object(vm, receiver(args), ref_arg(args, 1))?;
            object(value.as_ref())
        },
    ),
    method(
        "handleGetObject",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PROTECTED,
        abstract_method_pub,
    ),
    method(
        "setParent",
        "(Ljava/util/ResourceBundle;)V",
        ACC_PROTECTED,
        |vm, _, args| {
            let this = receiver(args);
            let parent = ref_arg(args, 1);
            let class = vm.class_of(this);
            let (declaring, field) =
                vm.find_field(class, "parent", "Ljava/util/ResourceBundle;")?;
            vm.set_instance_ref(this, declaring, field, parent);
            void()
        },
    ),
];

pub(crate) const RESOURCE_BUNDLE: oxjvm_vm::NativeClass = class(
    "java/util/ResourceBundle",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[field(
        "parent",
        "Ljava/util/ResourceBundle;",
        ACC_PRIVATE,
        None,
    )],
    &RESOURCE_BUNDLE_METHODS,
    None,
);

fn resource_bundle_get(
    vm: &mut Vm<'_>,
    base_object: ObjectRef,
    locale: ObjectRef,
) -> Result<Value, VmError> {
    let base = vm.string_or_empty(base_object);
    let mut candidates = Vec::new();
    if !locale.is_null() {
        let language = vm
            .read_string_field(locale, vm.class_of(locale), "language")
            .unwrap_or_default();
        let country = vm
            .read_string_field(locale, vm.class_of(locale), "country")
            .unwrap_or_default();
        let variant = vm
            .read_string_field(locale, vm.class_of(locale), "variant")
            .unwrap_or_default();
        if !variant.is_empty() {
            candidates.push(format!("{base}_{language}_{country}_{variant}"));
        }
        if !country.is_empty() {
            candidates.push(format!("{base}_{language}_{country}"));
        }
        if !language.is_empty() {
            candidates.push(format!("{base}_{language}"));
        }
    }
    candidates.push(base.clone());
    for candidate in candidates {
        let internal = candidate.replace('.', "/");
        if let Some(class) = vm.resolve_class_lenient(&internal) {
            let instance = vm.new_instance(class)?;
            if let Some((declaring, ctor)) = vm.find_method(class, "<init>", "()V") {
                vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
            }
            return object(instance);
        }
    }
    let locale_str = if locale.is_null() {
        String::new()
    } else {
        let (class, method) =
            vm.resolve_virtual_method(locale, "toString", "()Ljava/lang/String;")?;
        let text = vm.invoke_method(class, method, vec![Value::Ref(locale)])?;
        vm.string_or_empty(text.as_ref())
    };
    let message = format!("Can't find bundle for base name {base}, locale {locale_str}");
    Err(vm.throw_new("java/util/MissingResourceException", Some(&message)))
}

fn resource_bundle_object(
    vm: &mut Vm<'_>,
    bundle: ObjectRef,
    key: ObjectRef,
) -> Result<Value, VmError> {
    let (class, method) = vm.resolve_virtual_method(
        bundle,
        "handleGetObject",
        "(Ljava/lang/String;)Ljava/lang/Object;",
    )?;
    let value = vm.invoke_method(class, method, vec![Value::Ref(bundle), Value::Ref(key)])?;
    if value.as_ref().is_null() {
        // The JDK falls back to the parent bundle chain before giving up.
        let parent = vm
            .read_ref_field(
                bundle,
                vm.class_of(bundle),
                "parent",
                "Ljava/util/ResourceBundle;",
            )
            .unwrap_or(ObjectRef::NULL);
        if !parent.is_null() {
            return resource_bundle_object(vm, parent, key);
        }
        let message = vm.string_or_empty(key);
        return Err(vm.throw_new("java/util/MissingResourceException", Some(&message)));
    }
    Ok(value)
}

// -------------------------------------------------------------------------------------------
// ListResourceBundle
// -------------------------------------------------------------------------------------------

const LIST_RESOURCE_BUNDLE_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "getContents",
        "()[[Ljava/lang/Object;",
        ACC_PROTECTED | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "handleGetObject",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PROTECTED,
        |vm, _, args| {
            let contents = resource_bundle_contents(vm, receiver(args))?;
            let key = vm.string_or_empty(ref_arg(args, 1));
            let rows = vm.array_length(contents)?;
            for index in 0..rows {
                let row = vm.array_get(contents, index as i32)?.as_ref();
                if row.is_null() {
                    continue;
                }
                let candidate = vm.array_get(row, 0)?.as_ref();
                if vm.string_or_empty(candidate) == key {
                    return vm.array_get(row, 1);
                }
            }
            object(ObjectRef::NULL)
        },
    ),
    method("keySet", "()Ljava/util/Set;", ACC_PUBLIC, |vm, _, args| {
        let contents = resource_bundle_contents(vm, receiver(args))?;
        let rows = vm.array_length(contents)?;
        let mut keys = Vec::new();
        for index in 0..rows {
            let row = vm.array_get(contents, index as i32)?.as_ref();
            if !row.is_null() {
                keys.push(vm.array_get(row, 0)?.as_ref());
            }
        }
        object(crate::util::hash_set_from(vm, &keys)?)
    }),
];

fn resource_bundle_contents(vm: &mut Vm<'_>, bundle: ObjectRef) -> Result<ObjectRef, VmError> {
    let (class, method) =
        vm.resolve_virtual_method(bundle, "getContents", "()[[Ljava/lang/Object;")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(bundle)])?
        .as_ref())
}

/// `java.util.ListResourceBundle`: the array-backed bundle base class.
pub(crate) const LIST_RESOURCE_BUNDLE: oxjvm_vm::NativeClass = class(
    "java/util/ListResourceBundle",
    Some("java/util/ResourceBundle"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &LIST_RESOURCE_BUNDLE_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Filter / Formatter / ErrorManager
// -------------------------------------------------------------------------------------------

pub(crate) const FILTER: oxjvm_vm::NativeClass = class(
    "java/util/logging/Filter",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "isLoggable",
        "(Ljava/util/logging/LogRecord;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    )],
    None,
);

const FORMATTER_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "format",
        "(Ljava/util/logging/LogRecord;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "formatMessage",
        "(Ljava/util/logging/LogRecord;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let record = ref_arg(args, 1);
            let message = record_message(vm, record);
            let parameters = record_get_ref(vm, record, "parameters", "[Ljava/lang/Object;");
            let message = crate::format::format_log_message(vm, &message, parameters)?;
            let level = record_get_ref(vm, record, "level", "Ljava/util/logging/Level;");
            let name = level_name(vm, level);
            object(vm.make_string(&format!("{name}: {message}"))?)
        },
    ),
    method(
        "getHead",
        "(Ljava/util/logging/Handler;)Ljava/lang/String;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getTail",
        "(Ljava/util/logging/Handler;)Ljava/lang/String;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method("<init>", "()V", ACC_PROTECTED, |_, _, _| void()),
];

pub(crate) const FORMATTER: oxjvm_vm::NativeClass = class(
    "java/util/logging/Formatter",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &FORMATTER_METHODS,
    None,
);

const SIMPLE_FORMATTER_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "format",
    "(Ljava/util/logging/LogRecord;)Ljava/lang/String;",
    ACC_PUBLIC,
    |vm, _, args| {
        let record = ref_arg(args, 1);
        let message = record_message(vm, record);
        let parameters = record_get_ref(vm, record, "parameters", "[Ljava/lang/Object;");
        let message = crate::format::format_log_message(vm, &message, parameters)?;
        let level = record_get_ref(vm, record, "level", "Ljava/util/logging/Level;");
        let name = level_name(vm, level);
        let millis = crate::lang::read_long_field(vm, record, "millis");
        let source_class = vm
            .string_value(record_get_ref(
                vm,
                record,
                "sourceClassName",
                "Ljava/lang/String;",
            ))
            .unwrap_or_default();
        let source_method = vm
            .string_value(record_get_ref(
                vm,
                record,
                "sourceMethodName",
                "Ljava/lang/String;",
            ))
            .unwrap_or_default();
        let mut text = simple_formatter_date(millis);
        if !source_class.is_empty() {
            text.push(' ');
            text.push_str(&source_class);
            if !source_method.is_empty() {
                text.push(' ');
                text.push_str(&source_method);
            }
        }
        text.push('\n');
        text.push_str(&format!("{name}: {message}\n"));
        let thrown = record_get_ref(vm, record, "thrown", "Ljava/lang/Throwable;");
        if !thrown.is_null() {
            text.push_str(&format_throwable(vm, thrown));
        }
        object(vm.make_string(&text)?)
    },
)];

pub(crate) const SIMPLE_FORMATTER: oxjvm_vm::NativeClass = class(
    "java/util/logging/SimpleFormatter",
    Some("java/util/logging/Formatter"),
    &[],
    ACC_PUBLIC,
    &[],
    &SIMPLE_FORMATTER_METHODS,
    None,
);

fn format_throwable(vm: &Vm<'_>, thrown: ObjectRef) -> String {
    let class = vm.class_of(thrown);
    let name = vm.class_name(class).replace('/', ".");
    let message = vm
        .read_ref_field_any(thrown, "detailMessage")
        .and_then(|value| vm.string_value(value));
    match message {
        Some(message) if !message.is_empty() => format!("{name}: {message}\n"),
        _ => format!("{name}\n"),
    }
}

const ERROR_MANAGER_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "error",
        "(Ljava/lang/String;Ljava/lang/Exception;I)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
];

pub(crate) const ERROR_MANAGER: oxjvm_vm::NativeClass = class(
    "java/util/logging/ErrorManager",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &ERROR_MANAGER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Handler / StreamHandler / ConsoleHandler
// -------------------------------------------------------------------------------------------

fn handler_level(vm: &Vm<'_>, handler: ObjectRef) -> ObjectRef {
    record_get_ref(vm, handler, "level", "Ljava/util/logging/Level;")
}

fn handler_formatter(vm: &Vm<'_>, handler: ObjectRef) -> ObjectRef {
    record_get_ref(vm, handler, "formatter", "Ljava/util/logging/Formatter;")
}

const HANDLER_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method("<init>", "()V", ACC_PROTECTED, |vm, _, args| {
        let this = receiver(args);
        // Default handler level is ALL-equivalent (null means "log everything").
        record_set_ref(
            vm,
            this,
            "level",
            "Ljava/util/logging/Level;",
            ObjectRef::NULL,
        );
        let formatter_class = vm.resolve_class("java/util/logging/SimpleFormatter")?;
        let formatter = vm.new_instance(formatter_class)?;
        record_set_ref(
            vm,
            this,
            "formatter",
            "Ljava/util/logging/Formatter;",
            formatter,
        );
        void()
    }),
    method(
        "publish",
        "(Ljava/util/logging/LogRecord;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "flush",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "close",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "getLevel",
        "()Ljava/util/logging/Level;",
        ACC_PUBLIC,
        |vm, _, args| object(handler_level(vm, receiver(args))),
    ),
    method(
        "setLevel",
        "(Ljava/util/logging/Level;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            record_set_ref(
                vm,
                receiver(args),
                "level",
                "Ljava/util/logging/Level;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
    method(
        "getFormatter",
        "()Ljava/util/logging/Formatter;",
        ACC_PUBLIC,
        |vm, _, args| object(handler_formatter(vm, receiver(args))),
    ),
    method(
        "setFormatter",
        "(Ljava/util/logging/Formatter;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            record_set_ref(
                vm,
                receiver(args),
                "formatter",
                "Ljava/util/logging/Formatter;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
    method(
        "getFilter",
        "()Ljava/util/logging/Filter;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "filter",
                "Ljava/util/logging/Filter;",
            ))
        },
    ),
    method(
        "setFilter",
        "(Ljava/util/logging/Filter;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            record_set_ref(
                vm,
                receiver(args),
                "filter",
                "Ljava/util/logging/Filter;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
    method(
        "getErrorManager",
        "()Ljava/util/logging/ErrorManager;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "errorManager",
                "Ljava/util/logging/ErrorManager;",
            ))
        },
    ),
    method(
        "isLoggable",
        "(Ljava/util/logging/LogRecord;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let record = ref_arg(args, 1);
            let lvl = binder_level(vm, this);
            let record_level = record_get_ref(vm, record, "level", "Ljava/util/logging/Level;");
            boolean(level_int(vm, record_level) >= lvl)
        },
    ),
];

fn binder_level(vm: &Vm<'_>, handler: ObjectRef) -> i32 {
    let level = handler_level(vm, handler);
    if level.is_null() {
        i32::MIN
    } else {
        level_int(vm, level)
    }
}

pub(crate) const HANDLER: oxjvm_vm::NativeClass = class(
    HANDLER_CLASS,
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field("level", "Ljava/util/logging/Level;", ACC_PRIVATE, None),
        field(
            "formatter",
            "Ljava/util/logging/Formatter;",
            ACC_PRIVATE,
            None,
        ),
        field("filter", "Ljava/util/logging/Filter;", ACC_PRIVATE, None),
        field(
            "errorManager",
            "Ljava/util/logging/ErrorManager;",
            ACC_PRIVATE,
            None,
        ),
        field("output", "Ljava/lang/Object;", 0, None),
    ],
    &HANDLER_METHODS,
    None,
);

fn publish_to_output(
    vm: &mut Vm<'_>,
    handler: ObjectRef,
    record: ObjectRef,
) -> Result<(), VmError> {
    let formatter = handler_formatter(vm, handler);
    let text_value = if formatter.is_null() {
        Value::Ref(ObjectRef::NULL)
    } else {
        let (class, method) = vm.resolve_virtual_method(
            formatter,
            "format",
            "(Ljava/util/logging/LogRecord;)Ljava/lang/String;",
        )?;
        vm.invoke_method(
            class,
            method,
            vec![Value::Ref(formatter), Value::Ref(record)],
        )?
    };
    let text = vm.string_value(text_value.as_ref()).unwrap_or_default();
    let output = record_get_ref(vm, handler, "output", "Ljava/lang/Object;");
    if output.is_null() {
        vm.write_host(oxjvm_platform::Stream::Stderr, text.as_bytes());
        return Ok(());
    }
    // Write the encoded bytes through OutputStream.write(byte[], int, int) so
    // any OutputStream works, not just PrintStream (which alone has print()).
    let bytes = text.as_bytes();
    let array = crate::io::write_bytes_array(vm, bytes)?;
    let (class, method) = vm.resolve_virtual_method(output, "write", "([BII)V")?;
    vm.invoke_method(
        class,
        method,
        vec![
            Value::Ref(output),
            Value::Ref(array),
            Value::Int(0),
            Value::Int(bytes.len() as i32),
        ],
    )?;
    Ok(())
}

const STREAM_HANDLER_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "<init>",
        "(Ljava/io/OutputStream;Ljava/util/logging/Formatter;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let output = ref_arg(args, 1);
            let formatter = ref_arg(args, 2);
            if output.is_null() || formatter.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            record_set_ref(
                vm,
                this,
                "level",
                "Ljava/util/logging/Level;",
                ObjectRef::NULL,
            );
            record_set_ref(
                vm,
                this,
                "formatter",
                "Ljava/util/logging/Formatter;",
                formatter,
            );
            record_set_ref(vm, this, "output", "Ljava/lang/Object;", output);
            void()
        },
    ),
    method(
        "publish",
        "(Ljava/util/logging/LogRecord;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            publish_to_output(vm, receiver(args), ref_arg(args, 1))?;
            void()
        },
    ),
    method("flush", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("close", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "setOutputStream",
        "(Ljava/io/OutputStream;)V",
        ACC_PROTECTED,
        |vm, _, args| {
            record_set_ref(
                vm,
                receiver(args),
                "output",
                "Ljava/lang/Object;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
];

pub(crate) const STREAM_HANDLER: oxjvm_vm::NativeClass = class(
    "java/util/logging/StreamHandler",
    Some(HANDLER_CLASS),
    &[],
    ACC_PUBLIC,
    &[],
    &STREAM_HANDLER_METHODS,
    None,
);

const CONSOLE_HANDLER_METHODS: [oxjvm_vm::NativeMethodDef; 1] =
    [method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        record_set_ref(
            vm,
            this,
            "level",
            "Ljava/util/logging/Level;",
            ObjectRef::NULL,
        );
        let formatter_class = vm.resolve_class("java/util/logging/SimpleFormatter")?;
        let formatter = vm.new_instance(formatter_class)?;
        record_set_ref(
            vm,
            this,
            "formatter",
            "Ljava/util/logging/Formatter;",
            formatter,
        );
        let stderr = vm.stderr_object();
        record_set_ref(vm, this, "output", "Ljava/lang/Object;", stderr);
        void()
    })];

pub(crate) const CONSOLE_HANDLER: oxjvm_vm::NativeClass = class(
    "java/util/logging/ConsoleHandler",
    Some("java/util/logging/StreamHandler"),
    &[],
    ACC_PUBLIC,
    &[],
    &CONSOLE_HANDLER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Logger
// -------------------------------------------------------------------------------------------

fn logger_name(vm: &Vm<'_>, logger: ObjectRef) -> String {
    vm.read_ref_field(logger, vm.class_of(logger), "name", "Ljava/lang/String;")
        .and_then(|value| vm.string_value(value))
        .unwrap_or_default()
}

fn logger_handlers(vm: &Vm<'_>, logger: ObjectRef) -> ObjectRef {
    record_get_ref(vm, logger, "handlers", "Ljava/util/ArrayList;")
}

fn hlist_add(vm: &mut Vm<'_>, list: ObjectRef, value: ObjectRef) -> Result<(), VmError> {
    let (class, method) = vm.resolve_virtual_method(list, "add", "(Ljava/lang/Object;)Z")?;
    vm.invoke_method(class, method, vec![Value::Ref(list), Value::Ref(value)])?;
    Ok(())
}

fn hlist_len(vm: &mut Vm<'_>, list: ObjectRef) -> Result<i32, VmError> {
    let (class, method) = vm.resolve_virtual_method(list, "size", "()I")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(list)])?
        .as_int())
}

fn hlist_get(vm: &mut Vm<'_>, list: ObjectRef, index: i32) -> Result<ObjectRef, VmError> {
    let (class, method) = vm.resolve_virtual_method(list, "get", "(I)Ljava/lang/Object;")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(list), Value::Int(index)])?
        .as_ref())
}

fn publish_from_logger(
    vm: &mut Vm<'_>,
    logger: ObjectRef,
    record: ObjectRef,
) -> Result<(), VmError> {
    let handlers = logger_handlers(vm, logger);
    let count = hlist_len(vm, handlers)?;
    for index in 0..count {
        let handler = hlist_get(vm, handlers, index)?;
        let (class, method) =
            vm.resolve_virtual_method(handler, "publish", "(Ljava/util/logging/LogRecord;)V")?;
        vm.invoke_method(class, method, vec![Value::Ref(handler), Value::Ref(record)])?;
    }
    if crate::lang::read_int_field(vm, logger, "useParentHandlers") != 0 {
        let parent = record_get_ref(vm, logger, "parent", "Ljava/util/logging/Logger;");
        if !parent.is_null() {
            return publish_from_logger(vm, parent, record);
        }
    }
    Ok(())
}

fn effective_level(vm: &Vm<'_>, logger: ObjectRef) -> i32 {
    let mut current = logger;
    while !current.is_null() {
        let level = record_get_ref(vm, current, "level", "Ljava/util/logging/Level;");
        if !level.is_null() {
            return level_int(vm, level);
        }
        current = record_get_ref(vm, current, "parent", "Ljava/util/logging/Logger;");
    }
    800 // INFO
}

fn registry_map(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(LOG_MANAGER_CLASS)?;
    if let Some(map) = vm.read_static_ref_if_set(class, "loggers", "Ljava/util/HashMap;") {
        return Ok(map);
    }
    // Force the LogManager's <clinit> to install the map.
    vm.ensure_initialized(class)?;
    vm.read_static_ref_if_set(class, "loggers", "Ljava/util/HashMap;")
        .ok_or_else(|| VmError::internal("LogManager.loggers missing"))
}

fn logger_from_registry(vm: &mut Vm<'_>, name: &str) -> Result<ObjectRef, VmError> {
    let map = registry_map(vm)?;
    let key = vm.make_string(name)?;
    let existing = crate::util::hash_map_lookup(vm, map, key)?;
    if !existing.is_null() {
        return Ok(existing);
    }
    let class = vm.resolve_class(LOGGER_CLASS)?;
    let logger = vm.new_instance(class)?;
    let name_ref = vm.make_string(name)?;
    record_set_ref(vm, logger, "name", "Ljava/lang/String;", name_ref);
    record_set_ref(
        vm,
        logger,
        "level",
        "Ljava/util/logging/Level;",
        ObjectRef::NULL,
    );
    record_set_ref(
        vm,
        logger,
        "filter",
        "Ljava/util/logging/Filter;",
        ObjectRef::NULL,
    );
    record_set_ref(
        vm,
        logger,
        "resourceBundleName",
        "Ljava/lang/String;",
        ObjectRef::NULL,
    );
    crate::lang::write_int_field(vm, logger, "useParentHandlers", 1);
    let list = vm.resolve_class("java/util/ArrayList")?;
    let list_instance = vm.new_instance(list)?;
    let (declaring, ctor) = vm
        .find_method(list, "<init>", "()V")
        .ok_or_else(|| VmError::internal("ArrayList() missing"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(list_instance)])?;
    record_set_ref(
        vm,
        logger,
        "handlers",
        "Ljava/util/ArrayList;",
        list_instance,
    );
    // Link to the root logger (name "").
    if !name.is_empty() {
        let root = logger_from_registry(vm, "")?;
        record_set_ref(vm, logger, "parent", "Ljava/util/logging/Logger;", root);
    }
    crate::util::hash_map_insert(vm, map, key, logger)?;
    Ok(logger)
}

fn logger_log_message(
    vm: &mut Vm<'_>,
    logger: ObjectRef,
    level: ObjectRef,
    message: ObjectRef,
    thrown: ObjectRef,
) -> Result<(), VmError> {
    if level.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    if level_int(vm, level) < effective_level(vm, logger) {
        return Ok(());
    }
    let class = vm.resolve_class(LOG_RECORD_CLASS)?;
    let record = vm.new_instance(class)?;
    record_set_ref(vm, record, "level", "Ljava/util/logging/Level;", level);
    record_set_ref(vm, record, "message", "Ljava/lang/String;", message);
    record_set_ref(vm, record, "thrown", "Ljava/lang/Throwable;", thrown);
    let name = logger_name(vm, logger);
    let name_ref = vm.make_string(&name)?;
    record_set_ref(vm, record, "loggerName", "Ljava/lang/String;", name_ref);
    publish_from_logger(vm, logger, record)
}

/// Publish a record carrying an explicit source class/method and parameters,
/// as `Logger.logp` and the `entering`/`exiting` convenience methods do.
fn logger_logp(
    vm: &mut Vm<'_>,
    logger: ObjectRef,
    level: ObjectRef,
    source_class: ObjectRef,
    source_method: ObjectRef,
    message: ObjectRef,
    parameters: ObjectRef,
    thrown: ObjectRef,
) -> Result<(), VmError> {
    if level.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    if level_int(vm, level) < effective_level(vm, logger) {
        return Ok(());
    }
    let class = vm.resolve_class(LOG_RECORD_CLASS)?;
    let record = vm.new_instance(class)?;
    record_set_ref(vm, record, "level", "Ljava/util/logging/Level;", level);
    record_set_ref(vm, record, "message", "Ljava/lang/String;", message);
    record_set_ref(vm, record, "thrown", "Ljava/lang/Throwable;", thrown);
    record_set_ref(vm, record, "parameters", "[Ljava/lang/Object;", parameters);
    record_set_ref(
        vm,
        record,
        "sourceClassName",
        "Ljava/lang/String;",
        source_class,
    );
    record_set_ref(
        vm,
        record,
        "sourceMethodName",
        "Ljava/lang/String;",
        source_method,
    );
    let millis = vm.host_time_millis();
    crate::lang::write_long_field(vm, record, "millis", millis);
    let name = logger_name(vm, logger);
    let name_ref = vm.make_string(&name)?;
    record_set_ref(vm, record, "loggerName", "Ljava/lang/String;", name_ref);
    publish_from_logger(vm, logger, record)
}

/// Wrap a single argument in a one-element `Object[]`, as the single-parameter
/// `logp`/`entering`/`exiting` overloads do.
fn single_param_array(vm: &mut Vm<'_>, value: ObjectRef) -> Result<ObjectRef, VmError> {
    let array = vm.allocate_array_of(ArrayComponent::Reference, 1)?;
    vm.array_set_ref(array, 0, value)?;
    Ok(array)
}

/// The date prefix `SimpleFormatter` prints, in UTC (the only zone oxjvm models).
fn simple_formatter_date(millis: i64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let epoch_second = millis.div_euclid(1000);
    let epoch_day = epoch_second.div_euclid(86_400);
    let second_of_day = epoch_second.rem_euclid(86_400);
    let (year, month, day) = crate::time::civil_from_days(epoch_day);
    let hour24 = second_of_day / 3600;
    let minute = (second_of_day % 3600) / 60;
    let second = second_of_day % 60;
    let hour12 = match hour24 % 12 {
        0 => 12,
        h => h,
    };
    let ampm = if hour24 < 12 { "AM" } else { "PM" };
    let month_name = MONTHS.get((month - 1) as usize).copied().unwrap_or("Jan");
    format!("{month_name} {day}, {year} {hour12}:{minute:02}:{second:02} {ampm}")
}

fn level_named(vm: &mut Vm<'_>, name: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(LEVEL_CLASS)?;
    vm.ensure_initialized(class)?;
    vm.read_static_ref_if_set(class, name, &format!("L{LEVEL_CLASS};"))
        .ok_or_else(|| VmError::internal("Level constant missing"))
}

const LOGGER_METHODS: [oxjvm_vm::NativeMethodDef; 43] = [
    method(
        "getLogger",
        "(Ljava/lang/String;)Ljava/util/logging/Logger;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = crate::string_arg(vm, args[0])?;
            object(logger_from_registry(vm, &name)?)
        },
    ),
    method(
        "getLogger",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/util/logging/Logger;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = crate::string_arg(vm, args[0])?;
            object(logger_from_registry(vm, &name)?)
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PROTECTED,
        |vm, _, args| {
            let this = receiver(args);
            record_set_ref(vm, this, "name", "Ljava/lang/String;", ref_arg(args, 1));
            record_set_ref(
                vm,
                this,
                "resourceBundleName",
                "Ljava/lang/String;",
                ref_arg(args, 2),
            );
            crate::lang::write_int_field(vm, this, "useParentHandlers", 1);
            let list = vm.resolve_class("java/util/ArrayList")?;
            let list_instance = vm.new_instance(list)?;
            let (declaring, ctor) = vm
                .find_method(list, "<init>", "()V")
                .ok_or_else(|| VmError::internal("ArrayList() missing"))?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(list_instance)])?;
            record_set_ref(vm, this, "handlers", "Ljava/util/ArrayList;", list_instance);
            void()
        },
    ),
    method(
        "getGlobal",
        "()Ljava/util/logging/Logger;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| object(logger_from_registry(vm, "global")?),
    ),
    method(
        "getAnonymousLogger",
        "()Ljava/util/logging/Logger;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class(LOGGER_CLASS)?;
            let logger = vm.new_instance(class)?;
            let empty = vm.make_string("")?;
            record_set_ref(vm, logger, "name", "Ljava/lang/String;", empty);
            crate::lang::write_int_field(vm, logger, "useParentHandlers", 0);
            let list = vm.resolve_class("java/util/ArrayList")?;
            let list_instance = vm.new_instance(list)?;
            let (declaring, ctor) = vm
                .find_method(list, "<init>", "()V")
                .ok_or_else(|| VmError::internal("ArrayList() missing"))?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(list_instance)])?;
            record_set_ref(
                vm,
                logger,
                "handlers",
                "Ljava/util/ArrayList;",
                list_instance,
            );
            object(logger)
        },
    ),
    method(
        "getName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = logger_name(vm, receiver(args));
            object(vm.make_string(&name)?)
        },
    ),
    method(
        "getLevel",
        "()Ljava/util/logging/Level;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "level",
                "Ljava/util/logging/Level;",
            ))
        },
    ),
    method(
        "setLevel",
        "(Ljava/util/logging/Level;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            record_set_ref(
                vm,
                receiver(args),
                "level",
                "Ljava/util/logging/Level;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
    method(
        "getParent",
        "()Ljava/util/logging/Logger;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "parent",
                "Ljava/util/logging/Logger;",
            ))
        },
    ),
    method(
        "setParent",
        "(Ljava/util/logging/Logger;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            record_set_ref(
                vm,
                receiver(args),
                "parent",
                "Ljava/util/logging/Logger;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
    method("getUseParentHandlers", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "useParentHandlers") != 0)
    }),
    method("setUseParentHandlers", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        let value = if crate::int_arg(args, 1) != 0 { 1 } else { 0 };
        crate::lang::write_int_field(vm, receiver(args), "useParentHandlers", value);
        void()
    }),
    method(
        "getHandlers",
        "()[Ljava/util/logging/Handler;",
        ACC_PUBLIC,
        |vm, _, args| {
            let handlers = logger_handlers(vm, receiver(args));
            let count = hlist_len(vm, handlers)?;
            let handler_class = vm.resolve_class(HANDLER_CLASS)?;
            let array = vm.allocate_object_array(handler_class, count as usize)?;
            for index in 0..count {
                let handler = hlist_get(vm, handlers, index)?;
                vm.array_set_ref(array, index as usize, handler)?;
            }
            object(array)
        },
    ),
    method(
        "addHandler",
        "(Ljava/util/logging/Handler;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let handler = ref_arg(args, 1);
            if handler.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let handlers = logger_handlers(vm, receiver(args));
            hlist_add(vm, handlers, handler)?;
            void()
        },
    ),
    method(
        "removeHandler",
        "(Ljava/util/logging/Handler;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "setFilter",
        "(Ljava/util/logging/Filter;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            record_set_ref(
                vm,
                receiver(args),
                "filter",
                "Ljava/util/logging/Filter;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
    method(
        "getFilter",
        "()Ljava/util/logging/Filter;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "filter",
                "Ljava/util/logging/Filter;",
            ))
        },
    ),
    method(
        "getResourceBundleName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "resourceBundleName",
                "Ljava/lang/String;",
            ))
        },
    ),
    method(
        "getResourceBundle",
        "()Ljava/util/ResourceBundle;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "resourceBundle",
                "Ljava/util/ResourceBundle;",
            ))
        },
    ),
    method(
        "setResourceBundle",
        "(Ljava/util/ResourceBundle;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let bundle = ref_arg(args, 1);
            record_set_ref(
                vm,
                this,
                "resourceBundle",
                "Ljava/util/ResourceBundle;",
                bundle,
            );
            if bundle.is_null() {
                record_set_ref(
                    vm,
                    this,
                    "resourceBundleName",
                    "Ljava/lang/String;",
                    ObjectRef::NULL,
                );
            }
            void()
        },
    ),
    method(
        "isLoggable",
        "(Ljava/util/logging/Level;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let level = ref_arg(args, 1);
            if level.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            boolean(level_int(vm, level) >= effective_level(vm, receiver(args)))
        },
    ),
    method(
        "log",
        "(Ljava/util/logging/LogRecord;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let record = ref_arg(args, 1);
            if record.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            publish_from_logger(vm, receiver(args), record)?;
            void()
        },
    ),
    method(
        "log",
        "(Ljava/util/logging/Level;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let message = ref_arg(args, 2);
            logger_log_message(
                vm,
                receiver(args),
                ref_arg(args, 1),
                message,
                ObjectRef::NULL,
            )?;
            void()
        },
    ),
    method(
        "log",
        "(Ljava/util/logging/Level;Ljava/lang/String;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            logger_log_message(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ObjectRef::NULL,
            )?;
            void()
        },
    ),
    method(
        "log",
        "(Ljava/util/logging/Level;Ljava/lang/String;Ljava/lang/Object;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            logger_log_message(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ObjectRef::NULL,
            )?;
            void()
        },
    ),
    method(
        "log",
        "(Ljava/util/logging/Level;Ljava/lang/String;[Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            logger_log_message(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ObjectRef::NULL,
            )?;
            void()
        },
    ),
    method(
        "log",
        "(Ljava/util/logging/Level;Ljava/lang/String;Ljava/lang/Throwable;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            logger_log_message(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ref_arg(args, 3),
            )?;
            void()
        },
    ),
    method(
        "info",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let level = level_named(vm, "INFO")?;
            logger_log_message(vm, receiver(args), level, ref_arg(args, 1), ObjectRef::NULL)?;
            void()
        },
    ),
    method(
        "warning",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let level = level_named(vm, "WARNING")?;
            logger_log_message(vm, receiver(args), level, ref_arg(args, 1), ObjectRef::NULL)?;
            void()
        },
    ),
    method(
        "severe",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let level = level_named(vm, "SEVERE")?;
            logger_log_message(vm, receiver(args), level, ref_arg(args, 1), ObjectRef::NULL)?;
            void()
        },
    ),
    method(
        "fine",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let level = level_named(vm, "FINE")?;
            logger_log_message(vm, receiver(args), level, ref_arg(args, 1), ObjectRef::NULL)?;
            void()
        },
    ),
    method(
        "finer",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let level = level_named(vm, "FINER")?;
            logger_log_message(vm, receiver(args), level, ref_arg(args, 1), ObjectRef::NULL)?;
            void()
        },
    ),
    method(
        "finest",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let level = level_named(vm, "FINEST")?;
            logger_log_message(vm, receiver(args), level, ref_arg(args, 1), ObjectRef::NULL)?;
            void()
        },
    ),
    method(
        "config",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let level = level_named(vm, "CONFIG")?;
            logger_log_message(vm, receiver(args), level, ref_arg(args, 1), ObjectRef::NULL)?;
            void()
        },
    ),
    method(
        "logp",
        "(Ljava/util/logging/Level;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            logger_logp(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ref_arg(args, 3),
                ref_arg(args, 4),
                ObjectRef::NULL,
                ObjectRef::NULL,
            )?;
            void()
        },
    ),
    method(
        "logp",
        "(Ljava/util/logging/Level;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let params = single_param_array(vm, ref_arg(args, 5))?;
            logger_logp(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ref_arg(args, 3),
                ref_arg(args, 4),
                params,
                ObjectRef::NULL,
            )?;
            void()
        },
    ),
    method(
        "logp",
        "(Ljava/util/logging/Level;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;[Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            logger_logp(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ref_arg(args, 3),
                ref_arg(args, 4),
                ref_arg(args, 5),
                ObjectRef::NULL,
            )?;
            void()
        },
    ),
    method(
        "logp",
        "(Ljava/util/logging/Level;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/Throwable;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            logger_logp(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ref_arg(args, 3),
                ref_arg(args, 4),
                ObjectRef::NULL,
                ref_arg(args, 5),
            )?;
            void()
        },
    ),
    method(
        "entering",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let level = level_named(vm, "FINER")?;
            let message = vm.make_string("ENTRY")?;
            logger_logp(
                vm,
                receiver(args),
                level,
                ref_arg(args, 1),
                ref_arg(args, 2),
                message,
                ObjectRef::NULL,
                ObjectRef::NULL,
            )?;
            void()
        },
    ),
    method(
        "entering",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let level = level_named(vm, "FINER")?;
            let message = vm.make_string("ENTRY {0}")?;
            let params = single_param_array(vm, ref_arg(args, 3))?;
            logger_logp(
                vm,
                receiver(args),
                level,
                ref_arg(args, 1),
                ref_arg(args, 2),
                message,
                params,
                ObjectRef::NULL,
            )?;
            void()
        },
    ),
    method(
        "entering",
        "(Ljava/lang/String;Ljava/lang/String;[Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let level = level_named(vm, "FINER")?;
            let params = ref_arg(args, 3);
            let mut message = String::from("ENTRY");
            if !params.is_null() {
                let count = vm.array_length(params).unwrap_or(0);
                for index in 0..count {
                    message.push_str(&format!(" {{{index}}}"));
                }
            }
            let message = vm.make_string(&message)?;
            logger_logp(
                vm,
                receiver(args),
                level,
                ref_arg(args, 1),
                ref_arg(args, 2),
                message,
                params,
                ObjectRef::NULL,
            )?;
            void()
        },
    ),
    method(
        "exiting",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let level = level_named(vm, "FINER")?;
            let message = vm.make_string("RETURN")?;
            logger_logp(
                vm,
                receiver(args),
                level,
                ref_arg(args, 1),
                ref_arg(args, 2),
                message,
                ObjectRef::NULL,
                ObjectRef::NULL,
            )?;
            void()
        },
    ),
    method(
        "exiting",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let level = level_named(vm, "FINER")?;
            let message = vm.make_string("RETURN {0}")?;
            let params = single_param_array(vm, ref_arg(args, 3))?;
            logger_logp(
                vm,
                receiver(args),
                level,
                ref_arg(args, 1),
                ref_arg(args, 2),
                message,
                params,
                ObjectRef::NULL,
            )?;
            void()
        },
    ),
];

pub(crate) const LOGGER: oxjvm_vm::NativeClass = class(
    LOGGER_CLASS,
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "level",
            "Ljava/util/logging/Level;",
            ACC_PRIVATE | ACC_VOLATILE,
            None,
        ),
        field(
            "parent",
            "Ljava/util/logging/Logger;",
            ACC_PRIVATE | ACC_VOLATILE,
            None,
        ),
        field("handlers", "Ljava/util/ArrayList;", ACC_PRIVATE, None),
        field("filter", "Ljava/util/logging/Filter;", ACC_PRIVATE, None),
        field(
            "resourceBundleName",
            "Ljava/lang/String;",
            ACC_PRIVATE,
            None,
        ),
        field(
            "resourceBundle",
            "Ljava/util/ResourceBundle;",
            ACC_PRIVATE | ACC_VOLATILE,
            None,
        ),
        field("useParentHandlers", "I", ACC_PRIVATE, None),
    ],
    &LOGGER_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// LogManager
// -------------------------------------------------------------------------------------------

fn log_manager_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class(LOG_MANAGER_CLASS)?;
    let map = crate::util::hash_map_new(vm)?;
    vm.set_static_value(class, "loggers", "Ljava/util/HashMap;", Value::Ref(map));
    let manager = vm.new_instance(class)?;
    vm.set_static_value(
        class,
        "manager",
        &format!("L{LOG_MANAGER_CLASS};"),
        Value::Ref(manager),
    );
    // Install the root logger with a ConsoleHandler.
    let root = logger_from_registry_initializing(vm)?;
    let handler_class = vm.resolve_class("java/util/logging/ConsoleHandler")?;
    let handler = vm.new_instance(handler_class)?;
    let (declaring, ctor) = vm
        .find_method(handler_class, "<init>", "()V")
        .ok_or_else(|| VmError::internal("ConsoleHandler() missing"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(handler)])?;
    let handlers = logger_handlers(vm, root);
    hlist_add(vm, handlers, handler)?;
    Ok(())
}

/// Create the root logger (name "") the way `logger_from_registry` does, but without consulting the
/// registry (which is being initialized).
fn logger_from_registry_initializing(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(LOGGER_CLASS)?;
    let logger = vm.new_instance(class)?;
    let empty = vm.make_string("")?;
    record_set_ref(vm, logger, "name", "Ljava/lang/String;", empty);
    record_set_ref(
        vm,
        logger,
        "level",
        "Ljava/util/logging/Level;",
        ObjectRef::NULL,
    );
    record_set_ref(
        vm,
        logger,
        "filter",
        "Ljava/util/logging/Filter;",
        ObjectRef::NULL,
    );
    record_set_ref(
        vm,
        logger,
        "resourceBundleName",
        "Ljava/lang/String;",
        ObjectRef::NULL,
    );
    crate::lang::write_int_field(vm, logger, "useParentHandlers", 0);
    let list = vm.resolve_class("java/util/ArrayList")?;
    let list_instance = vm.new_instance(list)?;
    let (declaring, ctor) = vm
        .find_method(list, "<init>", "()V")
        .ok_or_else(|| VmError::internal("ArrayList() missing"))?;
    vm.invoke_method(declaring, ctor, vec![Value::Ref(list_instance)])?;
    record_set_ref(
        vm,
        logger,
        "handlers",
        "Ljava/util/ArrayList;",
        list_instance,
    );
    let map_class = vm.resolve_class(LOG_MANAGER_CLASS)?;
    let map = vm
        .read_static_ref_if_set(map_class, "loggers", "Ljava/util/HashMap;")
        .ok_or_else(|| VmError::internal("loggers map missing"))?;
    let key = vm.make_string("")?;
    crate::util::hash_map_insert(vm, map, key, logger)?;
    Ok(logger)
}

const LOG_MANAGER_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method("<init>", "()V", ACC_PROTECTED, |_, _, _| void()),
    method(
        "getLogManager",
        "()Ljava/util/logging/LogManager;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class(LOG_MANAGER_CLASS)?;
            vm.ensure_initialized(class)?;
            let manager = vm
                .read_static_ref_if_set(class, "manager", &format!("L{LOG_MANAGER_CLASS};"))
                .ok_or_else(|| VmError::internal("LogManager.manager missing"))?;
            object(manager)
        },
    ),
    method(
        "getLogger",
        "(Ljava/lang/String;)Ljava/util/logging/Logger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = crate::string_arg(vm, args[1])?;
            object(logger_from_registry(vm, &name)?)
        },
    ),
    method(
        "addLogger",
        "(Ljava/util/logging/Logger;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let logger = ref_arg(args, 1);
            let name = logger_name(vm, logger);
            let map = registry_map(vm)?;
            let key = vm.make_string(&name)?;
            crate::util::hash_map_insert(vm, map, key, logger)?;
            boolean(true)
        },
    ),
    method(
        "getProperty",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getProperty",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        |_, _, args| object(ref_arg(args, 2)),
    ),
    method("readConfiguration", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("reset", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "getLoggerNames",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getLoggingMXBean",
        "()Ljava/util/logging/LoggingMXBean;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/logging/OjvmLoggingMXBean")?;
            object(vm.new_instance(class)?)
        },
    ),
];

// -------------------------------------------------------------------------------------------
// LoggingMXBean
// -------------------------------------------------------------------------------------------

const LOGGING_MXBEAN_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "getLoggerNames",
        "()Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, _| object(crate::util::array_list_from(vm, &[])?),
    ),
    method(
        "getLoggerLevel",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "setLoggerLevel",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "getParentLoggerName",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
];

/// `java.util.logging.LoggingMXBean`: a read-only stub over the logger registry.
pub(crate) const LOGGING_MXBEAN: oxjvm_vm::NativeClass = class(
    "java/util/logging/LoggingMXBean",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &LOGGING_MXBEAN_METHODS,
    None,
);

/// The concrete MXBean returned by `LogManager.getLoggingMXBean`.
pub(crate) const LOGGING_MXBEAN_IMPL: oxjvm_vm::NativeClass = class(
    "java/util/logging/OjvmLoggingMXBean",
    Some("java/lang/Object"),
    &["java/util/logging/LoggingMXBean"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[],
    &LOGGING_MXBEAN_METHODS,
    None,
);

pub(crate) const LOG_MANAGER: oxjvm_vm::NativeClass = class(
    LOG_MANAGER_CLASS,
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field(
            "loggers",
            "Ljava/util/HashMap;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
        field(
            "manager",
            "Ljava/util/logging/LogManager;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
    ],
    &LOG_MANAGER_METHODS,
    Some(log_manager_clinit),
);
