//! A minimal `java.time.Duration` / `java.time.Instant` (nanosecond precision, no calendars).

use alloc::vec;
use alloc::vec::Vec;

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

const LOCAL_DATE_METHODS: [oxjvm_vm::NativeMethodDef; 24] = [
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
];

pub(crate) const LOCAL_DATE: oxjvm_vm::NativeClass = class(
    "java/time/LocalDate",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[field("epochDay", "J", ACC_PRIVATE, None)],
    &LOCAL_DATE_METHODS,
    None,
);

const LOCAL_TIME_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
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
];

pub(crate) const LOCAL_TIME: oxjvm_vm::NativeClass = class(
    "java/time/LocalTime",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[field("nanoOfDay", "J", ACC_PRIVATE, None)],
    &LOCAL_TIME_METHODS,
    None,
);

const LOCAL_DATE_TIME_METHODS: [oxjvm_vm::NativeMethodDef; 18] = [
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
];

pub(crate) const LOCAL_DATE_TIME: oxjvm_vm::NativeClass = class(
    "java/time/LocalDateTime",
    Some("java/lang/Object"),
    &[],
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
