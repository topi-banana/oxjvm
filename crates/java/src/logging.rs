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

const LOG_RECORD_METHODS: [oxjvm_vm::NativeMethodDef; 29] = [
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
            // Capture the wall clock with nanosecond precision (Instant.now()):
            // the host nano_time reports epoch nanoseconds on the CLI host.
            let nanos = vm.host_nano_time();
            crate::lang::write_long_field(vm, this, "millis", nanos / 1_000_000);
            crate::lang::write_int_field(vm, this, "nanoAdjustment", (nanos % 1_000_000) as i32);
            stamp_record(vm, this);
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
        let this = receiver(args);
        crate::lang::write_long_field(vm, this, "millis", crate::long_arg(args, 1));
        // setMillis is Instant.ofEpochMilli(millis): the sub-millisecond part is zero.
        crate::lang::write_int_field(vm, this, "nanoAdjustment", 0);
        void()
    }),
    method(
        "getInstant",
        "()Ljava/time/Instant;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let millis = crate::lang::read_long_field(vm, this, "millis");
            let nano_adjustment = crate::lang::read_int_field(vm, this, "nanoAdjustment");
            let epoch_second = millis.div_euclid(1000);
            let nano_of_second = (millis.rem_euclid(1000) as i32) * 1_000_000 + nano_adjustment;
            crate::time::instant_new(vm, epoch_second, nano_of_second)
        },
    ),
    method(
        "setInstant",
        "(Ljava/time/Instant;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let instant = ref_arg(args, 1);
            if instant.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let epoch_second = crate::lang::read_long_field(vm, instant, "epochSecond");
            let nano = crate::lang::read_int_field(vm, instant, "nano");
            // Instant.toEpochMilli throws ArithmeticException on overflow.
            let millis = epoch_second
                .checked_mul(1000)
                .and_then(|m| m.checked_add(i64::from(nano / 1_000_000)))
                .ok_or_else(|| {
                    vm.throw_new("java/lang/ArithmeticException", Some("long overflow"))
                })?;
            crate::lang::write_long_field(vm, this, "millis", millis);
            crate::lang::write_int_field(vm, this, "nanoAdjustment", nano % 1_000_000);
            void()
        },
    ),
    method("getSequenceNumber", "()J", ACC_PUBLIC, |vm, _, args| {
        long(crate::lang::read_long_field(vm, receiver(args), "sequence"))
    }),
    method("setSequenceNumber", "(J)V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_long_field(vm, receiver(args), "sequence", crate::long_arg(args, 1));
        void()
    }),
    method("getThreadID", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_long_field(vm, receiver(args), "threadID") as i32)
    }),
    method("setThreadID", "(I)V", ACC_PUBLIC, |vm, _, args| {
        crate::lang::write_long_field(
            vm,
            receiver(args),
            "threadID",
            i64::from(crate::int_arg(args, 1)),
        );
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
        field("nanoAdjustment", "I", ACC_PRIVATE, None),
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

const RESOURCE_BUNDLE_METHODS: [oxjvm_vm::NativeMethodDef; 13] = [
    method(
        "getBundle",
        "(Ljava/lang/String;)Ljava/util/ResourceBundle;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let locale = crate::format::default_locale(vm);
            resource_bundle_get(vm, ref_arg(args, 0), locale)
        },
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
        "getBaseBundleName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "baseBundleName",
                "Ljava/lang/String;",
            ))
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
    method(
        "getLocale",
        "()Ljava/util/Locale;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "locale",
                "Ljava/util/Locale;",
            ))
        },
    ),
    method(
        "getKeys",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "handleKeySet",
        "()Ljava/util/Set;",
        ACC_PROTECTED,
        |vm, _, args| {
            // Default implementation: the keys from getKeys() for which the
            // local handleGetObject returns a non-null value.
            let this = receiver(args);
            let (keys_class, keys_method) =
                vm.resolve_virtual_method(this, "getKeys", "()Ljava/util/Enumeration;")?;
            let enumeration = vm
                .invoke_method(keys_class, keys_method, vec![Value::Ref(this)])?
                .as_ref();
            let candidates = enumeration_elements(vm, enumeration)?;
            let mut keys = Vec::new();
            for key in candidates {
                let (obj_class, obj_method) = vm.resolve_virtual_method(
                    this,
                    "handleGetObject",
                    "(Ljava/lang/String;)Ljava/lang/Object;",
                )?;
                let value = vm
                    .invoke_method(
                        obj_class,
                        obj_method,
                        vec![Value::Ref(this), Value::Ref(key)],
                    )?
                    .as_ref();
                if !value.is_null() {
                    keys.push(key);
                }
            }
            object(crate::util::hash_set_from(vm, &keys)?)
        },
    ),
    method("keySet", "()Ljava/util/Set;", ACC_PUBLIC, |vm, _, args| {
        let mut current = receiver(args);
        let mut keys = Vec::new();
        while !current.is_null() {
            let (class, method) =
                vm.resolve_virtual_method(current, "handleKeySet", "()Ljava/util/Set;")?;
            let local = vm
                .invoke_method(class, method, vec![Value::Ref(current)])?
                .as_ref();
            keys.extend(crate::util::collection_elements(vm, local)?);
            current = record_get_ref(vm, current, "parent", "Ljava/util/ResourceBundle;");
        }
        object(crate::util::hash_set_from(vm, &keys)?)
    }),
    method(
        "containsKey",
        "(Ljava/lang/String;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let key = ref_arg(args, 1);
            if key.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let mut current = receiver(args);
            while !current.is_null() {
                let (class, method) =
                    vm.resolve_virtual_method(current, "handleKeySet", "()Ljava/util/Set;")?;
                let local = vm
                    .invoke_method(class, method, vec![Value::Ref(current)])?
                    .as_ref();
                let (contains_class, contains_method) =
                    vm.resolve_virtual_method(local, "contains", "(Ljava/lang/Object;)Z")?;
                let found = vm
                    .invoke_method(
                        contains_class,
                        contains_method,
                        vec![Value::Ref(local), Value::Ref(key)],
                    )?
                    .as_int();
                if found != 0 {
                    return boolean(true);
                }
                current = record_get_ref(vm, current, "parent", "Ljava/util/ResourceBundle;");
            }
            boolean(false)
        },
    ),
];

pub(crate) const RESOURCE_BUNDLE: oxjvm_vm::NativeClass = class(
    "java/util/ResourceBundle",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field("parent", "Ljava/util/ResourceBundle;", ACC_PRIVATE, None),
        field("baseBundleName", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("locale", "Ljava/util/Locale;", ACC_PRIVATE, None),
    ],
    &RESOURCE_BUNDLE_METHODS,
    None,
);

