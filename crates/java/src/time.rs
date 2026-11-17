//! A minimal `java.time.Duration` / `java.time.Instant` (nanosecond precision, no calendars).

use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::lang::{read_int_field, read_long_field, write_int_field, write_long_field};
use crate::{class, field, int, long, method, object, receiver, ref_arg};

const DURATION_METHODS: [oxjvm_vm::NativeMethodDef; 22] = [
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
    method(
        "ofSeconds",
        "(JJ)Ljava/time/Duration;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let nanos = crate::long_arg(args, 0)
                .saturating_mul(1_000_000_000)
                .saturating_add(crate::long_arg(args, 1));
            duration_new(vm, nanos)
        },
    ),
    method(
        "ofMinutes",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| duration_new(vm, crate::long_arg(args, 0).saturating_mul(60_000_000_000)),
    ),
    method(
        "ofHours",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            duration_new(
                vm,
                crate::long_arg(args, 0).saturating_mul(3_600_000_000_000),
            )
        },
    ),
    method(
        "ofDays",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            duration_new(
                vm,
                crate::long_arg(args, 0).saturating_mul(86_400_000_000_000),
            )
        },
    ),
    method(
        "of",
        "(JLjava/time/temporal/TemporalUnit;)Ljava/time/Duration;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let amount = crate::long_arg(args, 0);
            let unit = ref_arg(args, 1);
            if unit.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("unit")));
            }
            // DAYS is treated as exactly 24 hours even though its duration is estimated.
            let chrono = vm.resolve_class("java/time/temporal/ChronoUnit")?;
            vm.ensure_initialized(chrono)?;
            let days = vm
                .read_static_ref_if_set(chrono, "DAYS", "Ljava/time/temporal/ChronoUnit;")
                .unwrap_or(ObjectRef::NULL);
            if unit != days {
                let (class, method) =
                    vm.resolve_virtual_method(unit, "isDurationEstimated", "()Z")?;
                let estimated = vm
                    .invoke_method(class, method, vec![Value::Ref(unit)])?
                    .as_int()
                    != 0;
                if estimated {
                    return Err(vm.throw_new(
                        "java/time/temporal/UnsupportedTemporalTypeException",
                        Some("Unit must not have an estimated duration"),
                    ));
                }
            }
            if amount == 0 {
                return duration_new(vm, 0);
            }
            let (class, method) =
                vm.resolve_virtual_method(unit, "getDuration", "()Ljava/time/Duration;")?;
            let duration = vm
                .invoke_method(class, method, vec![Value::Ref(unit)])?
                .as_ref();
            let nanos_per_unit = read_long_field(vm, duration, "nanos");
            duration_new(vm, amount.saturating_mul(nanos_per_unit))
        },
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

/// `Duration.toString`: `PT{h}H{m}M{s}S` with zero parts omitted, matching the
/// reference implementation (including the negative-second fraction form).
fn format_duration(nanos: i64) -> alloc::string::String {
    if nanos == 0 {
        return alloc::string::String::from("PT0S");
    }
    let mut seconds = nanos / 1_000_000_000;
    let mut nano = nanos % 1_000_000_000;
    if nano < 0 {
        nano += 1_000_000_000;
        seconds -= 1;
    }
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let secs = seconds % 60;
    let mut text = alloc::string::String::from("PT");
    if hours != 0 {
        text.push_str(&oxjvm_vm::format::long_to_string(hours));
        text.push('H');
    }
    if minutes != 0 {
        text.push_str(&oxjvm_vm::format::long_to_string(minutes));
        text.push('M');
    }
    if secs == 0 && nano == 0 && text.len() > 2 {
        return text;
    }
    if secs < 0 && nano > 0 {
        if secs == -1 {
            text.push_str("-0.");
        } else {
            text.push_str(&oxjvm_vm::format::long_to_string(secs + 1));
            text.push('.');
        }
    } else {
        text.push_str(&oxjvm_vm::format::long_to_string(secs));
        if nano > 0 {
            text.push('.');
        }
    }
    if nano > 0 {
        let mut fraction = oxjvm_vm::format::long_to_string(nano);
        while fraction.len() < 9 {
            fraction.insert(0, '0');
        }
        while fraction.ends_with('0') {
            fraction.pop();
        }
        text.push_str(&fraction);
    }
    text.push('S');
    text
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

const INSTANT_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
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
    method(
        "isSupported",
        "(Ljava/time/temporal/TemporalField;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let ordinal = chrono_field_ordinal(vm, ref_arg(args, 1));
            crate::boolean(matches!(ordinal, Some(0 | 2 | 4 | 28)))
        },
    ),
    method(
        "getLong",
        "(Ljava/time/temporal/TemporalField;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let ordinal =
                chrono_field_ordinal(vm, ref_arg(args, 1)).ok_or_else(|| unsupported_field(vm))?;
            let nano = i64::from(read_int_field(vm, this, "nano"));
            let value = match ordinal {
                0 => nano,
                2 => nano / 1_000,
                4 => nano / 1_000_000,
                28 => read_long_field(vm, this, "epochSecond"),
                _ => return Err(unsupported_field(vm)),
            };
            long(value)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let epoch_second = read_long_field(vm, this, "epochSecond");
            let nano = read_int_field(vm, this, "nano");
            let days = epoch_second.div_euclid(86_400);
            let second_of_day = epoch_second.rem_euclid(86_400);
            let (year, month, day) = civil_from_days(days);
            let hour = second_of_day / 3600;
            let minute = (second_of_day / 60) % 60;
            let second = second_of_day % 60;
            let mut text =
                alloc::format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}");
            if nano != 0 {
                let fraction = if nano % 1_000_000 == 0 {
                    alloc::format!("{:03}", nano / 1_000_000)
                } else if nano % 1_000 == 0 {
                    alloc::format!("{:06}", nano / 1_000)
                } else {
                    alloc::format!("{nano:09}")
                };
                text.push('.');
                text.push_str(&fraction);
            }
            text.push('Z');
            object(vm.make_string(&text)?)
        },
    ),
];

fn instant_new(vm: &mut Vm<'_>, epoch_second: i64, nano: i32) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/time/Instant")?;
    let instance = vm.new_instance(class)?;
    write_long_field(vm, instance, "epochSecond", epoch_second);
    write_int_field(vm, instance, "nano", nano);
    object(instance)
}

fn instant_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/time/Instant")?;
    for (name, seconds, nano) in [
        ("EPOCH", 0i64, 0i32),
        ("MIN", -31_557_014_167_219_200, 0),
        ("MAX", 31_556_889_864_403_199, 999_999_999),
    ] {
        let instance = vm.new_instance(class)?;
        write_long_field(vm, instance, "epochSecond", seconds);
        write_int_field(vm, instance, "nano", nano);
        vm.set_static_value(class, name, "Ljava/time/Instant;", Value::Ref(instance));
    }
    Ok(())
}

/// `java.time.Instant`, stored as epoch seconds plus nanoseconds.
pub(crate) const INSTANT: oxjvm_vm::NativeClass = class(
    "java/time/Instant",
    Some("java/lang/Object"),
    &[
        "java/time/temporal/TemporalAccessor",
        "java/time/temporal/Temporal",
        "java/lang/Comparable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("epochSecond", "J", ACC_PRIVATE, None),
        field("nano", "I", ACC_PRIVATE, None),
        field(
            "EPOCH",
            "Ljava/time/Instant;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MIN",
            "Ljava/time/Instant;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MAX",
            "Ljava/time/Instant;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &INSTANT_METHODS,
    Some(instant_clinit),
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

const ZONED_DATE_TIME_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
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
        "of",
        "(Ljava/time/LocalDateTime;Ljava/time/ZoneId;)Ljava/time/ZonedDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let local = ref_arg(args, 0);
            let zone = ref_arg(args, 1);
            if local.is_null() || zone.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let epoch_day = read_long_field(vm, local, "epochDay");
            let nano_of_day = read_long_field(vm, local, "nanoOfDay");
            let local_second = epoch_day * 86_400 + nano_of_day / 1_000_000_000;
            let offset = zone_offset_seconds(vm, zone);
            zoned_date_time_new(vm, (local_second - i64::from(offset)) * 1000)
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

// -------------------------------------------------------------------------------------------
// java.time.format.FormatStyle
// -------------------------------------------------------------------------------------------

const FORMAT_STYLE_NAMES: [&str; 4] = ["FULL", "LONG", "MEDIUM", "SHORT"];

fn format_style_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/time/format/FormatStyle")?;
    for (index, name) in FORMAT_STYLE_NAMES.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        if let Some((declaring, field)) = vm.find_field_opt(class, "ordinal", "I") {
            vm.set_instance_int(instance, declaring, field, index as i32);
        }
        vm.set_static_value(
            class,
            name,
            "Ljava/time/format/FormatStyle;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

const FORMAT_STYLE_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "ordinal"))
    }),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let index = read_int_field(vm, receiver(args), "ordinal").clamp(0, 3) as usize;
        object(vm.make_string(FORMAT_STYLE_NAMES[index])?)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let index = read_int_field(vm, receiver(args), "ordinal").clamp(0, 3) as usize;
            object(vm.make_string(FORMAT_STYLE_NAMES[index])?)
        },
    ),
    method(
        "values",
        "()[Ljava/time/format/FormatStyle;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/time/format/FormatStyle")?;
            vm.ensure_initialized(class)?;
            let array = vm.allocate_object_array(class, FORMAT_STYLE_NAMES.len())?;
            for (index, name) in FORMAT_STYLE_NAMES.iter().enumerate() {
                if let Some(value) =
                    vm.read_static_ref_if_set(class, name, "Ljava/time/format/FormatStyle;")
                {
                    vm.array_set_ref(array, index, value)?;
                }
            }
            object(array)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/time/format/FormatStyle;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 0));
            let class = vm.resolve_class("java/time/format/FormatStyle")?;
            vm.ensure_initialized(class)?;
            for candidate in FORMAT_STYLE_NAMES {
                if candidate == name {
                    if let Some(value) = vm.read_static_ref_if_set(
                        class,
                        candidate,
                        "Ljava/time/format/FormatStyle;",
                    ) {
                        return object(value);
                    }
                }
            }
            Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some(&alloc::format!(
                    "No enum constant java.time.format.FormatStyle.{name}"
                )),
            ))
        },
    ),
];

