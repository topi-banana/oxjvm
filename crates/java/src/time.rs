//! A minimal `java.time.Duration` / `java.time.Instant` (nanosecond precision, no calendars).

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::lang::{read_int_field, read_long_field, write_int_field, write_long_field};
use crate::{class, field, int, long, method, object, receiver, ref_arg};

const DURATION_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
    method(
        "ofNanos",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| duration_new(vm, crate::long_arg(args, 0)),
    ),
    method(
        "ofMillis",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| duration_new(vm, crate::long_arg(args, 0).saturating_mul(1_000_000)),
    ),
    method(
        "ofSeconds",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| duration_new(vm, crate::long_arg(args, 0).saturating_mul(1_000_000_000)),
    ),
    method("toNanos", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "nanos"))
    }),
    method("toMillis", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "nanos") / 1_000_000)
    }),
    method("getSeconds", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "nanos") / 1_000_000_000)
    }),
    method("getNano", "()I", ACC_PUBLIC, |vm, _, args| {
        int((read_long_field(vm, receiver(args), "nanos") % 1_000_000_000) as i32)
    }),
    method("isZero", "()Z", ACC_PUBLIC, |vm, _, args| {
        crate::boolean(read_long_field(vm, receiver(args), "nanos") == 0)
    }),
    method("isNegative", "()Z", ACC_PUBLIC, |vm, _, args| {
        crate::boolean(read_long_field(vm, receiver(args), "nanos") < 0)
    }),
    method(
        "plusMillis",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let nanos = read_long_field(vm, receiver(args), "nanos")
                .saturating_add(crate::long_arg(args, 1).saturating_mul(1_000_000));
            duration_new(vm, nanos)
        },
    ),
    method(
        "minusMillis",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let nanos = read_long_field(vm, receiver(args), "nanos")
                .saturating_sub(crate::long_arg(args, 1).saturating_mul(1_000_000));
            duration_new(vm, nanos)
        },
    ),
    method(
        "plusNanos",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let nanos = read_long_field(vm, receiver(args), "nanos")
                .saturating_add(crate::long_arg(args, 1));
            duration_new(vm, nanos)
        },
    ),
    method(
        "minusNanos",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let nanos = read_long_field(vm, receiver(args), "nanos")
                .saturating_sub(crate::long_arg(args, 1));
            duration_new(vm, nanos)
        },
    ),
    method(
        "compareTo",
        "(Ljava/time/Duration;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = read_long_field(vm, receiver(args), "nanos");
            let other = read_long_field(vm, ref_arg(args, 1), "nanos");
            int(this.cmp(&other) as i32)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() {
                return crate::boolean(false);
            }
            let class = vm.resolve_class("java/time/Duration")?;
            if !vm.is_instance(other, class) {
                return crate::boolean(false);
            }
            crate::boolean(
                read_long_field(vm, receiver(args), "nanos") == read_long_field(vm, other, "nanos"),
            )
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let nanos = read_long_field(vm, receiver(args), "nanos");
        int((nanos ^ (nanos >> 32)) as i32)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let nanos = read_long_field(vm, receiver(args), "nanos");
            let text = format_duration(nanos);
            object(vm.make_string(&text)?)
        },
    ),
];

fn format_duration(nanos: i64) -> alloc::string::String {
    let seconds = nanos / 1_000_000_000;
    let fraction = nanos % 1_000_000_000;
    if fraction == 0 {
        alloc::format!("PT{}S", oxjvm_vm::format::long_to_string(seconds))
    } else {
        let mut text = oxjvm_vm::format::long_to_string(fraction);
        while text.len() < 9 {
            text.insert(0, '0');
        }
        while text.ends_with('0') {
            text.pop();
        }
        alloc::format!("PT{}.{}S", oxjvm_vm::format::long_to_string(seconds), text)
    }
}

fn duration_new(vm: &mut Vm<'_>, nanos: i64) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/time/Duration")?;
    let instance = vm.new_instance(class)?;
    write_long_field(vm, instance, "nanos", nanos);
    object(instance)
}

/// `java.time.Duration`, stored as a single nanosecond count.
fn duration_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/time/Duration")?;
    let zero = duration_new(vm, 0)?.as_ref();
    vm.set_static_value(class, "ZERO", "Ljava/time/Duration;", Value::Ref(zero));
    Ok(())
}