/// Construct the `Locale` matching a bundle candidate's suffix parts; the
/// empty suffix is `Locale.ROOT`.
fn locale_from_parts(
    vm: &mut Vm<'_>,
    language: &str,
    country: &str,
    variant: &str,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/Locale")?;
    vm.ensure_initialized(class)?;
    if language.is_empty() && country.is_empty() && variant.is_empty() {
        if let Some(root) = vm.read_static_ref_if_set(class, "ROOT", "Ljava/util/Locale;") {
            return Ok(root);
        }
    }
    let language_ref = vm.make_string(language)?;
    let country_ref = vm.make_string(country)?;
    let variant_ref = vm.make_string(variant)?;
    let instance = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(
            class,
            "<init>",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
        )
        .ok_or_else(|| VmError::internal("Locale(String,String,String) missing"))?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![
            Value::Ref(instance),
            Value::Ref(language_ref),
            Value::Ref(country_ref),
            Value::Ref(variant_ref),
        ],
    )?;
    Ok(instance)
}

/// Drain an `Enumeration` into a vector of its elements.
fn enumeration_elements(
    vm: &mut Vm<'_>,
    enumeration: ObjectRef,
) -> Result<Vec<ObjectRef>, VmError> {
    let mut elements = Vec::new();
    if enumeration.is_null() {
        return Ok(elements);
    }
    loop {
        let (has_class, has_method) =
            vm.resolve_virtual_method(enumeration, "hasMoreElements", "()Z")?;
        let more = vm
            .invoke_method(has_class, has_method, vec![Value::Ref(enumeration)])?
            .as_int();
        if more == 0 {
            break;
        }
        let (next_class, next_method) =
            vm.resolve_virtual_method(enumeration, "nextElement", "()Ljava/lang/Object;")?;
        let element = vm
            .invoke_method(next_class, next_method, vec![Value::Ref(enumeration)])?
            .as_ref();
        elements.push(element);
    }
    Ok(elements)
}

/// Append the keys enumerated by a bundle's parent chain, as `getKeys` does.
fn append_parent_keys(
    vm: &mut Vm<'_>,
    bundle: ObjectRef,
    keys: &mut Vec<ObjectRef>,
) -> Result<(), VmError> {
    let parent = record_get_ref(vm, bundle, "parent", "Ljava/util/ResourceBundle;");
    if parent.is_null() {
        return Ok(());
    }
    let (class, method) =
        vm.resolve_virtual_method(parent, "getKeys", "()Ljava/util/Enumeration;")?;
    let enumeration = vm
        .invoke_method(class, method, vec![Value::Ref(parent)])?
        .as_ref();
    keys.extend(enumeration_elements(vm, enumeration)?);
    Ok(())
}

/// Instantiate a single bundle candidate: a class on the classpath, or a
/// `.properties` resource backing a PropertyResourceBundle. Returns `None`
/// when neither exists. The matched locale and base name are recorded.
fn load_bundle_candidate(
    vm: &mut Vm<'_>,
    internal: &str,
    base: &str,
    matched_locale: ObjectRef,
) -> Result<Option<ObjectRef>, VmError> {
    if let Some(class) = vm.resolve_class_lenient(internal) {
        let instance = vm.new_instance(class)?;
        if let Some((declaring, ctor)) = vm.find_method(class, "<init>", "()V") {
            vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
        }
        // getBundle records the base name; a directly instantiated bundle
        // (e.g. `new ListBundle()`) keeps a null base name, as in the JDK.
        let base_ref = vm.make_string(base)?;
        let (declaring, field) = vm.find_field(class, "baseBundleName", "Ljava/lang/String;")?;
        vm.set_instance_ref(instance, declaring, field, base_ref);
        record_set_ref(vm, instance, "locale", "Ljava/util/Locale;", matched_locale);
        return Ok(Some(instance));
    }
    // A .properties resource on the classpath backs a PropertyResourceBundle.
    let resource = format!("{internal}.properties");
    let stream = crate::lang::resource_as_stream(vm, &resource)?;
    if !stream.as_ref().is_null() {
        let props_class = vm.resolve_class("java/util/Properties")?;
        let props = vm.new_instance(props_class)?;
        if let Some((declaring, ctor)) = vm.find_method(props_class, "<init>", "()V") {
            vm.invoke_method(declaring, ctor, vec![Value::Ref(props)])?;
        }
        let (load_class, load_method) =
            vm.resolve_virtual_method(props, "load", "(Ljava/io/InputStream;)V")?;
        vm.invoke_method(load_class, load_method, vec![Value::Ref(props), stream])?;
        let prb_class = vm.resolve_class("java/util/PropertyResourceBundle")?;
        let instance = vm.new_instance(prb_class)?;
        let (declaring, ctor) = vm
            .find_method(prb_class, "<init>", "(Ljava/util/Properties;)V")
            .ok_or_else(|| VmError::internal("PropertyResourceBundle(Properties) missing"))?;
        vm.invoke_method(
            declaring,
            ctor,
            vec![Value::Ref(instance), Value::Ref(props)],
        )?;
        let base_ref = vm.make_string(base)?;
        let (declaring, field) =
            vm.find_field(prb_class, "baseBundleName", "Ljava/lang/String;")?;
        vm.set_instance_ref(instance, declaring, field, base_ref);
        record_set_ref(vm, instance, "locale", "Ljava/util/Locale;", matched_locale);
        return Ok(Some(instance));
    }
    Ok(None)
}

/// Build the fallback candidate list for a locale: the locale's own suffixes
/// (variant, country, language) followed by the unsuffixed base. Each entry
/// carries the locale parts it represents so the match can record its locale.
fn bundle_candidates(
    vm: &mut Vm<'_>,
    base: &str,
    locale: ObjectRef,
) -> Result<Vec<(String, String, String, String)>, VmError> {
    let mut candidates: Vec<(String, String, String, String)> = Vec::new();
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
            candidates.push((
                format!("{base}_{language}_{country}_{variant}"),
                language.clone(),
                country.clone(),
                variant.clone(),
            ));
        }
        if !country.is_empty() {
            candidates.push((
                format!("{base}_{language}_{country}"),
                language.clone(),
                country.clone(),
                String::new(),
            ));
        }
        if !language.is_empty() {
            candidates.push((
                format!("{base}_{language}"),
                language.clone(),
                String::new(),
                String::new(),
            ));
        }
    }
    candidates.push((
        String::from(base),
        String::new(),
        String::new(),
        String::new(),
    ));
    Ok(candidates)
}

/// Load every candidate that exists, in fallback order.
fn load_bundle_candidates(
    vm: &mut Vm<'_>,
    base: &str,
    candidates: Vec<(String, String, String, String)>,
) -> Result<Vec<ObjectRef>, VmError> {
    let mut loaded: Vec<ObjectRef> = Vec::new();
    for (candidate, language, country, variant) in candidates {
        let internal = candidate.replace('.', "/");
        let matched_locale = locale_from_parts(vm, &language, &country, &variant)?;
        if let Some(instance) = load_bundle_candidate(vm, &internal, base, matched_locale)? {
            loaded.push(instance);
        }
    }
    Ok(loaded)
}