/// `java.time.format.FormatStyle`: the length of a localized date/time format.
pub(crate) const FORMAT_STYLE: oxjvm_vm::NativeClass = class(
    "java/time/format/FormatStyle",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "FULL",
            "Ljava/time/format/FormatStyle;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "LONG",
            "Ljava/time/format/FormatStyle;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MEDIUM",
            "Ljava/time/format/FormatStyle;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SHORT",
            "Ljava/time/format/FormatStyle;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &FORMAT_STYLE_METHODS,
    Some(format_style_clinit),
);

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

// -------------------------------------------------------------------------------------------
// LocalDate / LocalTime / LocalDateTime (proleptic Gregorian calendar)
// -------------------------------------------------------------------------------------------

/// Convert a count of days since 1970-01-01 into (year, month, day) using Howard Hinnant's
/// civil-calendar algorithm.
fn civil_from_days(days: i64) -> (i32, i32, i32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = (y + i64::from(m <= 2)) as i32;
    (year, m as i32, d as i32)
}

/// Convert (year, month, day) into a count of days since 1970-01-01.
fn days_from_civil(year: i32, month: i32, day: i32) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let m = i64::from(month);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn length_of_month(year: i32, month: i32) -> i32 {
    match month {
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn local_date_new(vm: &mut Vm<'_>, epoch_day: i64) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/time/LocalDate")?;
    let instance = vm.new_instance(class)?;
    write_long_field(vm, instance, "epochDay", epoch_day);
    Ok(instance)
}

// -------------------------------------------------------------------------------------------
// TemporalAccessor wiring shared by the date/time classes
// -------------------------------------------------------------------------------------------

/// The `ChronoField` ordinal of a field argument, or `None` for other `TemporalField` types.
fn chrono_field_ordinal(vm: &mut Vm<'_>, field: ObjectRef) -> Option<i32> {
    let class = vm.resolve_class("java/time/temporal/ChronoField").ok()?;
    if !vm.is_instance(field, class) {
        return None;
    }
    Some(read_int_field(vm, field, "ordinal"))
}

fn unsupported_field(vm: &mut Vm<'_>) -> VmError {
    vm.throw_new(
        "java/time/temporal/UnsupportedTemporalTypeException",
        Some("unsupported field"),
    )
}

/// Date-based `ChronoField` values (ordinals 15..=27) derived from an epoch day.
fn date_field_long(epoch_day: i64, ordinal: i32) -> Option<i64> {
    let (year, month, day) = civil_from_days(epoch_day);
    let day_of_year = (epoch_day - days_from_civil(year, 1, 1) + 1) as i32;
    Some(match ordinal {
        15 => (epoch_day + 3).rem_euclid(7) + 1,
        16 => i64::from((day - 1) % 7 + 1),
        17 => i64::from((day_of_year - 1) % 7 + 1),
        18 => i64::from(day),
        19 => i64::from(day_of_year),
        20 => epoch_day,
        21 => i64::from((day - 1) / 7 + 1),
        22 => i64::from((day_of_year - 1) / 7 + 1),
        23 => i64::from(month),
        24 => i64::from(year) * 12 + i64::from(month - 1),
        25 => i64::from(if year >= 1 { year } else { 1 - year }),
        26 => i64::from(year),
        27 => i64::from(if year >= 1 { 1 } else { 0 }),
        _ => return None,
    })
}

/// Time-based `ChronoField` values (ordinals 0..=14) derived from a nano-of-day.
fn time_field_long(nano_of_day: i64, ordinal: i32) -> Option<i64> {
    let hour = nano_of_day / 3_600_000_000_000;
    Some(match ordinal {
        0 => nano_of_day % 1_000_000_000,
        1 => nano_of_day,
        2 => (nano_of_day % 1_000_000_000) / 1_000,
        3 => nano_of_day / 1_000,
        4 => (nano_of_day % 1_000_000_000) / 1_000_000,
        5 => nano_of_day / 1_000_000,
        6 => (nano_of_day / 1_000_000_000) % 60,
        7 => nano_of_day / 1_000_000_000,
        8 => (nano_of_day / 60_000_000_000) % 60,
        9 => nano_of_day / 60_000_000_000,
        10 => hour % 12,
        11 => {
            if hour % 12 == 0 {
                12
            } else {
                hour % 12
            }
        }
        12 => hour,
        13 => {
            if hour == 0 {
                24
            } else {
                hour
            }
        }
        14 => hour / 12,
        _ => return None,
    })
}

/// Refined date-field ranges for a concrete date; `None` falls back to the field's own range.
fn date_field_range(epoch_day: i64, ordinal: i32) -> Option<(i64, i64)> {
    let (year, month, _) = civil_from_days(epoch_day);
    let month_len = length_of_month(year, month);
    let year_len = if is_leap_year(year) { 366 } else { 365 };
    match ordinal {
        18 => Some((1, i64::from(month_len))),
        19 => Some((1, i64::from(year_len))),
        21 => Some((1, i64::from((month_len + 6) / 7))),
        22 => Some((1, i64::from((year_len + 6) / 7))),
        25 => Some((
            1,
            if year >= 1 {
                999_999_999
            } else {
                1_000_000_000
            },
        )),
        _ => None,
    }
}

/// `range(TemporalField)`: refined date ranges where applicable, else the field's own range.
fn temporal_range(
    vm: &mut Vm<'_>,
    field: ObjectRef,
    epoch_day: Option<i64>,
) -> Result<Value, VmError> {
    let ordinal = chrono_field_ordinal(vm, field).ok_or_else(|| unsupported_field(vm))?;
    if let Some(epoch_day) = epoch_day {
        if let Some((min, max)) = date_field_range(epoch_day, ordinal) {
            return object(crate::temporal::value_range_new(vm, min, min, max, max)?);
        }
    }
    let (class, method) =
        vm.resolve_virtual_method(field, "range", "()Ljava/time/temporal/ValueRange;")?;
    vm.invoke_method(class, method, vec![Value::Ref(field)])
}

/// `isSupported(TemporalUnit)` via the unit's date/time classification.
fn temporal_unit_supported(
    vm: &mut Vm<'_>,
    unit: ObjectRef,
    date_based: bool,
    time_based: bool,
) -> Result<Value, VmError> {
    if date_based {
        let (class, method) = vm.resolve_virtual_method(unit, "isDateBased", "()Z")?;
        let result = vm.invoke_method(class, method, vec![Value::Ref(unit)])?;
        if result.as_int() != 0 {
            return crate::boolean(true);
        }
    }
    if time_based {
        let (class, method) = vm.resolve_virtual_method(unit, "isTimeBased", "()Z")?;
        let result = vm.invoke_method(class, method, vec![Value::Ref(unit)])?;
        if result.as_int() != 0 {
            return crate::boolean(true);
        }
    }
    crate::boolean(false)
}

/// The `ChronoUnit` ordinal of a unit argument, or `None` for other `TemporalUnit` types.
fn chrono_unit_ordinal(vm: &mut Vm<'_>, unit: ObjectRef) -> Option<i32> {
    let class = vm.resolve_class("java/time/temporal/ChronoUnit").ok()?;
    if !vm.is_instance(unit, class) {
        return None;
    }
    Some(read_int_field(vm, unit, "ordinal"))
}

fn unsupported_unit(vm: &mut Vm<'_>) -> VmError {
    vm.throw_new(
        "java/time/temporal/UnsupportedTemporalTypeException",
        Some("unsupported unit"),
    )
}

const NANOS_PER_DAY: i64 = 86_400_000_000_000;

/// Add `amount` of a date-based `ChronoUnit` to an epoch day.
fn date_plus(epoch_day: i64, amount: i64, ordinal: i32) -> Option<i64> {
    let (year, month, day) = civil_from_days(epoch_day);
    Some(match ordinal {
        7 => epoch_day + amount,
        8 => epoch_day + amount * 7,
        9 => {
            let total = i64::from(year) * 12 + i64::from(month - 1) + amount;
            let new_year = total.div_euclid(12) as i32;
            let new_month = (total.rem_euclid(12) + 1) as i32;
            let new_day = day.min(length_of_month(new_year, new_month));
            days_from_civil(new_year, new_month, new_day)
        }
        10..=13 => {
            let years = amount
                * match ordinal {
                    10 => 1,
                    11 => 10,
                    12 => 100,
                    _ => 1000,
                };
            let new_year = (i64::from(year) + years) as i32;
            let new_day = day.min(length_of_month(new_year, month));
            days_from_civil(new_year, month, new_day)
        }
        _ => return None,
    })
}

/// Whole years between two epoch days (anniversary-aware).
fn years_between(start: i64, end: i64) -> i64 {
    let (sy, sm, sd) = civil_from_days(start);
    let (ey, em, ed) = civil_from_days(end);
    let mut years = i64::from(ey) - i64::from(sy);
    if years > 0 && (em < sm || (em == sm && ed < sd)) {
        years -= 1;
    } else if years < 0 && (em > sm || (em == sm && ed > sd)) {
        years += 1;
    }
    years
}

/// Amount of a date-based `ChronoUnit` between two epoch days.
fn date_until(start: i64, end: i64, ordinal: i32) -> Option<i64> {
    Some(match ordinal {
        7 => end - start,
        8 => (end - start) / 7,
        9 => {
            let (sy, sm, sd) = civil_from_days(start);
            let (ey, em, ed) = civil_from_days(end);
            let mut months =
                (i64::from(ey) * 12 + i64::from(em - 1)) - (i64::from(sy) * 12 + i64::from(sm - 1));
            if months > 0 && ed < sd {
                months -= 1;
            } else if months < 0 && ed > sd {
                months += 1;
            }
            months
        }
        10 => years_between(start, end),
        11 => years_between(start, end) / 10,
        12 => years_between(start, end) / 100,
        13 => years_between(start, end) / 1000,
        _ => return None,
    })
}

/// Nanoseconds per unit for the time-based `ChronoUnit`s (ordinals 0..=6).
fn time_unit_nanos(ordinal: i32) -> Option<i64> {
    Some(match ordinal {
        0 => 1,
        1 => 1_000,
        2 => 1_000_000,
        3 => 1_000_000_000,
        4 => 60_000_000_000,
        5 => 3_600_000_000_000,
        6 => 43_200_000_000_000,
        _ => return None,
    })
}

fn local_time_new(vm: &mut Vm<'_>, nano_of_day: i64) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/time/LocalTime")?;
    let instance = vm.new_instance(class)?;
    write_long_field(vm, instance, "nanoOfDay", nano_of_day);
    Ok(instance)
}

fn local_date_time_new(
    vm: &mut Vm<'_>,
    epoch_day: i64,
    nano_of_day: i64,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/time/LocalDateTime")?;
    let instance = vm.new_instance(class)?;
    write_long_field(vm, instance, "epochDay", epoch_day);
    write_long_field(vm, instance, "nanoOfDay", nano_of_day);
    Ok(instance)
}

fn local_date_string(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let (year, month, day) = civil_from_days(read_long_field(vm, this, "epochDay"));
    let text = alloc::format!("{year:04}-{month:02}-{day:02}");
    object(vm.make_string(&text)?)
}

fn local_time_string(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let nano = read_long_field(vm, this, "nanoOfDay");
    let hour = nano / 3_600_000_000_000;
    let minute = (nano / 60_000_000_000) % 60;
    let second = (nano / 1_000_000_000) % 60;
    let nanos = nano % 1_000_000_000;
    let text = if nanos == 0 {
        alloc::format!("{hour:02}:{minute:02}:{second:02}")
    } else {
        alloc::format!("{hour:02}:{minute:02}:{second:02}.{nanos:09}")
    };
    object(vm.make_string(&text)?)
}

const LOCAL_DATE_METHODS: [oxjvm_vm::NativeMethodDef; 35] = [
    method(
        "of",
        "(III)Ljava/time/LocalDate;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let year = crate::int_arg(args, 0);
            let month = crate::int_arg(args, 1);
            let day = crate::int_arg(args, 2);
            if !(1..=12).contains(&month) || day < 1 || day > length_of_month(year, month) {
                return Err(vm.throw_new("java/time/DateTimeException", Some("Invalid date")));
            }
            object(local_date_new(vm, days_from_civil(year, month, day))?)
        },
    ),
    method(
        "of",
        "(ILjava/time/Month;I)Ljava/time/LocalDate;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let year = crate::int_arg(args, 0);
            let month = read_int_field(vm, ref_arg(args, 1), "value");
            let day = crate::int_arg(args, 2);
            if !(1..=12).contains(&month) || day < 1 || day > length_of_month(year, month) {
                return Err(vm.throw_new("java/time/DateTimeException", Some("Invalid date")));
            }
            object(local_date_new(vm, days_from_civil(year, month, day))?)
        },
    ),
    method(
        "ofEpochDay",
        "(J)Ljava/time/LocalDate;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(local_date_new(vm, crate::long_arg(args, 0))?),
    ),
    method(
        "now",
        "()Ljava/time/LocalDate;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let millis = vm.host_time_millis();
            object(local_date_new(vm, millis.div_euclid(86_400_000))?)
        },
    ),
    method(
        "parse",
        "(Ljava/lang/CharSequence;)Ljava/time/LocalDate;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 0));
            let parts: Vec<&str> = text.split('-').collect();
            if parts.len() != 3 {
                return Err(vm.throw_new("java/time/format/DateTimeParseException", Some(&text)));
            }
            let year = parts[0].parse::<i32>().unwrap_or(0);
            let month = parts[1].parse::<i32>().unwrap_or(0);
            let day = parts[2].parse::<i32>().unwrap_or(0);
            object(local_date_new(vm, days_from_civil(year, month, day))?)
        },
    ),
    method("getYear", "()I", ACC_PUBLIC, |vm, _, args| {
        int(civil_from_days(read_long_field(vm, receiver(args), "epochDay")).0)
    }),
    method("getMonthValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(civil_from_days(read_long_field(vm, receiver(args), "epochDay")).1)
    }),
    method("getDayOfMonth", "()I", ACC_PUBLIC, |vm, _, args| {
        int(civil_from_days(read_long_field(vm, receiver(args), "epochDay")).2)
    }),
    method("getDayOfYear", "()I", ACC_PUBLIC, |vm, _, args| {
        let (year, _, _) = civil_from_days(read_long_field(vm, receiver(args), "epochDay"));
        let start = days_from_civil(year, 1, 1);
        int((read_long_field(vm, receiver(args), "epochDay") - start + 1) as i32)
    }),
    method(
        "getDayOfWeek",
        "()Ljava/time/DayOfWeek;",
        ACC_PUBLIC,
        |vm, _, args| {
            let epoch_day = read_long_field(vm, receiver(args), "epochDay");
            // 1970-01-01 was a Thursday (value 4).
            let dow = (epoch_day + 3).rem_euclid(7) + 1;
            let class = vm.resolve_class("java/time/DayOfWeek")?;
            let (declaring, method) = vm
                .find_method(class, "of", "(I)Ljava/time/DayOfWeek;")
                .ok_or_else(|| VmError::internal("DayOfWeek lacks of"))?;
            vm.invoke_method(declaring, method, vec![Value::Int(dow as i32)])
        },
    ),
    method("toEpochDay", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "epochDay"))
    }),
    method("lengthOfMonth", "()I", ACC_PUBLIC, |vm, _, args| {
        let (year, month, _) = civil_from_days(read_long_field(vm, receiver(args), "epochDay"));
        int(length_of_month(year, month))
    }),
    method("lengthOfYear", "()I", ACC_PUBLIC, |vm, _, args| {
        let (year, _, _) = civil_from_days(read_long_field(vm, receiver(args), "epochDay"));
        int(if is_leap_year(year) { 366 } else { 365 })
    }),
    method("isLeapYear", "()Z", ACC_PUBLIC, |vm, _, args| {
        let (year, _, _) = civil_from_days(read_long_field(vm, receiver(args), "epochDay"));
        crate::boolean(is_leap_year(year))
    }),
    method(
        "plusDays",
        "(J)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let epoch_day = read_long_field(vm, receiver(args), "epochDay");
            object(local_date_new(vm, epoch_day + crate::long_arg(args, 1))?)
        },
    ),
    method(
        "minusDays",
        "(J)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let epoch_day = read_long_field(vm, receiver(args), "epochDay");
            object(local_date_new(vm, epoch_day - crate::long_arg(args, 1))?)
        },
    ),
    method(
        "plusWeeks",
        "(J)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let epoch_day = read_long_field(vm, receiver(args), "epochDay");
            object(local_date_new(
                vm,
                epoch_day + crate::long_arg(args, 1) * 7,
            )?)
        },
    ),
    method(
        "plusMonths",
        "(J)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (year, month, day) =
                civil_from_days(read_long_field(vm, receiver(args), "epochDay"));
            let total = i64::from(year) * 12 + i64::from(month - 1) + crate::long_arg(args, 1);
            let new_year = total.div_euclid(12) as i32;
            let new_month = (total.rem_euclid(12) + 1) as i32;
            let new_day = day.min(length_of_month(new_year, new_month));
            object(local_date_new(
                vm,
                days_from_civil(new_year, new_month, new_day),
            )?)
        },
    ),
    method(
        "plusYears",
        "(J)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (year, month, day) =
                civil_from_days(read_long_field(vm, receiver(args), "epochDay"));
            let new_year = (i64::from(year) + crate::long_arg(args, 1)) as i32;
            let new_day = day.min(length_of_month(new_year, month));
            object(local_date_new(
                vm,
                days_from_civil(new_year, month, new_day),
            )?)
        },
    ),
    method(
        "withDayOfMonth",
        "(I)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (year, month, _) = civil_from_days(read_long_field(vm, receiver(args), "epochDay"));
            object(local_date_new(
                vm,
                days_from_civil(year, month, crate::int_arg(args, 1)),
            )?)
        },
    ),
    method(
        "atStartOfDay",
        "()Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let epoch_day = read_long_field(vm, receiver(args), "epochDay");
            object(local_date_time_new(vm, epoch_day, 0)?)
        },
    ),
    method(
        "atTime",
        "(II)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let epoch_day = read_long_field(vm, receiver(args), "epochDay");
            let hour = crate::int_arg(args, 1);
            let minute = crate::int_arg(args, 2);
            let nano = (i64::from(hour) * 3600 + i64::from(minute) * 60) * 1_000_000_000;
            object(local_date_time_new(vm, epoch_day, nano)?)
        },
    ),
    method(
        "isAfter",
        "(Ljava/time/LocalDate;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            crate::boolean(
                read_long_field(vm, receiver(args), "epochDay")
                    > read_long_field(vm, ref_arg(args, 1), "epochDay"),
            )
        },
    ),
    method(
        "isBefore",
        "(Ljava/time/LocalDate;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            crate::boolean(
                read_long_field(vm, receiver(args), "epochDay")
                    < read_long_field(vm, ref_arg(args, 1), "epochDay"),
            )
        },
    ),
    method(
        "compareTo",
        "(Ljava/time/LocalDate;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            int(
                read_long_field(vm, receiver(args), "epochDay").cmp(&read_long_field(
                    vm,
                    ref_arg(args, 1),
                    "epochDay",
                )) as i32,
            )
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| local_date_string(vm, receiver(args)),
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
            let class = vm.resolve_class("java/time/LocalDate")?;
            if !vm.is_instance(other, class) {
                return crate::boolean(false);
            }
            crate::boolean(
                read_long_field(vm, receiver(args), "epochDay")
                    == read_long_field(vm, other, "epochDay"),
            )
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let epoch_day = read_long_field(vm, receiver(args), "epochDay");
        int((epoch_day ^ (epoch_day >> 32)) as i32)
    }),
    method(
        "isSupported",
        "(Ljava/time/temporal/TemporalField;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let ordinal = chrono_field_ordinal(vm, ref_arg(args, 1));
            crate::boolean(matches!(ordinal, Some(15..=27)))
        },
    ),
    method(
        "getLong",
        "(Ljava/time/temporal/TemporalField;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let ordinal =
                chrono_field_ordinal(vm, ref_arg(args, 1)).ok_or_else(|| unsupported_field(vm))?;
            let epoch_day = read_long_field(vm, this, "epochDay");
            long(date_field_long(epoch_day, ordinal).ok_or_else(|| unsupported_field(vm))?)
        },
    ),
    method(
        "range",
        "(Ljava/time/temporal/TemporalField;)Ljava/time/temporal/ValueRange;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let epoch_day = read_long_field(vm, this, "epochDay");
            temporal_range(vm, ref_arg(args, 1), Some(epoch_day))
        },
    ),
    method(
        "isSupported",
        "(Ljava/time/temporal/TemporalUnit;)Z",
        ACC_PUBLIC,
        |vm, _, args| temporal_unit_supported(vm, ref_arg(args, 1), true, false),
    ),
    method(
        "plus",
        "(JLjava/time/temporal/TemporalUnit;)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let epoch_day = read_long_field(vm, receiver(args), "epochDay");
            let ordinal =
                chrono_unit_ordinal(vm, ref_arg(args, 2)).ok_or_else(|| unsupported_unit(vm))?;
            let new_epoch = date_plus(epoch_day, crate::long_arg(args, 1), ordinal)
                .ok_or_else(|| unsupported_unit(vm))?;
            object(local_date_new(vm, new_epoch)?)
        },
    ),
    method(
        "minus",
        "(JLjava/time/temporal/TemporalUnit;)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let epoch_day = read_long_field(vm, receiver(args), "epochDay");
            let ordinal =
                chrono_unit_ordinal(vm, ref_arg(args, 2)).ok_or_else(|| unsupported_unit(vm))?;
            let new_epoch = date_plus(epoch_day, -crate::long_arg(args, 1), ordinal)
                .ok_or_else(|| unsupported_unit(vm))?;
            object(local_date_new(vm, new_epoch)?)
        },
    ),
    method(
        "until",
        "(Ljava/time/temporal/Temporal;Ljava/time/temporal/TemporalUnit;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let start = read_long_field(vm, receiver(args), "epochDay");
            let end = read_long_field(vm, ref_arg(args, 1), "epochDay");
            let ordinal =
                chrono_unit_ordinal(vm, ref_arg(args, 2)).ok_or_else(|| unsupported_unit(vm))?;
            long(date_until(start, end, ordinal).ok_or_else(|| unsupported_unit(vm))?)
        },
    ),
];