pub(crate) const DURATION: oxjvm_vm::NativeClass = class(
    "java/time/Duration",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("nanos", "J", ACC_PRIVATE, None),
        field(
            "ZERO",
            "Ljava/time/Duration;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &DURATION_METHODS,
    Some(duration_clinit),
);

const INSTANT_METHODS: [oxjvm_vm::NativeMethodDef; 13] = [
    method(
        "now",
        "()Ljava/time/Instant;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let millis = vm.host_time_millis();
            instant_new(
                vm,
                millis.div_euclid(1000),
                (millis.rem_euclid(1000) * 1_000_000) as i32,
            )
        },
    ),
    method(
        "ofEpochMilli",
        "(J)Ljava/time/Instant;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let millis = crate::long_arg(args, 0);
            instant_new(
                vm,
                millis.div_euclid(1000),
                (millis.rem_euclid(1000) * 1_000_000) as i32,
            )
        },
    ),
    method(
        "ofEpochSecond",
        "(J)Ljava/time/Instant;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| instant_new(vm, crate::long_arg(args, 0), 0),
    ),
    method("getEpochSecond", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "epochSecond"))
    }),
    method("getNano", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "nano"))
    }),
    method("toEpochMilli", "()J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        long(
            read_long_field(vm, this, "epochSecond").saturating_mul(1000)
                + i64::from(read_int_field(vm, this, "nano")) / 1_000_000,
        )
    }),
    method(
        "plusMillis",
        "(J)Ljava/time/Instant;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let millis = read_long_field(vm, this, "epochSecond")
                .saturating_mul(1000)
                .saturating_add(i64::from(read_int_field(vm, this, "nano")) / 1_000_000)
                .saturating_add(crate::long_arg(args, 1));
            instant_new(
                vm,
                millis.div_euclid(1000),
                (millis.rem_euclid(1000) * 1_000_000) as i32,
            )
        },
    ),
    method(
        "minusMillis",
        "(J)Ljava/time/Instant;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let millis = read_long_field(vm, this, "epochSecond")
                .saturating_mul(1000)
                .saturating_add(i64::from(read_int_field(vm, this, "nano")) / 1_000_000)
                .saturating_sub(crate::long_arg(args, 1));
            instant_new(
                vm,
                millis.div_euclid(1000),
                (millis.rem_euclid(1000) * 1_000_000) as i32,
            )
        },
    ),
    method(
        "isAfter",
        "(Ljava/time/Instant;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let ordering = (
                read_long_field(vm, this, "epochSecond"),
                read_int_field(vm, this, "nano"),
            )
                .cmp(&(
                    read_long_field(vm, other, "epochSecond"),
                    read_int_field(vm, other, "nano"),
                ));
            crate::boolean(ordering.is_gt())
        },
    ),
    method(
        "isBefore",
        "(Ljava/time/Instant;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let ordering = (
                read_long_field(vm, this, "epochSecond"),
                read_int_field(vm, this, "nano"),
            )
                .cmp(&(
                    read_long_field(vm, other, "epochSecond"),
                    read_int_field(vm, other, "nano"),
                ));
            crate::boolean(ordering.is_lt())
        },
    ),
    method(
        "compareTo",
        "(Ljava/time/Instant;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            int((
                read_long_field(vm, this, "epochSecond"),
                read_int_field(vm, this, "nano"),
            )
                .cmp(&(
                    read_long_field(vm, other, "epochSecond"),
                    read_int_field(vm, other, "nano"),
                )) as i32)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() {
                return crate::boolean(false);
            }
            let class = vm.resolve_class("java/time/Instant")?;
            if !vm.is_instance(other, class) {
                return crate::boolean(false);
            }
            let this = receiver(args);
            crate::boolean(
                read_long_field(vm, this, "epochSecond")
                    == read_long_field(vm, other, "epochSecond")
                    && read_int_field(vm, this, "nano") == read_int_field(vm, other, "nano"),
            )
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let seconds = read_long_field(vm, this, "epochSecond");
        int(((seconds ^ (seconds >> 32)) as i32)
            .wrapping_mul(31)
            .wrapping_add(read_int_field(vm, this, "nano")))
    }),
];