/// Whether a bundle's recorded locale is `Locale.ROOT` (the unsuffixed base
/// candidate), used to decide when to try the default-locale search path.
fn bundle_locale_is_root(vm: &mut Vm<'_>, bundle: ObjectRef) -> bool {
    let locale = record_get_ref(vm, bundle, "locale", "Ljava/util/Locale;");
    if locale.is_null() {
        return true;
    }
    let class = vm.class_of(locale);
    let language = vm
        .read_string_field(locale, class, "language")
        .unwrap_or_default();
    let country = vm
        .read_string_field(locale, class, "country")
        .unwrap_or_default();
    let variant = vm
        .read_string_field(locale, class, "variant")
        .unwrap_or_default();
    language.is_empty() && country.is_empty() && variant.is_empty()
}

fn resource_bundle_get(
    vm: &mut Vm<'_>,
    base_object: ObjectRef,
    locale: ObjectRef,
) -> Result<Value, VmError> {
    let base = vm.string_or_empty(base_object);
    // Load every candidate that exists, in fallback order, then link them into
    // a parent chain so keySet/getKeys aggregate the base bundle's keys.
    let candidates = bundle_candidates(vm, &base, locale)?;
    let mut loaded = load_bundle_candidates(vm, &base, candidates)?;
    // The JDK falls back to the default locale's search path when the requested
    // locale yields no bundle, or only the unsuffixed base bundle (a more
    // specific default-locale bundle then becomes the parent of the base).
    let only_base = !loaded.is_empty() && bundle_locale_is_root(vm, loaded[0]);
    if loaded.is_empty() || only_base {
        let default = crate::format::default_locale(vm);
        if !default.is_null() {
            let default_candidates = bundle_candidates(vm, &base, default)?;
            let fallback = load_bundle_candidates(vm, &base, default_candidates)?;
            if !fallback.is_empty() {
                loaded = fallback;
            }
        }
    }
    if !loaded.is_empty() {
        for index in 0..loaded.len() - 1 {
            record_set_ref(
                vm,
                loaded[index],
                "parent",
                "Ljava/util/ResourceBundle;",
                loaded[index + 1],
            );
        }
        return object(loaded[0]);
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

const LIST_RESOURCE_BUNDLE_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "getContents",
        "()[[Ljava/lang/Object;",
        ACC_PROTECTED | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "handleGetObject",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PUBLIC,
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
    method(
        "handleKeySet",
        "()Ljava/util/Set;",
        ACC_PROTECTED,
        |vm, _, args| {
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
        },
    ),
    method(
        "getKeys",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let contents = resource_bundle_contents(vm, this)?;
            let rows = vm.array_length(contents)?;
            let mut keys = Vec::new();
            for index in 0..rows {
                let row = vm.array_get(contents, index as i32)?.as_ref();
                if !row.is_null() {
                    keys.push(vm.array_get(row, 0)?.as_ref());
                }
            }
            append_parent_keys(vm, this, &mut keys)?;
            object(crate::util::new_enumeration(vm, &keys)?)
        },
    ),
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

const PROPERTY_RESOURCE_BUNDLE_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "<init>",
        "(Ljava/util/Properties;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            record_set_ref(
                vm,
                receiver(args),
                "properties",
                "Ljava/util/Properties;",
                ref_arg(args, 1),
            );
            void()
        },
    ),
    method(
        "handleGetObject",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let props = record_get_ref(vm, this, "properties", "Ljava/util/Properties;");
            let key = vm.make_string(&vm.string_or_empty(ref_arg(args, 1)))?;
            let (class, method) = vm.resolve_virtual_method(
                props,
                "getProperty",
                "(Ljava/lang/String;)Ljava/lang/String;",
            )?;
            vm.invoke_method(class, method, vec![Value::Ref(props), Value::Ref(key)])
        },
    ),
    method(
        "<init>",
        "(Ljava/io/InputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let stream = ref_arg(args, 1);
            if stream.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let props_class = vm.resolve_class("java/util/Properties")?;
            let props = vm.new_instance(props_class)?;
            if let Some((declaring, ctor)) = vm.find_method(props_class, "<init>", "()V") {
                vm.invoke_method(declaring, ctor, vec![Value::Ref(props)])?;
            }
            let (load_class, load_method) =
                vm.resolve_virtual_method(props, "load", "(Ljava/io/InputStream;)V")?;
            vm.invoke_method(
                load_class,
                load_method,
                vec![Value::Ref(props), Value::Ref(stream)],
            )?;
            record_set_ref(vm, this, "properties", "Ljava/util/Properties;", props);
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/io/Reader;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let reader = ref_arg(args, 1);
            if reader.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let props_class = vm.resolve_class("java/util/Properties")?;
            let props = vm.new_instance(props_class)?;
            if let Some((declaring, ctor)) = vm.find_method(props_class, "<init>", "()V") {
                vm.invoke_method(declaring, ctor, vec![Value::Ref(props)])?;
            }
            let (load_class, load_method) =
                vm.resolve_virtual_method(props, "load", "(Ljava/io/Reader;)V")?;
            vm.invoke_method(
                load_class,
                load_method,
                vec![Value::Ref(props), Value::Ref(reader)],
            )?;
            record_set_ref(vm, this, "properties", "Ljava/util/Properties;", props);
            void()
        },
    ),
    method(
        "handleKeySet",
        "()Ljava/util/Set;",
        ACC_PROTECTED,
        |vm, _, args| {
            let props = record_get_ref(vm, receiver(args), "properties", "Ljava/util/Properties;");
            let (class, method) =
                vm.resolve_virtual_method(props, "stringPropertyNames", "()Ljava/util/Set;")?;
            vm.invoke_method(class, method, vec![Value::Ref(props)])
        },
    ),
    method(
        "getKeys",
        "()Ljava/util/Enumeration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let props = record_get_ref(vm, this, "properties", "Ljava/util/Properties;");
            let (class, method) =
                vm.resolve_virtual_method(props, "stringPropertyNames", "()Ljava/util/Set;")?;
            let names = vm
                .invoke_method(class, method, vec![Value::Ref(props)])?
                .as_ref();
            let mut keys = crate::util::collection_elements(vm, names)?;
            append_parent_keys(vm, this, &mut keys)?;
            object(crate::util::new_enumeration(vm, &keys)?)
        },
    ),
];