pub(crate) const LOCAL_DATE: oxjvm_vm::NativeClass = class(
    "java/time/LocalDate",
    Some("java/lang/Object"),
    &[
        "java/time/temporal/TemporalAccessor",
        "java/time/temporal/Temporal",
        "java/lang/Comparable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC | ACC_FINAL,
    &[field("epochDay", "J", ACC_PRIVATE, None)],
    &LOCAL_DATE_METHODS,
    None,
);

const LOCAL_TIME_METHODS: [oxjvm_vm::NativeMethodDef; 25] = [
    method(
        "of",
        "(II)Ljava/time/LocalTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let hour = crate::int_arg(args, 0);
            let minute = crate::int_arg(args, 1);
            object(local_time_new(
                vm,
                (i64::from(hour) * 3600 + i64::from(minute) * 60) * 1_000_000_000,
            )?)
        },
    ),
    method(
        "of",
        "(III)Ljava/time/LocalTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let hour = crate::int_arg(args, 0);
            let minute = crate::int_arg(args, 1);
            let second = crate::int_arg(args, 2);
            object(local_time_new(
                vm,
                (i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second))
                    * 1_000_000_000,
            )?)
        },
    ),
    method(
        "of",
        "(IIII)Ljava/time/LocalTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let hour = crate::int_arg(args, 0);
            let minute = crate::int_arg(args, 1);
            let second = crate::int_arg(args, 2);
            let nano = crate::int_arg(args, 3);
            object(local_time_new(
                vm,
                (i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second))
                    * 1_000_000_000
                    + i64::from(nano),
            )?)
        },
    ),
    method(
        "ofNanoOfDay",
        "(J)Ljava/time/LocalTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(local_time_new(vm, crate::long_arg(args, 0))?),
    ),
    method(
        "ofSecondOfDay",
        "(J)Ljava/time/LocalTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(local_time_new(
                vm,
                crate::long_arg(args, 0) * 1_000_000_000,
            )?)
        },
    ),
    method(
        "now",
        "()Ljava/time/LocalTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let millis = vm.host_time_millis();
            let nano = millis.rem_euclid(86_400_000) * 1_000_000;
            object(local_time_new(vm, nano)?)
        },
    ),
    method("getHour", "()I", ACC_PUBLIC, |vm, _, args| {
        int((read_long_field(vm, receiver(args), "nanoOfDay") / 3_600_000_000_000) as i32)
    }),
    method("getMinute", "()I", ACC_PUBLIC, |vm, _, args| {
        int(((read_long_field(vm, receiver(args), "nanoOfDay") / 60_000_000_000) % 60) as i32)
    }),
    method("getSecond", "()I", ACC_PUBLIC, |vm, _, args| {
        int(((read_long_field(vm, receiver(args), "nanoOfDay") / 1_000_000_000) % 60) as i32)
    }),
    method("getNano", "()I", ACC_PUBLIC, |vm, _, args| {
        int((read_long_field(vm, receiver(args), "nanoOfDay") % 1_000_000_000) as i32)
    }),
    method("toNanoOfDay", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "nanoOfDay"))
    }),
    method("toSecondOfDay", "()I", ACC_PUBLIC, |vm, _, args| {
        int((read_long_field(vm, receiver(args), "nanoOfDay") / 1_000_000_000) as i32)
    }),
    method(
        "plusHours",
        "(J)Ljava/time/LocalTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let nano = read_long_field(vm, receiver(args), "nanoOfDay");
            object(local_time_new(
                vm,
                (nano + crate::long_arg(args, 1) * 3_600_000_000_000)
                    .rem_euclid(86_400_000_000_000),
            )?)
        },
    ),
    method(
        "minusHours",
        "(J)Ljava/time/LocalTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let nano = read_long_field(vm, receiver(args), "nanoOfDay");
            object(local_time_new(
                vm,
                (nano - crate::long_arg(args, 1) * 3_600_000_000_000)
                    .rem_euclid(86_400_000_000_000),
            )?)
        },
    ),
    method(
        "compareTo",
        "(Ljava/time/LocalTime;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            int(
                read_long_field(vm, receiver(args), "nanoOfDay").cmp(&read_long_field(
                    vm,
                    ref_arg(args, 1),
                    "nanoOfDay",
                )) as i32,
            )
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| local_time_string(vm, receiver(args)),
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
            let class = vm.resolve_class("java/time/LocalTime")?;
            if !vm.is_instance(other, class) {
                return crate::boolean(false);
            }
            crate::boolean(
                read_long_field(vm, receiver(args), "nanoOfDay")
                    == read_long_field(vm, other, "nanoOfDay"),
            )
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let nano = read_long_field(vm, receiver(args), "nanoOfDay");
        int((nano ^ (nano >> 32)) as i32)
    }),
    method(
        "isSupported",
        "(Ljava/time/temporal/TemporalField;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let ordinal = chrono_field_ordinal(vm, ref_arg(args, 1));
            crate::boolean(matches!(ordinal, Some(0..=14)))
        },
    ),
    method(
        "getLong",
        "(Ljava/time/temporal/TemporalField;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let ordinal =
                chrono_field_ordinal(vm, ref_arg(args, 1)).ok_or_else(|| unsupported_field(vm))?;
            let nano_of_day = read_long_field(vm, this, "nanoOfDay");
            long(time_field_long(nano_of_day, ordinal).ok_or_else(|| unsupported_field(vm))?)
        },
    ),
    method(
        "range",
        "(Ljava/time/temporal/TemporalField;)Ljava/time/temporal/ValueRange;",
        ACC_PUBLIC,
        |vm, _, args| temporal_range(vm, ref_arg(args, 1), None),
    ),
    method(
        "isSupported",
        "(Ljava/time/temporal/TemporalUnit;)Z",
        ACC_PUBLIC,
        |vm, _, args| temporal_unit_supported(vm, ref_arg(args, 1), false, true),
    ),
    method(
        "plus",
        "(JLjava/time/temporal/TemporalUnit;)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let nano = read_long_field(vm, receiver(args), "nanoOfDay");
            let ordinal =
                chrono_unit_ordinal(vm, ref_arg(args, 2)).ok_or_else(|| unsupported_unit(vm))?;
            let unit_nanos = time_unit_nanos(ordinal).ok_or_else(|| unsupported_unit(vm))?;
            let new_nano = (nano + crate::long_arg(args, 1) * unit_nanos).rem_euclid(NANOS_PER_DAY);
            object(local_time_new(vm, new_nano)?)
        },
    ),
    method(
        "minus",
        "(JLjava/time/temporal/TemporalUnit;)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let nano = read_long_field(vm, receiver(args), "nanoOfDay");
            let ordinal =
                chrono_unit_ordinal(vm, ref_arg(args, 2)).ok_or_else(|| unsupported_unit(vm))?;
            let unit_nanos = time_unit_nanos(ordinal).ok_or_else(|| unsupported_unit(vm))?;
            let new_nano = (nano - crate::long_arg(args, 1) * unit_nanos).rem_euclid(NANOS_PER_DAY);
            object(local_time_new(vm, new_nano)?)
        },
    ),
    method(
        "until",
        "(Ljava/time/temporal/Temporal;Ljava/time/temporal/TemporalUnit;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let start = read_long_field(vm, receiver(args), "nanoOfDay");
            let end = read_long_field(vm, ref_arg(args, 1), "nanoOfDay");
            let ordinal =
                chrono_unit_ordinal(vm, ref_arg(args, 2)).ok_or_else(|| unsupported_unit(vm))?;
            let unit_nanos = time_unit_nanos(ordinal).ok_or_else(|| unsupported_unit(vm))?;
            long((end - start) / unit_nanos)
        },
    ),
];