fn instant_new(vm: &mut Vm<'_>, epoch_second: i64, nano: i32) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/time/Instant")?;
    let instance = vm.new_instance(class)?;
    write_long_field(vm, instance, "epochSecond", epoch_second);
    write_int_field(vm, instance, "nano", nano);
    object(instance)
}

/// `java.time.Instant`, stored as epoch seconds plus nanoseconds.
pub(crate) const INSTANT: oxjvm_vm::NativeClass = class(
    "java/time/Instant",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("epochSecond", "J", ACC_PRIVATE, None),
        field("nano", "I", ACC_PRIVATE, None),
    ],
    &INSTANT_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// DateTimeFormatter (pattern only) + ZonedDateTime
// -------------------------------------------------------------------------------------------

const DATE_TIME_FORMATTER_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "ofPattern",
        "(Ljava/lang/String;)Ljava/time/format/DateTimeFormatter;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| date_time_formatter_new(vm, ref_arg(args, 0)),
    ),
    method(
        "ofPattern",
        "(Ljava/lang/String;Ljava/util/Locale;)Ljava/time/format/DateTimeFormatter;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| date_time_formatter_new(vm, ref_arg(args, 0)),
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm
                .read_string_field(receiver(args), vm.class_of(receiver(args)), "pattern")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
];

fn date_time_formatter_new(vm: &mut Vm<'_>, pattern: ObjectRef) -> Result<Value, VmError> {
    if pattern.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let class = vm.resolve_class("java/time/format/DateTimeFormatter")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "pattern", "Ljava/lang/String;")?;
    vm.set_instance_ref(instance, declaring, field, pattern);
    object(instance)
}

/// `java.time.format.DateTimeFormatter`, carrying only the pattern.
pub(crate) const DATE_TIME_FORMATTER: oxjvm_vm::NativeClass = class(
    "java/time/format/DateTimeFormatter",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[field("pattern", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &DATE_TIME_FORMATTER_METHODS,
    None,
);

const ZONED_DATE_TIME_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "now",
        "()Ljava/time/ZonedDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let millis = vm.host_time_millis();
            zoned_date_time_new(vm, millis)
        },
    ),
    method(
        "format",
        "(Ljava/time/format/DateTimeFormatter;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let millis = read_long_field(vm, this, "millis");
            let formatter = ref_arg(args, 1);
            let pattern = vm
                .read_string_field(formatter, vm.class_of(formatter), "pattern")
                .unwrap_or_default();
            object(vm.make_string(&crate::text::format_pattern(&pattern, millis))?)
        },
    ),
    method(
        "toInstant",
        "()Ljava/time/Instant;",
        ACC_PUBLIC,
        |vm, _, args| {
            let millis = read_long_field(vm, receiver(args), "millis");
            instant_new(
                vm,
                millis.div_euclid(1000),
                (millis.rem_euclid(1000) * 1_000_000) as i32,
            )
        },
    ),
    method("toEpochSecond", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "millis").div_euclid(1000))
    }),
    method("getNano", "()I", ACC_PUBLIC, |vm, _, args| {
        int((read_long_field(vm, receiver(args), "millis").rem_euclid(1000) * 1_000_000) as i32)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let millis = read_long_field(vm, receiver(args), "millis");
            object(vm.make_string(&crate::text::format_pattern(
                "yyyy-MM-dd'T'HH:mm:ss.SSS",
                millis,
            ))?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() {
                return crate::boolean(false);
            }
            let class = vm.resolve_class("java/time/ZonedDateTime")?;
            if !vm.is_instance(other, class) {
                return crate::boolean(false);
            }
            crate::boolean(
                read_long_field(vm, receiver(args), "millis")
                    == read_long_field(vm, other, "millis"),
            )
        },
    ),
];

fn zoned_date_time_new(vm: &mut Vm<'_>, millis: i64) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/time/ZonedDateTime")?;
    let instance = vm.new_instance(class)?;
    write_long_field(vm, instance, "millis", millis);
    object(instance)
}

/// `java.time.ZonedDateTime`, stored as an epoch-millisecond count.
pub(crate) const ZONED_DATE_TIME: oxjvm_vm::NativeClass = class(
    "java/time/ZonedDateTime",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[field("millis", "J", ACC_PRIVATE, None)],
    &ZONED_DATE_TIME_METHODS,
    None,
);