pub(crate) const PROPERTY_RESOURCE_BUNDLE: oxjvm_vm::NativeClass = class(
    "java/util/PropertyResourceBundle",
    Some("java/util/ResourceBundle"),
    &[],
    ACC_PUBLIC,
    &[field(
        "properties",
        "Ljava/util/Properties;",
        ACC_PRIVATE,
        None,
    )],
    &PROPERTY_RESOURCE_BUNDLE_METHODS,
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
        let nano_adjustment = crate::lang::read_int_field(vm, record, "nanoAdjustment");
        let nano_of_second = (millis.rem_euclid(1000) as i32) * 1_000_000 + nano_adjustment;
        // The JDK's default SimpleFormatter format omits sub-second digits; a
        // custom java.util.logging.SimpleFormatter.format with a %tN specifier
        // asks for the nine-digit nano-of-second field.
        let show_nanos =
            crate::lang::system_property(vm, "java.util.logging.SimpleFormatter.format")
                .map(|format| format.contains("tN"))
                .unwrap_or(false);
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
        let mut text = simple_formatter_date(millis, nano_of_second, show_nanos);
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

fn xml_escape(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Render an epoch millisecond timestamp as an ISO-8601 date-time in UTC.
/// With `use_instant` the result is the `ISO_INSTANT` form (trailing `Z` and a
/// nanosecond fraction); otherwise it is the old zone-less `YYYY-MM-DDTHH:MM:SS`.
fn xml_iso_date(millis: i64, nano_of_second: i32, use_instant: bool) -> String {
    let days = millis.div_euclid(86_400_000);
    let secs_of_day = millis.rem_euclid(86_400_000) / 1000;
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    let mut text = format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}");
    if use_instant {
        if nano_of_second != 0 {
            let fraction = format!("{nano_of_second:09}");
            let width = if nano_of_second % 1_000_000 == 0 {
                3
            } else if nano_of_second % 1_000 == 0 {
                6
            } else {
                9
            };
            text.push('.');
            text.push_str(&fraction[..width]);
        }
        text.push('Z');
    }
    text
}

const XML_FORMATTER_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let use_instant =
            match log_manager_get_property(vm, "java.util.logging.XMLFormatter.useInstant")? {
                Some(value) => value != "false",
                None => true,
            };
        crate::lang::write_int_field(vm, this, "useInstant", i32::from(use_instant));
        void()
    }),
    method(
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
            let nano_adjustment = crate::lang::read_int_field(vm, record, "nanoAdjustment");
            let sequence = crate::lang::read_long_field(vm, record, "sequence");
            let use_instant = crate::lang::read_int_field(vm, receiver(args), "useInstant") != 0;
            let nano_of_second = ((millis.rem_euclid(1000)) * 1_000_000) as i32 + nano_adjustment;
            let date = xml_iso_date(millis, nano_of_second, use_instant);
            let mut text = String::from("<record>\n");
            text.push_str(&format!("  <date>{date}</date>\n"));
            text.push_str(&format!("  <millis>{millis}</millis>\n"));
            if use_instant && nano_adjustment != 0 {
                text.push_str(&format!("  <nanos>{nano_adjustment}</nanos>\n"));
            }
            text.push_str(&format!("  <sequence>{sequence}</sequence>\n"));
            text.push_str(&format!("  <level>{}</level>\n", xml_escape(&name)));
            text.push_str(&format!("  <message>{}</message>\n", xml_escape(&message)));
            text.push_str("</record>\n");
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getHead",
        "(Ljava/util/logging/Handler;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let handler = ref_arg(args, 1);
            let mut encoding = String::from("UTF-8");
            if !handler.is_null() {
                let (class, method) =
                    vm.resolve_virtual_method(handler, "getEncoding", "()Ljava/lang/String;")?;
                let value = vm
                    .invoke_method(class, method, vec![Value::Ref(handler)])?
                    .as_ref();
                if !value.is_null() {
                    encoding = vm.string_or_empty(value);
                }
            }
            let text = format!(
                "<?xml version=\"1.0\" encoding=\"{encoding}\" standalone=\"no\"?>\n<!DOCTYPE log SYSTEM \"logger.dtd\">\n<log>\n"
            );
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getTail",
        "(Ljava/util/logging/Handler;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("</log>\n")?),
    ),
];

pub(crate) const XML_FORMATTER: oxjvm_vm::NativeClass = class(
    "java/util/logging/XMLFormatter",
    Some("java/util/logging/Formatter"),
    &[],
    ACC_PUBLIC,
    &[field("useInstant", "I", ACC_PRIVATE, None)],
    &XML_FORMATTER_METHODS,
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

const HANDLER_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
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
            // A null record is never loggable (and must not raise an NPE).
            if record.is_null() {
                return boolean(false);
            }
            let lvl = binder_level(vm, this);
            if lvl == i32::MAX {
                // Level.OFF disables logging entirely.
                return boolean(false);
            }
            let record_level = record_get_ref(vm, record, "level", "Ljava/util/logging/Level;");
            if level_int(vm, record_level) < lvl {
                return boolean(false);
            }
            let filter = record_get_ref(vm, this, "filter", "Ljava/util/logging/Filter;");
            if filter.is_null() {
                return boolean(true);
            }
            let (class, method) = vm.resolve_virtual_method(
                filter,
                "isLoggable",
                "(Ljava/util/logging/LogRecord;)Z",
            )?;
            vm.invoke_method(class, method, vec![Value::Ref(filter), Value::Ref(record)])
        },
    ),
    method(
        "setEncoding",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let encoding = ref_arg(args, 1);
            if !encoding.is_null() {
                let charset_class = vm.resolve_class("java/nio/charset/Charset")?;
                let (declaring, is_supported) = vm
                    .find_method(charset_class, "isSupported", "(Ljava/lang/String;)Z")
                    .ok_or_else(|| VmError::internal("Charset.isSupported missing"))?;
                let supported = vm
                    .invoke_method(declaring, is_supported, vec![Value::Ref(encoding)])?
                    .as_int()
                    != 0;
                if !supported {
                    let name = vm.string_or_empty(encoding);
                    return Err(vm.throw_new("java/io/UnsupportedEncodingException", Some(&name)));
                }
            }
            record_set_ref(vm, this, "encoding", "Ljava/lang/String;", encoding);
            void()
        },
    ),
    method(
        "getEncoding",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "encoding",
                "Ljava/lang/String;",
            ))
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
        field("encoding", "Ljava/lang/String;", ACC_PRIVATE, None),
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