pub(crate) const LOCAL_TIME: oxjvm_vm::NativeClass = class(
    "java/time/LocalTime",
    Some("java/lang/Object"),
    &[
        "java/time/temporal/TemporalAccessor",
        "java/time/temporal/Temporal",
        "java/lang/Comparable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC | ACC_FINAL,
    &[field("nanoOfDay", "J", ACC_PRIVATE, None)],
    &LOCAL_TIME_METHODS,
    None,
);

const LOCAL_DATE_TIME_METHODS: [oxjvm_vm::NativeMethodDef; 30] = [
    method(
        "of",
        "(IIIII)Ljava/time/LocalDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let year = crate::int_arg(args, 0);
            let month = crate::int_arg(args, 1);
            let day = crate::int_arg(args, 2);
            let hour = crate::int_arg(args, 3);
            let minute = crate::int_arg(args, 4);
            let epoch_day = days_from_civil(year, month, day);
            let nano = (i64::from(hour) * 3600 + i64::from(minute) * 60) * 1_000_000_000;
            object(local_date_time_new(vm, epoch_day, nano)?)
        },
    ),
    method(
        "of",
        "(IIIIII)Ljava/time/LocalDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let year = crate::int_arg(args, 0);
            let month = crate::int_arg(args, 1);
            let day = crate::int_arg(args, 2);
            let hour = crate::int_arg(args, 3);
            let minute = crate::int_arg(args, 4);
            let second = crate::int_arg(args, 5);
            let epoch_day = days_from_civil(year, month, day);
            let nano = (i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second))
                * 1_000_000_000;
            object(local_date_time_new(vm, epoch_day, nano)?)
        },
    ),
    method(
        "of",
        "(IIIIIII)Ljava/time/LocalDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let year = crate::int_arg(args, 0);
            let month = crate::int_arg(args, 1);
            let day = crate::int_arg(args, 2);
            let hour = crate::int_arg(args, 3);
            let minute = crate::int_arg(args, 4);
            let second = crate::int_arg(args, 5);
            let nano_of_second = crate::int_arg(args, 6);
            let epoch_day = days_from_civil(year, month, day);
            let nano = (i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second))
                * 1_000_000_000
                + i64::from(nano_of_second);
            object(local_date_time_new(vm, epoch_day, nano)?)
        },
    ),
    method(
        "of",
        "(Ljava/time/LocalDate;Ljava/time/LocalTime;)Ljava/time/LocalDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let date = ref_arg(args, 0);
            let time = ref_arg(args, 1);
            object(local_date_time_new(
                vm,
                read_long_field(vm, date, "epochDay"),
                read_long_field(vm, time, "nanoOfDay"),
            )?)
        },
    ),
    method(
        "now",
        "()Ljava/time/LocalDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let millis = vm.host_time_millis();
            object(local_date_time_new(
                vm,
                millis.div_euclid(86_400_000),
                millis.rem_euclid(86_400_000) * 1_000_000,
            )?)
        },
    ),
    method("getYear", "()I", ACC_PUBLIC, |vm, _, args| {
        int(civil_from_days(read_long_field(vm, receiver(args), "epochDay")).0)
    }),
    method("getMonthValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(civil_from_days(read_long_field(vm, receiver(args), "epochDay")).1)
    }),
    method("getDayOfMonth", "()I", ACC_PUBLIC, |vm, _, args| {
        int(civil_from_days(read_long_field(vm, receiver(args), "epochDay")).2)
    }),
    method("getHour", "()I", ACC_PUBLIC, |vm, _, args| {
        int((read_long_field(vm, receiver(args), "nanoOfDay") / 3_600_000_000_000) as i32)
    }),
    method("getMinute", "()I", ACC_PUBLIC, |vm, _, args| {
        int(((read_long_field(vm, receiver(args), "nanoOfDay") / 60_000_000_000) % 60) as i32)
    }),
    method("getSecond", "()I", ACC_PUBLIC, |vm, _, args| {
        int(((read_long_field(vm, receiver(args), "nanoOfDay") / 1_000_000_000) % 60) as i32)
    }),
    method(
        "toLocalDate",
        "()Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(local_date_new(
                vm,
                read_long_field(vm, receiver(args), "epochDay"),
            )?)
        },
    ),
    method(
        "toLocalTime",
        "()Ljava/time/LocalTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            object(local_time_new(
                vm,
                read_long_field(vm, receiver(args), "nanoOfDay"),
            )?)
        },
    ),
    method(
        "plusDays",
        "(J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(local_date_time_new(
                vm,
                read_long_field(vm, this, "epochDay") + crate::long_arg(args, 1),
                read_long_field(vm, this, "nanoOfDay"),
            )?)
        },
    ),
    method(
        "minusDays",
        "(J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(local_date_time_new(
                vm,
                read_long_field(vm, this, "epochDay") - crate::long_arg(args, 1),
                read_long_field(vm, this, "nanoOfDay"),
            )?)
        },
    ),
    method(
        "plusHours",
        "(J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let total = read_long_field(vm, this, "nanoOfDay")
                + crate::long_arg(args, 1) * 3_600_000_000_000;
            object(local_date_time_new(
                vm,
                read_long_field(vm, this, "epochDay") + total.div_euclid(86_400_000_000_000),
                total.rem_euclid(86_400_000_000_000),
            )?)
        },
    ),
    method(
        "compareTo",
        "(Ljava/time/LocalDateTime;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let ordering = (
                read_long_field(vm, this, "epochDay"),
                read_long_field(vm, this, "nanoOfDay"),
            )
                .cmp(&(
                    read_long_field(vm, other, "epochDay"),
                    read_long_field(vm, other, "nanoOfDay"),
                ));
            int(ordering as i32)
        },
    ),
    method(
        "isAfter",
        "(Ljava/time/LocalDateTime;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            crate::boolean(
                (
                    read_long_field(vm, this, "epochDay"),
                    read_long_field(vm, this, "nanoOfDay"),
                ) > (
                    read_long_field(vm, other, "epochDay"),
                    read_long_field(vm, other, "nanoOfDay"),
                ),
            )
        },
    ),
    method(
        "isBefore",
        "(Ljava/time/LocalDateTime;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            crate::boolean(
                (
                    read_long_field(vm, this, "epochDay"),
                    read_long_field(vm, this, "nanoOfDay"),
                ) < (
                    read_long_field(vm, other, "epochDay"),
                    read_long_field(vm, other, "nanoOfDay"),
                ),
            )
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (year, month, day) = civil_from_days(read_long_field(vm, this, "epochDay"));
            let nano = read_long_field(vm, this, "nanoOfDay");
            let hour = nano / 3_600_000_000_000;
            let minute = (nano / 60_000_000_000) % 60;
            let second = (nano / 1_000_000_000) % 60;
            let text =
                alloc::format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}");
            object(vm.make_string(&text)?)
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
            let class = vm.resolve_class("java/time/LocalDateTime")?;
            if !vm.is_instance(other, class) {
                return crate::boolean(false);
            }
            let this = receiver(args);
            crate::boolean(
                read_long_field(vm, this, "epochDay") == read_long_field(vm, other, "epochDay")
                    && read_long_field(vm, this, "nanoOfDay")
                        == read_long_field(vm, other, "nanoOfDay"),
            )
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let epoch_day = read_long_field(vm, this, "epochDay");
        let nano = read_long_field(vm, this, "nanoOfDay");
        int(((epoch_day ^ (epoch_day >> 32)) as i32)
            .wrapping_mul(31)
            .wrapping_add((nano ^ (nano >> 32)) as i32))
    }),
    method(
        "isSupported",
        "(Ljava/time/temporal/TemporalField;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let ordinal = chrono_field_ordinal(vm, ref_arg(args, 1));
            crate::boolean(matches!(ordinal, Some(0..=27)))
        },
    ),
    method(
        "getLong",
        "(Ljava/time/temporal/TemporalField;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let ordinal =
                chrono_field_ordinal(vm, ref_arg(args, 1)).ok_or_else(|| unsupported_field(vm))?;
            let nano_of_day = read_long_field(vm, this, "nanoOfDay");
            if let Some(value) = time_field_long(nano_of_day, ordinal) {
                return long(value);
            }
            let epoch_day = read_long_field(vm, this, "epochDay");
            long(date_field_long(epoch_day, ordinal).ok_or_else(|| unsupported_field(vm))?)
        },
    ),
    method(
        "range",
        "(Ljava/time/temporal/TemporalField;)Ljava/time/temporal/ValueRange;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let epoch_day = read_long_field(vm, this, "epochDay");
            temporal_range(vm, ref_arg(args, 1), Some(epoch_day))
        },
    ),
    method(
        "isSupported",
        "(Ljava/time/temporal/TemporalUnit;)Z",
        ACC_PUBLIC,
        |vm, _, args| temporal_unit_supported(vm, ref_arg(args, 1), true, true),
    ),
    method(
        "plus",
        "(JLjava/time/temporal/TemporalUnit;)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let epoch_day = read_long_field(vm, this, "epochDay");
            let nano = read_long_field(vm, this, "nanoOfDay");
            let amount = crate::long_arg(args, 1);
            let ordinal =
                chrono_unit_ordinal(vm, ref_arg(args, 2)).ok_or_else(|| unsupported_unit(vm))?;
            let (new_epoch, new_nano) = if let Some(unit_nanos) = time_unit_nanos(ordinal) {
                let total = nano + amount * unit_nanos;
                (
                    epoch_day + total.div_euclid(NANOS_PER_DAY),
                    total.rem_euclid(NANOS_PER_DAY),
                )
            } else {
                (
                    date_plus(epoch_day, amount, ordinal).ok_or_else(|| unsupported_unit(vm))?,
                    nano,
                )
            };
            object(local_date_time_new(vm, new_epoch, new_nano)?)
        },
    ),
    method(
        "minus",
        "(JLjava/time/temporal/TemporalUnit;)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let epoch_day = read_long_field(vm, this, "epochDay");
            let nano = read_long_field(vm, this, "nanoOfDay");
            let amount = crate::long_arg(args, 1);
            let ordinal =
                chrono_unit_ordinal(vm, ref_arg(args, 2)).ok_or_else(|| unsupported_unit(vm))?;
            let (new_epoch, new_nano) = if let Some(unit_nanos) = time_unit_nanos(ordinal) {
                let total = nano - amount * unit_nanos;
                (
                    epoch_day + total.div_euclid(NANOS_PER_DAY),
                    total.rem_euclid(NANOS_PER_DAY),
                )
            } else {
                (
                    date_plus(epoch_day, -amount, ordinal).ok_or_else(|| unsupported_unit(vm))?,
                    nano,
                )
            };
            object(local_date_time_new(vm, new_epoch, new_nano)?)
        },
    ),
    method(
        "until",
        "(Ljava/time/temporal/Temporal;Ljava/time/temporal/TemporalUnit;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let end = ref_arg(args, 1);
            let start_epoch = read_long_field(vm, this, "epochDay");
            let start_nano = read_long_field(vm, this, "nanoOfDay");
            let end_epoch = read_long_field(vm, end, "epochDay");
            let end_nano = read_long_field(vm, end, "nanoOfDay");
            let ordinal =
                chrono_unit_ordinal(vm, ref_arg(args, 2)).ok_or_else(|| unsupported_unit(vm))?;
            if let Some(unit_nanos) = time_unit_nanos(ordinal) {
                let amount = (end_epoch - start_epoch) * NANOS_PER_DAY + (end_nano - start_nano);
                return long(amount / unit_nanos);
            }
            let mut adjusted_end = end_epoch;
            if end_epoch > start_epoch && end_nano < start_nano {
                adjusted_end -= 1;
            } else if end_epoch < start_epoch && end_nano > start_nano {
                adjusted_end += 1;
            }
            long(
                date_until(start_epoch, adjusted_end, ordinal)
                    .ok_or_else(|| unsupported_unit(vm))?,
            )
        },
    ),
    method(
        "atZone",
        "(Ljava/time/ZoneId;)Ljava/time/ZonedDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let zone = ref_arg(args, 1);
            if zone.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let epoch_day = read_long_field(vm, this, "epochDay");
            let nano_of_day = read_long_field(vm, this, "nanoOfDay");
            let local_second = epoch_day * 86_400 + nano_of_day / 1_000_000_000;
            let offset = zone_offset_seconds(vm, zone);
            zoned_date_time_new(vm, (local_second - i64::from(offset)) * 1000)
        },
    ),
];