const STREAM_HANDLER_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
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
            let this = receiver(args);
            let record = ref_arg(args, 1);
            // StreamHandler.publish drops records that are not loggable (a null
            // record included) before formatting them.
            let (class, method) =
                vm.resolve_virtual_method(this, "isLoggable", "(Ljava/util/logging/LogRecord;)Z")?;
            let loggable = vm
                .invoke_method(class, method, vec![Value::Ref(this), Value::Ref(record)])?
                .as_int();
            if loggable == 0 {
                return void();
            }
            publish_to_output(vm, this, record)?;
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
    method(
        "setEncoding",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let handler_class = vm.resolve_class(HANDLER_CLASS)?;
            let (declaring, set_encoding) = vm
                .find_method(handler_class, "setEncoding", "(Ljava/lang/String;)V")
                .ok_or_else(|| VmError::internal("Handler.setEncoding missing"))?;
            vm.invoke_method(
                declaring,
                set_encoding,
                vec![Value::Ref(this), Value::Ref(ref_arg(args, 1))],
            )?;
            let (flush_class, flush) = vm.resolve_virtual_method(this, "flush", "()V")?;
            vm.invoke_method(flush_class, flush, vec![Value::Ref(this)])?;
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

/// Push every buffered record to the target handler and empty the buffer.
fn memory_handler_push(vm: &mut Vm<'_>, handler: ObjectRef) -> Result<(), VmError> {
    let buffer = record_get_ref(vm, handler, "buffer", "Ljava/util/ArrayList;");
    let target = record_get_ref(vm, handler, "target", "Ljava/util/logging/Handler;");
    let records = crate::util::array_list_elements(vm, buffer);
    for record in records {
        let (class, method) =
            vm.resolve_virtual_method(target, "publish", "(Ljava/util/logging/LogRecord;)V")?;
        vm.invoke_method(class, method, vec![Value::Ref(target), Value::Ref(record)])?;
    }
    crate::util::array_list_store(vm, buffer, &[])?;
    Ok(())
}

const MEMORY_HANDLER_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "<init>",
        "(Ljava/util/logging/Handler;ILjava/util/logging/Level;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let target = ref_arg(args, 1);
            let size = crate::int_arg(args, 2);
            let push_level = ref_arg(args, 3);
            if target.is_null() || push_level.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            if size <= 0 {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            }
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
            record_set_ref(vm, this, "target", "Ljava/util/logging/Handler;", target);
            record_set_ref(
                vm,
                this,
                "pushLevel",
                "Ljava/util/logging/Level;",
                push_level,
            );
            crate::lang::write_int_field(vm, this, "size", size);
            let buffer = crate::util::new_array_list(vm)?;
            record_set_ref(vm, this, "buffer", "Ljava/util/ArrayList;", buffer);
            void()
        },
    ),
    method(
        "publish",
        "(Ljava/util/logging/LogRecord;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let record = ref_arg(args, 1);
            let (class, method) =
                vm.resolve_virtual_method(this, "isLoggable", "(Ljava/util/logging/LogRecord;)Z")?;
            let loggable = vm
                .invoke_method(class, method, vec![Value::Ref(this), Value::Ref(record)])?
                .as_int();
            if loggable == 0 {
                return void();
            }
            let buffer = record_get_ref(vm, this, "buffer", "Ljava/util/ArrayList;");
            hlist_add(vm, buffer, record)?;
            // The buffer is circular: once full, the oldest record is dropped.
            let size = crate::lang::read_int_field(vm, this, "size");
            let len = hlist_len(vm, buffer)?;
            if len > size {
                let records = crate::util::array_list_elements(vm, buffer);
                let keep = &records[(len - size) as usize..];
                crate::util::array_list_store(vm, buffer, keep)?;
            }
            let record_level = record_get_ref(vm, record, "level", "Ljava/util/logging/Level;");
            let push_level = record_get_ref(vm, this, "pushLevel", "Ljava/util/logging/Level;");
            if level_int(vm, record_level) >= level_int(vm, push_level) {
                memory_handler_push(vm, this)?;
            }
            void()
        },
    ),
    method("push", "()V", ACC_PUBLIC, |vm, _, args| {
        memory_handler_push(vm, receiver(args))?;
        void()
    }),
    method("flush", "()V", ACC_PUBLIC, |vm, _, args| {
        let target = record_get_ref(vm, receiver(args), "target", "Ljava/util/logging/Handler;");
        let (class, method) = vm.resolve_virtual_method(target, "flush", "()V")?;
        vm.invoke_method(class, method, vec![Value::Ref(target)])?;
        void()
    }),
    method("close", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let target = record_get_ref(vm, this, "target", "Ljava/util/logging/Handler;");
        let (class, method) = vm.resolve_virtual_method(target, "close", "()V")?;
        vm.invoke_method(class, method, vec![Value::Ref(target)])?;
        let off = level_named(vm, "OFF")?;
        record_set_ref(vm, this, "level", "Ljava/util/logging/Level;", off);
        void()
    }),
    method(
        "setPushLevel",
        "(Ljava/util/logging/Level;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let new_level = ref_arg(args, 1);
            if new_level.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            record_set_ref(
                vm,
                receiver(args),
                "pushLevel",
                "Ljava/util/logging/Level;",
                new_level,
            );
            void()
        },
    ),
    method(
        "getPushLevel",
        "()Ljava/util/logging/Level;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(record_get_ref(
                vm,
                receiver(args),
                "pushLevel",
                "Ljava/util/logging/Level;",
            ))
        },
    ),
];

pub(crate) const MEMORY_HANDLER: oxjvm_vm::NativeClass = class(
    "java/util/logging/MemoryHandler",
    Some(HANDLER_CLASS),
    &[],
    ACC_PUBLIC,
    &[
        field("target", "Ljava/util/logging/Handler;", ACC_PRIVATE, None),
        field("pushLevel", "Ljava/util/logging/Level;", ACC_PRIVATE, None),
        field("size", "I", ACC_PRIVATE, None),
        field("buffer", "Ljava/util/ArrayList;", ACC_PRIVATE, None),
    ],
    &MEMORY_HANDLER_METHODS,
    None,
);

/// Shared `FileHandler` constructor body: validate the pattern/limit/count the
/// way the JDK does, then open the backing file and install the default
/// formatter.
fn file_handler_configure(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    pattern: ObjectRef,
    limit: i32,
    count: i32,
    append: bool,
) -> Result<(), VmError> {
    if pattern.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    if limit < 0 {
        return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("limit < 0")));
    }
    if count < 1 {
        return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("count < 1")));
    }
    if vm.string_or_empty(pattern).is_empty() {
        return Err(vm.throw_new("java/lang/IllegalArgumentException", Some("empty pattern")));
    }
    let stream_class = vm.resolve_class("java/io/FileOutputStream")?;
    let stream = vm.new_instance(stream_class)?;
    let (declaring, ctor) = vm
        .find_method(stream_class, "<init>", "(Ljava/lang/String;Z)V")
        .ok_or_else(|| VmError::internal("FileOutputStream(String,boolean) missing"))?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![Value::Ref(stream), Value::Ref(pattern), boolean(append)?],
    )?;
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
    record_set_ref(vm, this, "output", "Ljava/lang/Object;", stream);
    Ok(())
}

const FILE_HANDLER_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let pattern = match log_manager_get_property(vm, "java.util.logging.FileHandler.pattern")? {
            Some(value) => vm.make_string(&value)?,
            None => ObjectRef::NULL,
        };
        let limit = log_manager_get_property(vm, "java.util.logging.FileHandler.limit")?
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(0);
        let count = log_manager_get_property(vm, "java.util.logging.FileHandler.count")?
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(1);
        let append = log_manager_get_property(vm, "java.util.logging.FileHandler.append")?
            .map(|value| value == "true")
            .unwrap_or(false);
        file_handler_configure(vm, this, pattern, limit, count, append)?;
        void()
    }),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            file_handler_configure(vm, receiver(args), ref_arg(args, 1), 0, 1, false)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Z)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let append = args[2].as_int() != 0;
            file_handler_configure(vm, receiver(args), ref_arg(args, 1), 0, 1, append)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;II)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let limit = args[2].as_int();
            let count = args[3].as_int();
            file_handler_configure(vm, receiver(args), ref_arg(args, 1), limit, count, false)?;
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;IIZ)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let limit = args[2].as_int();
            let count = args[3].as_int();
            let append = args[4].as_int() != 0;
            file_handler_configure(vm, receiver(args), ref_arg(args, 1), limit, count, append)?;
            void()
        },
    ),
];