/// The fixed offset (seconds) of a `ZoneId`/`ZoneOffset` argument (region transitions not modelled).
fn zone_offset_seconds(vm: &Vm<'_>, zone: ObjectRef) -> i32 {
    let class = vm.class_of(zone);
    vm.find_field_opt(class, "totalSeconds", "I")
        .and_then(|(declaring, field)| vm.read_instance_int(zone, declaring, field))
        .unwrap_or(0)
}

pub(crate) const LOCAL_DATE_TIME: oxjvm_vm::NativeClass = class(
    "java/time/LocalDateTime",
    Some("java/lang/Object"),
    &[
        "java/time/temporal/TemporalAccessor",
        "java/time/temporal/Temporal",
        "java/lang/Comparable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("epochDay", "J", ACC_PRIVATE, None),
        field("nanoOfDay", "J", ACC_PRIVATE, None),
    ],
    &LOCAL_DATE_TIME_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// DayOfWeek
// -------------------------------------------------------------------------------------------

const DAY_OF_WEEK_NAMES: [&str; 7] = [
    "MONDAY",
    "TUESDAY",
    "WEDNESDAY",
    "THURSDAY",
    "FRIDAY",
    "SATURDAY",
    "SUNDAY",
];

fn day_of_week_new(vm: &mut Vm<'_>, value: i32) -> Result<ObjectRef, VmError> {
    if !(1..=7).contains(&value) {
        return Err(vm.throw_new("java/time/DateTimeException", Some("Invalid day of week")));
    }
    let class = vm.resolve_class("java/time/DayOfWeek")?;
    let instance = vm.new_instance(class)?;
    write_int_field(vm, instance, "value", value);
    Ok(instance)
}

const DAY_OF_WEEK_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "of",
        "(I)Ljava/time/DayOfWeek;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(day_of_week_new(vm, crate::int_arg(args, 0))?),
    ),
    method("getValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "value"))
    }),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let value = read_int_field(vm, receiver(args), "value");
        let name = DAY_OF_WEEK_NAMES
            .get((value - 1) as usize)
            .copied()
            .unwrap_or("");
        object(vm.make_string(name)?)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = read_int_field(vm, receiver(args), "value");
            let name = DAY_OF_WEEK_NAMES
                .get((value - 1) as usize)
                .copied()
                .unwrap_or("");
            object(vm.make_string(name)?)
        },
    ),
    method(
        "plus",
        "(J)Ljava/time/DayOfWeek;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = read_int_field(vm, receiver(args), "value");
            let shifted = (i64::from(value - 1) + crate::long_arg(args, 1)).rem_euclid(7) + 1;
            object(day_of_week_new(vm, shifted as i32)?)
        },
    ),
    method(
        "minus",
        "(J)Ljava/time/DayOfWeek;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = read_int_field(vm, receiver(args), "value");
            let shifted = (i64::from(value - 1) - crate::long_arg(args, 1)).rem_euclid(7) + 1;
            object(day_of_week_new(vm, shifted as i32)?)
        },
    ),
];

pub(crate) const DAY_OF_WEEK: oxjvm_vm::NativeClass = class(
    "java/time/DayOfWeek",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[field("value", "I", ACC_PRIVATE, None)],
    &DAY_OF_WEEK_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Month
// -------------------------------------------------------------------------------------------

const MONTH_NAMES: [&str; 12] = [
    "JANUARY",
    "FEBRUARY",
    "MARCH",
    "APRIL",
    "MAY",
    "JUNE",
    "JULY",
    "AUGUST",
    "SEPTEMBER",
    "OCTOBER",
    "NOVEMBER",
    "DECEMBER",
];

fn month_new(vm: &mut Vm<'_>, value: i32) -> Result<ObjectRef, VmError> {
    if !(1..=12).contains(&value) {
        return Err(vm.throw_new("java/time/DateTimeException", Some("Invalid month")));
    }
    let class = vm.resolve_class("java/time/Month")?;
    let instance = vm.new_instance(class)?;
    write_int_field(vm, instance, "value", value);
    Ok(instance)
}

fn month_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/time/Month")?;
    for (index, name) in MONTH_NAMES.iter().enumerate() {
        let instance = month_new(vm, index as i32 + 1)?;
        vm.set_static_value(class, name, "Ljava/time/Month;", Value::Ref(instance));
    }
    Ok(())
}