pub(crate) const FILE_HANDLER: oxjvm_vm::NativeClass = class(
    "java/util/logging/FileHandler",
    Some("java/util/logging/StreamHandler"),
    &[],
    ACC_PUBLIC,
    &[],
    &FILE_HANDLER_METHODS,
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

/// Attach the effective resource bundle to a record, mirroring the JDK
/// `Logger.getEffectiveLoggerBundle`: this logger's overridable
/// `getResourceBundle()`/`getResourceBundleName()`, or the nearest ancestor's
/// stored bundle (or a bundle looked up by the ancestor's overridable name).
fn attach_effective_bundle(
    vm: &mut Vm<'_>,
    logger: ObjectRef,
    record: ObjectRef,
) -> Result<(), VmError> {
    let (bundle_class, bundle_method) =
        vm.resolve_virtual_method(logger, "getResourceBundle", "()Ljava/util/ResourceBundle;")?;
    let bundle = vm
        .invoke_method(bundle_class, bundle_method, vec![Value::Ref(logger)])?
        .as_ref();
    if !bundle.is_null() {
        let stored = record_get_ref(vm, logger, "resourceBundle", "Ljava/util/ResourceBundle;");
        let name = if bundle == stored {
            // The stored bundle is returned unchanged: keep the stored name.
            record_get_ref(vm, logger, "resourceBundleName", "Ljava/lang/String;")
        } else {
            // getResourceBundle() is overridden (or no bundle is stored): pair
            // it with the overridable getResourceBundleName().
            let (name_class, name_method) =
                vm.resolve_virtual_method(logger, "getResourceBundleName", "()Ljava/lang/String;")?;
            vm.invoke_method(name_class, name_method, vec![Value::Ref(logger)])?
                .as_ref()
        };
        record_set_ref(vm, record, "resourceBundleName", "Ljava/lang/String;", name);
        record_set_ref(
            vm,
            record,
            "resourceBundle",
            "Ljava/util/ResourceBundle;",
            bundle,
        );
        return Ok(());
    }
    // No bundle on this logger: walk the parent stack.
    let mut current = record_get_ref(vm, logger, "parent", "Ljava/util/logging/Logger;");
    while !current.is_null() {
        let parent_bundle =
            record_get_ref(vm, current, "resourceBundle", "Ljava/util/ResourceBundle;");
        if !parent_bundle.is_null() {
            let parent_name =
                record_get_ref(vm, current, "resourceBundleName", "Ljava/lang/String;");
            record_set_ref(
                vm,
                record,
                "resourceBundleName",
                "Ljava/lang/String;",
                parent_name,
            );
            record_set_ref(
                vm,
                record,
                "resourceBundle",
                "Ljava/util/ResourceBundle;",
                parent_bundle,
            );
            return Ok(());
        }
        // A name without a stored bundle means the bundle is looked up by the
        // ancestor's overridable getResourceBundleName().
        let (name_class, name_method) =
            vm.resolve_virtual_method(current, "getResourceBundleName", "()Ljava/lang/String;")?;
        let name_value = vm.invoke_method(name_class, name_method, vec![Value::Ref(current)])?;
        let name = vm.string_or_empty(name_value.as_ref());
        if !name.is_empty() {
            let name_ref = vm.make_string(&name)?;
            let found = resource_bundle_get(vm, name_ref, ObjectRef::NULL)?;
            record_set_ref(
                vm,
                record,
                "resourceBundleName",
                "Ljava/lang/String;",
                name_ref,
            );
            record_set_ref(
                vm,
                record,
                "resourceBundle",
                "Ljava/util/ResourceBundle;",
                found.as_ref(),
            );
            return Ok(());
        }
        current = record_get_ref(vm, current, "parent", "Ljava/util/logging/Logger;");
    }
    Ok(())
}

fn publish_from_logger(
    vm: &mut Vm<'_>,
    logger: ObjectRef,
    record: ObjectRef,
) -> Result<(), VmError> {
    // getHandlers() is overridable, so call it virtually rather than reading
    // the handler list field directly.
    let (handlers_class, handlers_method) =
        vm.resolve_virtual_method(logger, "getHandlers", "()[Ljava/util/logging/Handler;")?;
    let handlers = vm
        .invoke_method(handlers_class, handlers_method, vec![Value::Ref(logger)])?
        .as_ref();
    let count = vm.array_length(handlers)?;
    for index in 0..count {
        let handler = vm.array_get(handlers, index as i32)?.as_ref();
        if handler.is_null() {
            continue;
        }
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

fn log_manager_properties(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(LOG_MANAGER_CLASS)?;
    if let Some(map) = vm.read_static_ref_if_set(class, "properties", "Ljava/util/HashMap;") {
        return Ok(map);
    }
    vm.ensure_initialized(class)?;
    vm.read_static_ref_if_set(class, "properties", "Ljava/util/HashMap;")
        .ok_or_else(|| VmError::internal("LogManager.properties missing"))
}

fn log_manager_get_property(vm: &mut Vm<'_>, name: &str) -> Result<Option<String>, VmError> {
    let map = log_manager_properties(vm)?;
    let key = vm.make_string(name)?;
    let value = crate::util::hash_map_lookup(vm, map, key)?;
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(vm.string_or_empty(value)))
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
    // Link to the nearest extant ancestor in the namespace (the JDK's
    // getParent), falling back to the root logger (name "").
    if !name.is_empty() {
        let mut parent = logger_from_registry(vm, "")?;
        let mut candidate = name;
        while let Some(dot) = candidate.rfind('.') {
            candidate = &candidate[..dot];
            let ancestor_key = vm.make_string(candidate)?;
            let found = crate::util::hash_map_lookup(vm, map, ancestor_key)?;
            if !found.is_null() {
                parent = found;
                break;
            }
        }
        record_set_ref(vm, logger, "parent", "Ljava/util/logging/Logger;", parent);
    }
    crate::util::hash_map_insert(vm, map, key, logger)?;
    Ok(logger)
}

/// Global monotonically increasing sequence counter for log records, as the
/// JDK's LogRecord assigns from a static AtomicLong.
fn next_sequence_number() -> i64 {
    use core::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    SEQUENCE.fetch_add(1, Ordering::Relaxed) as i64
}

/// Give a freshly created record its sequence number and the current thread id.
fn stamp_record(vm: &mut Vm<'_>, record: ObjectRef) {
    crate::lang::write_long_field(vm, record, "sequence", next_sequence_number());
    let thread = vm.current_thread_object();
    crate::lang::write_long_field(vm, record, "threadID", vm.thread_id(thread));
}

/// Infer the calling class and method by skipping the logging framework's own
/// frames, mirroring LogRecord.inferCaller.
fn infer_caller_source(vm: &mut Vm<'_>) -> Result<(ObjectRef, ObjectRef), VmError> {
    let frames = vm.capture_frames();
    for frame in &frames {
        let dotted = frame.class_name.replace('/', ".");
        if dotted.starts_with("java.util.logging") {
            continue;
        }
        let class_ref = vm.make_string(&dotted)?;
        let method_ref = vm.make_string(&frame.method_name)?;
        return Ok((class_ref, method_ref));
    }
    Ok((ObjectRef::NULL, ObjectRef::NULL))
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
    stamp_record(vm, record);
    let (source_class, source_method) = infer_caller_source(vm)?;
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
    // Convenience methods attach the logger's effective bundle, as doLog(lr) does.
    attach_effective_bundle(vm, logger, record)?;
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
    stamp_record(vm, record);
    let name = logger_name(vm, logger);
    let name_ref = vm.make_string(&name)?;
    record_set_ref(vm, record, "loggerName", "Ljava/lang/String;", name_ref);
    // logp attaches the logger's effective bundle, as doLog(lr) does.
    attach_effective_bundle(vm, logger, record)?;
    publish_from_logger(vm, logger, record)
}

/// Publish a record localized with a resource bundle, as the `logrb` family
/// does. Either a bundle instance or a bundle name is supplied; the other is
/// null. Mirrors Logger.doLog(LogRecord, ResourceBundle) / (LogRecord, String).
#[allow(clippy::too_many_arguments)]
fn logger_logrb(
    vm: &mut Vm<'_>,
    logger: ObjectRef,
    level: ObjectRef,
    source_class: ObjectRef,
    source_method: ObjectRef,
    bundle: ObjectRef,
    bundle_name: ObjectRef,
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
    // Overloads without an explicit source infer the caller, as the LogRecord
    // constructor does.
    let (source_class, source_method) = if source_class.is_null() {
        infer_caller_source(vm)?
    } else {
        (source_class, source_method)
    };
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
    stamp_record(vm, record);
    let name = logger_name(vm, logger);
    let name_ref = vm.make_string(&name)?;
    record_set_ref(vm, record, "loggerName", "Ljava/lang/String;", name_ref);
    if !bundle.is_null() {
        let (base_class, base_method) =
            vm.resolve_virtual_method(bundle, "getBaseBundleName", "()Ljava/lang/String;")?;
        let base_name = vm
            .invoke_method(base_class, base_method, vec![Value::Ref(bundle)])?
            .as_ref();
        record_set_ref(
            vm,
            record,
            "resourceBundleName",
            "Ljava/lang/String;",
            base_name,
        );
        record_set_ref(
            vm,
            record,
            "resourceBundle",
            "Ljava/util/ResourceBundle;",
            bundle,
        );
    } else if !bundle_name.is_null() {
        record_set_ref(
            vm,
            record,
            "resourceBundleName",
            "Ljava/lang/String;",
            bundle_name,
        );
        let loaded = resource_bundle_get(vm, bundle_name, ObjectRef::NULL)?.as_ref();
        record_set_ref(
            vm,
            record,
            "resourceBundle",
            "Ljava/util/ResourceBundle;",
            loaded,
        );
    }
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
fn simple_formatter_date(millis: i64, nano_of_second: i32, show_nanos: bool) -> String {
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
    if show_nanos {
        format!(
            "{month_name} {day}, {year} {hour12}:{minute:02}:{second:02}.{nano_of_second:09} {ampm}"
        )
    } else {
        format!("{month_name} {day}, {year} {hour12}:{minute:02}:{second:02} {ampm}")
    }
}

fn level_named(vm: &mut Vm<'_>, name: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(LEVEL_CLASS)?;
    vm.ensure_initialized(class)?;
    vm.read_static_ref_if_set(class, name, &format!("L{LEVEL_CLASS};"))
        .ok_or_else(|| VmError::internal("Level constant missing"))
}

const LOGGER_METHODS: [oxjvm_vm::NativeMethodDef; 51] = [
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
            let logger = logger_from_registry(vm, &name)?;
            let requested = ref_arg(args, 1);
            let existing = record_get_ref(vm, logger, "resourceBundleName", "Ljava/lang/String;");
            if !existing.is_null() {
                // A logger that already has a bundle keeps it; a different (or
                // null) requested name is rejected, as in setupResourceInfo.
                let existing_name = vm.string_or_empty(existing);
                let requested_name = vm.string_or_empty(requested);
                if requested.is_null() || existing_name != requested_name {
                    let shown = if requested.is_null() {
                        String::from("null")
                    } else {
                        requested_name
                    };
                    return Err(vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some(&format!("{existing_name} != {shown}")),
                    ));
                }
            } else if !requested.is_null() {
                // No bundle yet: load the requested bundle and record its name.
                let bundle = resource_bundle_get(vm, requested, ObjectRef::NULL)?;
                record_set_ref(
                    vm,
                    logger,
                    "resourceBundle",
                    "Ljava/util/ResourceBundle;",
                    bundle.as_ref(),
                );
                record_set_ref(
                    vm,
                    logger,
                    "resourceBundleName",
                    "Ljava/lang/String;",
                    requested,
                );
            }
            object(logger)
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
            // The JDK dereferences the bundle to read its base name, so a null
            // bundle raises a NullPointerException.
            if bundle.is_null() {
                return Err(vm.throw_new(
                    "java/lang/NullPointerException",
                    Some(
                        "Cannot invoke \"java.util.ResourceBundle.getBaseBundleName()\" \
                         because \"bundle\" is null",
                    ),
                ));
            }
            let (base_class, base_method) =
                vm.resolve_virtual_method(bundle, "getBaseBundleName", "()Ljava/lang/String;")?;
            let base_value = vm.invoke_method(base_class, base_method, vec![Value::Ref(bundle)])?;
            let base_name = vm.string_or_empty(base_value.as_ref());
            if base_name.is_empty() {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("resource bundle must have a name"),
                ));
            }
            let existing = record_get_ref(vm, this, "resourceBundleName", "Ljava/lang/String;");
            let can_replace = existing.is_null() || vm.string_or_empty(existing) == base_name;
            if !can_replace {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("can't replace resource bundle"),
                ));
            }
            record_set_ref(
                vm,
                this,
                "resourceBundle",
                "Ljava/util/ResourceBundle;",
                bundle,
            );
            let base_ref = vm.make_string(&base_name)?;
            record_set_ref(
                vm,
                this,
                "resourceBundleName",
                "Ljava/lang/String;",
                base_ref,
            );
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
    method(
        "logrb",
        "(Ljava/util/logging/Level;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            logger_logrb(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ref_arg(args, 3),
                ObjectRef::NULL,
                ref_arg(args, 4),
                ref_arg(args, 5),
                ObjectRef::NULL,
                ObjectRef::NULL,
            )?;
            void()
        },
    ),
    method(
        "logrb",
        "(Ljava/util/logging/Level;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let params = single_param_array(vm, ref_arg(args, 6))?;
            logger_logrb(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ref_arg(args, 3),
                ObjectRef::NULL,
                ref_arg(args, 4),
                ref_arg(args, 5),
                params,
                ObjectRef::NULL,
            )?;
            void()
        },
    ),
    method(
        "logrb",
        "(Ljava/util/logging/Level;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;[Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            logger_logrb(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ref_arg(args, 3),
                ObjectRef::NULL,
                ref_arg(args, 4),
                ref_arg(args, 5),
                ref_arg(args, 6),
                ObjectRef::NULL,
            )?;
            void()
        },
    ),
    method(
        "logrb",
        "(Ljava/util/logging/Level;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/Throwable;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            logger_logrb(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ref_arg(args, 3),
                ObjectRef::NULL,
                ref_arg(args, 4),
                ref_arg(args, 5),
                ObjectRef::NULL,
                ref_arg(args, 6),
            )?;
            void()
        },
    ),
    method(
        "logrb",
        "(Ljava/util/logging/Level;Ljava/lang/String;Ljava/lang/String;Ljava/util/ResourceBundle;Ljava/lang/String;[Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            logger_logrb(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ref_arg(args, 3),
                ref_arg(args, 4),
                ObjectRef::NULL,
                ref_arg(args, 5),
                ref_arg(args, 6),
                ObjectRef::NULL,
            )?;
            void()
        },
    ),
    method(
        "logrb",
        "(Ljava/util/logging/Level;Ljava/util/ResourceBundle;Ljava/lang/String;[Ljava/lang/Object;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            logger_logrb(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ObjectRef::NULL,
                ObjectRef::NULL,
                ref_arg(args, 2),
                ObjectRef::NULL,
                ref_arg(args, 3),
                ref_arg(args, 4),
                ObjectRef::NULL,
            )?;
            void()
        },
    ),
    method(
        "logrb",
        "(Ljava/util/logging/Level;Ljava/lang/String;Ljava/lang/String;Ljava/util/ResourceBundle;Ljava/lang/String;Ljava/lang/Throwable;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            logger_logrb(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ref_arg(args, 2),
                ref_arg(args, 3),
                ref_arg(args, 4),
                ObjectRef::NULL,
                ref_arg(args, 5),
                ObjectRef::NULL,
                ref_arg(args, 6),
            )?;
            void()
        },
    ),
    method(
        "logrb",
        "(Ljava/util/logging/Level;Ljava/util/ResourceBundle;Ljava/lang/String;Ljava/lang/Throwable;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            logger_logrb(
                vm,
                receiver(args),
                ref_arg(args, 1),
                ObjectRef::NULL,
                ObjectRef::NULL,
                ref_arg(args, 2),
                ObjectRef::NULL,
                ref_arg(args, 3),
                ObjectRef::NULL,
                ref_arg(args, 4),
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
    let properties = crate::util::hash_map_new(vm)?;
    vm.set_static_value(
        class,
        "properties",
        "Ljava/util/HashMap;",
        Value::Ref(properties),
    );
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

const LOG_MANAGER_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
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
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 1));
            match log_manager_get_property(vm, &name)? {
                Some(value) => object(vm.make_string(&value)?),
                None => object(ObjectRef::NULL),
            }
        },
    ),
    method(
        "getProperty",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 1));
            match log_manager_get_property(vm, &name)? {
                Some(value) => object(vm.make_string(&value)?),
                None => object(ref_arg(args, 2)),
            }
        },
    ),
    method(
        "updateConfiguration",
        "(Ljava/io/InputStream;Ljava/util/function/Function;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let ins = ref_arg(args, 1);
            let mapper = ref_arg(args, 2);
            if ins.is_null() || mapper.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let props_class = vm.resolve_class("java/util/Properties")?;
            let props = vm.new_instance(props_class)?;
            let (props_ctor_class, props_ctor) = vm
                .find_method(props_class, "<init>", "()V")
                .ok_or_else(|| VmError::internal("Properties() missing"))?;
            vm.invoke_method(props_ctor_class, props_ctor, vec![Value::Ref(props)])?;
            let (load_class, load) = vm
                .find_method(props_class, "load", "(Ljava/io/InputStream;)V")
                .ok_or_else(|| VmError::internal("Properties.load missing"))?;
            vm.invoke_method(load_class, load, vec![Value::Ref(props), Value::Ref(ins)])?;
            let (names_class, names_method) =
                vm.resolve_virtual_method(props, "stringPropertyNames", "()Ljava/util/Set;")?;
            let names = vm
                .invoke_method(names_class, names_method, vec![Value::Ref(props)])?
                .as_ref();
            let store = log_manager_properties(vm)?;
            for key in crate::util::collection_elements(vm, names)? {
                let (apply_class, apply) = vm.resolve_virtual_method(
                    mapper,
                    "apply",
                    "(Ljava/lang/Object;)Ljava/lang/Object;",
                )?;
                let merger = vm
                    .invoke_method(
                        apply_class,
                        apply,
                        vec![Value::Ref(mapper), Value::Ref(key)],
                    )?
                    .as_ref();
                let name = vm.string_or_empty(key);
                let old = match log_manager_get_property(vm, &name)? {
                    Some(value) => vm.make_string(&value)?,
                    None => ObjectRef::NULL,
                };
                let (get_class, get) = vm.resolve_virtual_method(
                    props,
                    "getProperty",
                    "(Ljava/lang/String;)Ljava/lang/String;",
                )?;
                let new = vm
                    .invoke_method(get_class, get, vec![Value::Ref(props), Value::Ref(key)])?
                    .as_ref();
                let (merge_class, merge) = vm.resolve_virtual_method(
                    merger,
                    "apply",
                    "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                )?;
                let merged = vm
                    .invoke_method(
                        merge_class,
                        merge,
                        vec![Value::Ref(merger), Value::Ref(old), Value::Ref(new)],
                    )?
                    .as_ref();
                if !merged.is_null() {
                    crate::util::hash_map_insert(vm, store, key, merged)?;
                }
            }
            void()
        },
    ),
    method("readConfiguration", "()V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "readConfiguration",
        "(Ljava/io/InputStream;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let ins = ref_arg(args, 1);
            if ins.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let props_class = vm.resolve_class("java/util/Properties")?;
            let props = vm.new_instance(props_class)?;
            let (ctor_class, ctor) = vm
                .find_method(props_class, "<init>", "()V")
                .ok_or_else(|| VmError::internal("Properties() missing"))?;
            vm.invoke_method(ctor_class, ctor, vec![Value::Ref(props)])?;
            let (load_class, load) = vm
                .find_method(props_class, "load", "(Ljava/io/InputStream;)V")
                .ok_or_else(|| VmError::internal("Properties.load missing"))?;
            vm.invoke_method(load_class, load, vec![Value::Ref(props), Value::Ref(ins)])?;
            let (names_class, names_method) =
                vm.resolve_virtual_method(props, "stringPropertyNames", "()Ljava/util/Set;")?;
            let names = vm
                .invoke_method(names_class, names_method, vec![Value::Ref(props)])?
                .as_ref();
            let store = log_manager_properties(vm)?;
            for key in crate::util::collection_elements(vm, names)? {
                let (get_class, get) = vm.resolve_virtual_method(
                    props,
                    "getProperty",
                    "(Ljava/lang/String;)Ljava/lang/String;",
                )?;
                let value = vm
                    .invoke_method(get_class, get, vec![Value::Ref(props), Value::Ref(key)])?
                    .as_ref();
                if !value.is_null() {
                    crate::util::hash_map_insert(vm, store, key, value)?;
                }
            }
            void()
        },
    ),
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
            "properties",
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