const MONTH_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "of",
        "(I)Ljava/time/Month;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(month_new(vm, crate::int_arg(args, 0))?),
    ),
    method("getValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "value"))
    }),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let value = read_int_field(vm, receiver(args), "value").clamp(1, 12) as usize;
        object(vm.make_string(MONTH_NAMES[value - 1])?)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = read_int_field(vm, receiver(args), "value").clamp(1, 12) as usize;
            object(vm.make_string(MONTH_NAMES[value - 1])?)
        },
    ),
    method("plus", "(J)Ljava/time/Month;", ACC_PUBLIC, |vm, _, args| {
        let value = read_int_field(vm, receiver(args), "value");
        let amount = crate::long_arg(args, 1);
        let shifted = (i64::from(value - 1) + amount).rem_euclid(12) + 1;
        object(month_new(vm, shifted as i32)?)
    }),
    method(
        "minus",
        "(J)Ljava/time/Month;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = read_int_field(vm, receiver(args), "value");
            let amount = crate::long_arg(args, 1);
            let shifted = (i64::from(value - 1) - amount).rem_euclid(12) + 1;
            object(month_new(vm, shifted as i32)?)
        },
    ),
    method(
        "values",
        "()[Ljava/time/Month;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/time/Month")?;
            vm.ensure_initialized(class)?;
            let array = vm.allocate_object_array(class, MONTH_NAMES.len())?;
            for (index, name) in MONTH_NAMES.iter().enumerate() {
                if let Some(value) = vm.read_static_ref_if_set(class, name, "Ljava/time/Month;") {
                    vm.array_set_ref(array, index, value)?;
                }
            }
            object(array)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/time/Month;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 0));
            let class = vm.resolve_class("java/time/Month")?;
            vm.ensure_initialized(class)?;
            for candidate in MONTH_NAMES {
                if candidate == name {
                    if let Some(value) =
                        vm.read_static_ref_if_set(class, candidate, "Ljava/time/Month;")
                    {
                        return object(value);
                    }
                }
            }
            Err(vm.throw_new("java/lang/IllegalArgumentException", None))
        },
    ),
    method(
        "isSupported",
        "(Ljava/time/temporal/TemporalField;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let ordinal = chrono_field_ordinal(vm, ref_arg(args, 1));
            crate::boolean(matches!(ordinal, Some(23)))
        },
    ),
    method(
        "getLong",
        "(Ljava/time/temporal/TemporalField;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let ordinal =
                chrono_field_ordinal(vm, ref_arg(args, 1)).ok_or_else(|| unsupported_field(vm))?;
            if ordinal != 23 {
                return Err(unsupported_field(vm));
            }
            long(i64::from(read_int_field(vm, receiver(args), "value")))
        },
    ),
];

pub(crate) const MONTH: oxjvm_vm::NativeClass = class(
    "java/time/Month",
    Some("java/lang/Object"),
    &[
        "java/time/temporal/TemporalAccessor",
        "java/time/temporal/TemporalAdjuster",
    ],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("value", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "JANUARY",
            "Ljava/time/Month;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "FEBRUARY",
            "Ljava/time/Month;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MARCH",
            "Ljava/time/Month;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "APRIL",
            "Ljava/time/Month;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MAY",
            "Ljava/time/Month;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "JUNE",
            "Ljava/time/Month;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "JULY",
            "Ljava/time/Month;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "AUGUST",
            "Ljava/time/Month;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SEPTEMBER",
            "Ljava/time/Month;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "OCTOBER",
            "Ljava/time/Month;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "NOVEMBER",
            "Ljava/time/Month;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "DECEMBER",
            "Ljava/time/Month;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &MONTH_METHODS,
    Some(month_clinit),
);

// -------------------------------------------------------------------------------------------
// java.time.format.ResolverStyle / TextStyle
// -------------------------------------------------------------------------------------------

const RESOLVER_STYLE_NAMES: [&str; 3] = ["STRICT", "SMART", "LENIENT"];

fn resolver_style_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/time/format/ResolverStyle")?;
    for (index, name) in RESOLVER_STYLE_NAMES.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        if let Some((declaring, field)) = vm.find_field_opt(class, "ordinal", "I") {
            vm.set_instance_int(instance, declaring, field, index as i32);
        }
        vm.set_static_value(
            class,
            name,
            "Ljava/time/format/ResolverStyle;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

const RESOLVER_STYLE_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "ordinal"))
    }),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let index = read_int_field(vm, receiver(args), "ordinal").clamp(0, 2) as usize;
        object(vm.make_string(RESOLVER_STYLE_NAMES[index])?)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let index = read_int_field(vm, receiver(args), "ordinal").clamp(0, 2) as usize;
            object(vm.make_string(RESOLVER_STYLE_NAMES[index])?)
        },
    ),
    method(
        "values",
        "()[Ljava/time/format/ResolverStyle;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/time/format/ResolverStyle")?;
            vm.ensure_initialized(class)?;
            let array = vm.allocate_object_array(class, RESOLVER_STYLE_NAMES.len())?;
            for (index, name) in RESOLVER_STYLE_NAMES.iter().enumerate() {
                if let Some(value) =
                    vm.read_static_ref_if_set(class, name, "Ljava/time/format/ResolverStyle;")
                {
                    vm.array_set_ref(array, index, value)?;
                }
            }
            object(array)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/time/format/ResolverStyle;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 0));
            let class = vm.resolve_class("java/time/format/ResolverStyle")?;
            vm.ensure_initialized(class)?;
            for candidate in RESOLVER_STYLE_NAMES {
                if candidate == name {
                    if let Some(value) = vm.read_static_ref_if_set(
                        class,
                        candidate,
                        "Ljava/time/format/ResolverStyle;",
                    ) {
                        return object(value);
                    }
                }
            }
            Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some(&alloc::format!(
                    "No enum constant java.time.format.ResolverStyle.{name}"
                )),
            ))
        },
    ),
];

/// `java.time.format.ResolverStyle`: how strictly a formatter resolves parsed fields.
pub(crate) const RESOLVER_STYLE: oxjvm_vm::NativeClass = class(
    "java/time/format/ResolverStyle",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "STRICT",
            "Ljava/time/format/ResolverStyle;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "SMART",
            "Ljava/time/format/ResolverStyle;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "LENIENT",
            "Ljava/time/format/ResolverStyle;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
    ],
    &RESOLVER_STYLE_METHODS,
    Some(resolver_style_clinit),
);

const TEXT_STYLE_NAMES: [&str; 6] = [
    "FULL",
    "FULL_STANDALONE",
    "SHORT",
    "SHORT_STANDALONE",
    "NARROW",
    "NARROW_STANDALONE",
];

fn text_style_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/time/format/TextStyle")?;
    for (index, name) in TEXT_STYLE_NAMES.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        if let Some((declaring, field)) = vm.find_field_opt(class, "ordinal", "I") {
            vm.set_instance_int(instance, declaring, field, index as i32);
        }
        vm.set_static_value(
            class,
            name,
            "Ljava/time/format/TextStyle;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

fn text_style_constant(vm: &mut Vm<'_>, index: usize) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/time/format/TextStyle")?;
    vm.ensure_initialized(class)?;
    object(
        vm.read_static_ref_if_set(
            class,
            TEXT_STYLE_NAMES[index],
            "Ljava/time/format/TextStyle;",
        )
        .unwrap_or(ObjectRef::NULL),
    )
}

const TEXT_STYLE_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "ordinal"))
    }),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let index = read_int_field(vm, receiver(args), "ordinal").clamp(0, 5) as usize;
        object(vm.make_string(TEXT_STYLE_NAMES[index])?)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let index = read_int_field(vm, receiver(args), "ordinal").clamp(0, 5) as usize;
            object(vm.make_string(TEXT_STYLE_NAMES[index])?)
        },
    ),
    method("isStandalone", "()Z", ACC_PUBLIC, |vm, _, args| {
        let index = read_int_field(vm, receiver(args), "ordinal");
        crate::boolean(index % 2 == 1)
    }),
    method(
        "asStandalone",
        "()Ljava/time/format/TextStyle;",
        ACC_PUBLIC,
        |vm, _, args| {
            let index = read_int_field(vm, receiver(args), "ordinal").clamp(0, 5) as usize;
            text_style_constant(vm, index | 1)
        },
    ),
    method(
        "asNormal",
        "()Ljava/time/format/TextStyle;",
        ACC_PUBLIC,
        |vm, _, args| {
            let index = read_int_field(vm, receiver(args), "ordinal").clamp(0, 5) as usize;
            text_style_constant(vm, index & !1)
        },
    ),
    method(
        "values",
        "()[Ljava/time/format/TextStyle;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/time/format/TextStyle")?;
            vm.ensure_initialized(class)?;
            let array = vm.allocate_object_array(class, TEXT_STYLE_NAMES.len())?;
            for (index, name) in TEXT_STYLE_NAMES.iter().enumerate() {
                if let Some(value) =
                    vm.read_static_ref_if_set(class, name, "Ljava/time/format/TextStyle;")
                {
                    vm.array_set_ref(array, index, value)?;
                }
            }
            object(array)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/time/format/TextStyle;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 0));
            let class = vm.resolve_class("java/time/format/TextStyle")?;
            vm.ensure_initialized(class)?;
            for candidate in TEXT_STYLE_NAMES {
                if candidate == name {
                    if let Some(value) =
                        vm.read_static_ref_if_set(class, candidate, "Ljava/time/format/TextStyle;")
                    {
                        return object(value);
                    }
                }
            }
            Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some(&alloc::format!(
                    "No enum constant java.time.format.TextStyle.{name}"
                )),
            ))
        },
    ),
];

/// `java.time.format.TextStyle`: the width of a localized textual field.
pub(crate) const TEXT_STYLE: oxjvm_vm::NativeClass = class(
    "java/time/format/TextStyle",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "FULL",
            "Ljava/time/format/TextStyle;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "FULL_STANDALONE",
            "Ljava/time/format/TextStyle;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "SHORT",
            "Ljava/time/format/TextStyle;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "SHORT_STANDALONE",
            "Ljava/time/format/TextStyle;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "NARROW",
            "Ljava/time/format/TextStyle;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "NARROW_STANDALONE",
            "Ljava/time/format/TextStyle;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
    ],
    &TEXT_STYLE_METHODS,
    Some(text_style_clinit),
);

// -------------------------------------------------------------------------------------------
// java.time.zone.ZoneRules (fixed-offset model)
// -------------------------------------------------------------------------------------------

fn zone_rules_offset(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let total = read_int_field(vm, this, "totalSeconds");
    object(crate::lang::zone_offset_new(vm, total)?)
}

const ZONE_RULES_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "getOffset",
        "(Ljava/time/Instant;)Ljava/time/ZoneOffset;",
        ACC_PUBLIC,
        |vm, _, args| zone_rules_offset(vm, receiver(args)),
    ),
    method(
        "getOffset",
        "(Ljava/time/LocalDateTime;)Ljava/time/ZoneOffset;",
        ACC_PUBLIC,
        |vm, _, args| zone_rules_offset(vm, receiver(args)),
    ),
    method(
        "getStandardOffset",
        "(Ljava/time/Instant;)Ljava/time/ZoneOffset;",
        ACC_PUBLIC,
        |vm, _, args| zone_rules_offset(vm, receiver(args)),
    ),
    method("isFixedOffset", "()Z", ACC_PUBLIC, |_, _, _| {
        crate::boolean(true)
    }),
];

/// `java.time.zone.ZoneRules`: modelled as a single fixed offset (transition data not modelled).
pub(crate) const ZONE_RULES: oxjvm_vm::NativeClass = class(
    "java/time/zone/ZoneRules",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_FINAL,
    &[field("totalSeconds", "I", ACC_PRIVATE, None)],
    &ZONE_RULES_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// java.time.OffsetDateTime
// -------------------------------------------------------------------------------------------

fn offset_date_time_new(
    vm: &mut Vm<'_>,
    epoch_day: i64,
    nano_of_day: i64,
    offset: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/time/OffsetDateTime")?;
    let instance = vm.new_instance(class)?;
    write_long_field(vm, instance, "epochDay", epoch_day);
    write_long_field(vm, instance, "nanoOfDay", nano_of_day);
    let (declaring, field) = vm.find_field(class, "offset", "Ljava/time/ZoneOffset;")?;
    vm.set_instance_ref(instance, declaring, field, offset);
    Ok(instance)
}

fn offset_date_time_offset(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/ZoneOffset;")
        .unwrap_or(ObjectRef::NULL)
}

fn offset_date_time_epoch_second(vm: &Vm<'_>, this: ObjectRef) -> i64 {
    let epoch_day = read_long_field(vm, this, "epochDay");
    let nano_of_day = read_long_field(vm, this, "nanoOfDay");
    let offset = zone_offset_seconds(vm, offset_date_time_offset(vm, this));
    epoch_day * 86_400 + nano_of_day / 1_000_000_000 - i64::from(offset)
}

const OFFSET_DATE_TIME_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    method(
        "of",
        "(IIIIIIILjava/time/ZoneOffset;)Ljava/time/OffsetDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let year = crate::int_arg(args, 0);
            let month = crate::int_arg(args, 1);
            let day = crate::int_arg(args, 2);
            let hour = crate::int_arg(args, 3);
            let minute = crate::int_arg(args, 4);
            let second = crate::int_arg(args, 5);
            let nano = crate::int_arg(args, 6);
            let offset = ref_arg(args, 7);
            let epoch_day = days_from_civil(year, month, day);
            let nano_of_day = (i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second))
                * 1_000_000_000
                + i64::from(nano);
            object(offset_date_time_new(vm, epoch_day, nano_of_day, offset)?)
        },
    ),
    method(
        "of",
        "(Ljava/time/LocalDateTime;Ljava/time/ZoneOffset;)Ljava/time/OffsetDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let date_time = ref_arg(args, 0);
            object(offset_date_time_new(
                vm,
                read_long_field(vm, date_time, "epochDay"),
                read_long_field(vm, date_time, "nanoOfDay"),
                ref_arg(args, 1),
            )?)
        },
    ),
    method("getYear", "()I", ACC_PUBLIC, |vm, _, args| {
        let (year, _, _) = civil_from_days(read_long_field(vm, receiver(args), "epochDay"));
        int(year)
    }),
    method("getMonthValue", "()I", ACC_PUBLIC, |vm, _, args| {
        let (_, month, _) = civil_from_days(read_long_field(vm, receiver(args), "epochDay"));
        int(month)
    }),
    method("getDayOfMonth", "()I", ACC_PUBLIC, |vm, _, args| {
        let (_, _, day) = civil_from_days(read_long_field(vm, receiver(args), "epochDay"));
        int(day)
    }),
    method("getHour", "()I", ACC_PUBLIC, |vm, _, args| {
        int((read_long_field(vm, receiver(args), "nanoOfDay") / 3_600_000_000_000) as i32)
    }),
    method("getMinute", "()I", ACC_PUBLIC, |vm, _, args| {
        int(((read_long_field(vm, receiver(args), "nanoOfDay") / 60_000_000_000) % 60) as i32)
    }),
    method("getSecond", "()I", ACC_PUBLIC, |vm, _, args| {
        int(((read_long_field(vm, receiver(args), "nanoOfDay") / 1_000_000_000) % 60) as i32)
    }),
    method("getNano", "()I", ACC_PUBLIC, |vm, _, args| {
        int((read_long_field(vm, receiver(args), "nanoOfDay") % 1_000_000_000) as i32)
    }),
    method(
        "getOffset",
        "()Ljava/time/ZoneOffset;",
        ACC_PUBLIC,
        |vm, _, args| object(offset_date_time_offset(vm, receiver(args))),
    ),
    method("toEpochSecond", "()J", ACC_PUBLIC, |vm, _, args| {
        long(offset_date_time_epoch_second(vm, receiver(args)))
    }),
    method(
        "toInstant",
        "()Ljava/time/Instant;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let nano = (read_long_field(vm, this, "nanoOfDay") % 1_000_000_000) as i32;
            instant_new(vm, offset_date_time_epoch_second(vm, this), nano)
        },
    ),
    method(
        "toLocalDateTime",
        "()Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(local_date_time_new(
                vm,
                read_long_field(vm, this, "epochDay"),
                read_long_field(vm, this, "nanoOfDay"),
            )?)
        },
    ),
    method(
        "toLocalDate",
        "()Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.resolve_class("java/time/LocalDate")?;
            let instance = vm.new_instance(class)?;
            write_long_field(
                vm,
                instance,
                "epochDay",
                read_long_field(vm, this, "epochDay"),
            );
            object(instance)
        },
    ),
];

/// `java.time.OffsetDateTime`: a local date-time plus a fixed zone offset.
pub(crate) const OFFSET_DATE_TIME: oxjvm_vm::NativeClass = class(
    "java/time/OffsetDateTime",
    Some("java/lang/Object"),
    &[
        "java/time/temporal/TemporalAccessor",
        "java/time/temporal/Temporal",
        "java/lang/Comparable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("epochDay", "J", ACC_PRIVATE, None),
        field("nanoOfDay", "J", ACC_PRIVATE, None),
        field("offset", "Ljava/time/ZoneOffset;", ACC_PRIVATE, None),
    ],
    &OFFSET_DATE_TIME_METHODS,
    None,
);
