//! A minimal `java.time.Duration` / `java.time.Instant` (nanosecond precision, no calendars).

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::lang::{read_int_field, read_long_field, write_int_field, write_long_field};
use crate::{boolean, class, field, int, int_arg, long, method, object, receiver, ref_arg};

/// Parse an ISO-8601 duration such as `PT1H30M`, `P2DT3H4M0.5S` or `-PT0.5S`.
fn duration_parse(vm: &mut Vm<'_>, text: &str) -> Result<Value, VmError> {
    let error =
        |vm: &mut Vm<'_>| vm.throw_new("java/time/format/DateTimeParseException", Some(text));
    let bytes = text.as_bytes();
    if bytes.len() < 2 {
        return Err(error(vm));
    }
    let mut index = 0;
    let negative = match bytes[0] {
        b'+' => {
            index = 1;
            false
        }
        b'-' => {
            index = 1;
            true
        }
        _ => false,
    };
    if bytes.get(index) != Some(&b'P') {
        return Err(error(vm));
    }
    index += 1;
    let mut nanos = 0i64;
    let mut in_time = false;
    let mut saw_any = false;
    while index < bytes.len() {
        if bytes[index] == b'T' {
            if in_time {
                return Err(error(vm));
            }
            in_time = true;
            index += 1;
            if index >= bytes.len() {
                return Err(error(vm));
            }
            continue;
        }
        let start = index;
        if bytes[index] == b'+' || bytes[index] == b'-' {
            index += 1;
        }
        let mut digits = String::new();
        while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'.') {
            digits.push(bytes[index] as char);
            index += 1;
        }
        if digits.is_empty() || index >= bytes.len() {
            return Err(error(vm));
        }
        let unit = bytes[index];
        index += 1;
        let value: f64 = digits.parse().map_err(|_| error(vm))?;
        let scale = match (in_time, unit) {
            (false, b'D') => 86_400_000_000_000.0,
            (true, b'H') => 3_600_000_000_000.0,
            (true, b'M') => 60_000_000_000.0,
            (true, b'S') => 1_000_000_000.0,
            _ => return Err(error(vm)),
        };
        if unit != b'S' && digits.contains('.') {
            return Err(error(vm));
        }
        nanos = nanos.saturating_add((value * scale) as i64);
        saw_any = true;
        let _ = start;
    }
    if !saw_any {
        return Err(error(vm));
    }
    if negative {
        nanos = -nanos;
    }
    duration_new(vm, nanos)
}

const DURATION_METHODS: [oxjvm_vm::NativeMethodDef; 46] = [
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
        "between",
        "(Ljava/time/temporal/Temporal;Ljava/time/temporal/Temporal;)Ljava/time/Duration;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let start = ref_arg(args, 0);
            let end = ref_arg(args, 1);
            if start.is_null() || end.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let (start_day, start_nano) = temporal_day_nano(vm, start)?;
            let (end_day, end_nano) = temporal_day_nano(vm, end)?;
            let nano_delta = end_nano - start_nano;
            let secs = (end_day - start_day) * 86_400 + nano_delta / 1_000_000_000;
            duration_of_seconds(vm, secs, nano_delta % 1_000_000_000)
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
        boolean(read_long_field(vm, receiver(args), "nanos") == 0)
    }),
    method("isNegative", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(read_long_field(vm, receiver(args), "nanos") < 0)
    }),
    method(
        "plusMillis",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            duration_shift(
                vm,
                receiver(args),
                crate::long_arg(args, 1).saturating_mul(1_000_000),
            )
        },
    ),
    method(
        "minusMillis",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            duration_shift(
                vm,
                receiver(args),
                -crate::long_arg(args, 1).saturating_mul(1_000_000),
            )
        },
    ),
    method(
        "plusNanos",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| duration_shift(vm, receiver(args), crate::long_arg(args, 1)),
    ),
    method(
        "minusNanos",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| duration_shift(vm, receiver(args), -crate::long_arg(args, 1)),
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
                return boolean(false);
            }
            let class = vm.resolve_class("java/time/Duration")?;
            if !vm.is_instance(other, class) {
                return boolean(false);
            }
            boolean(
                read_long_field(vm, receiver(args), "nanos") == read_long_field(vm, other, "nanos"),
            )
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let nanos = read_long_field(vm, receiver(args), "nanos");
        let seconds = nanos.div_euclid(1_000_000_000);
        let nano = nanos.rem_euclid(1_000_000_000) as i32;
        int(instant_hash(seconds, nano))
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
    method(
        "plusSeconds",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            duration_shift(
                vm,
                receiver(args),
                crate::long_arg(args, 1).saturating_mul(1_000_000_000),
            )
        },
    ),
    method(
        "minusSeconds",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            duration_shift(
                vm,
                receiver(args),
                -crate::long_arg(args, 1).saturating_mul(1_000_000_000),
            )
        },
    ),
    method(
        "plusMinutes",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            duration_shift(
                vm,
                receiver(args),
                crate::long_arg(args, 1).saturating_mul(60_000_000_000),
            )
        },
    ),
    method(
        "minusMinutes",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            duration_shift(
                vm,
                receiver(args),
                -crate::long_arg(args, 1).saturating_mul(60_000_000_000),
            )
        },
    ),
    method(
        "plusHours",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            duration_shift(
                vm,
                receiver(args),
                crate::long_arg(args, 1).saturating_mul(3_600_000_000_000),
            )
        },
    ),
    method(
        "minusHours",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            duration_shift(
                vm,
                receiver(args),
                -crate::long_arg(args, 1).saturating_mul(3_600_000_000_000),
            )
        },
    ),
    method(
        "plusDays",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            duration_shift(
                vm,
                receiver(args),
                crate::long_arg(args, 1).saturating_mul(86_400_000_000_000),
            )
        },
    ),
    method(
        "minusDays",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            duration_shift(
                vm,
                receiver(args),
                -crate::long_arg(args, 1).saturating_mul(86_400_000_000_000),
            )
        },
    ),
    method(
        "plus",
        "(Ljava/time/Duration;)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            duration_shift(
                vm,
                receiver(args),
                read_long_field(vm, ref_arg(args, 1), "nanos"),
            )
        },
    ),
    method(
        "minus",
        "(Ljava/time/Duration;)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            duration_shift(
                vm,
                receiver(args),
                -read_long_field(vm, ref_arg(args, 1), "nanos"),
            )
        },
    ),
    method(
        "abs",
        "()Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let nanos = read_long_field(vm, this, "nanos");
            if nanos >= 0 {
                return object(this);
            }
            duration_new(vm, -nanos)
        },
    ),
    method(
        "negated",
        "()Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| duration_new(vm, -read_long_field(vm, receiver(args), "nanos")),
    ),
    method(
        "multipliedBy",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let nanos = read_long_field(vm, receiver(args), "nanos")
                .saturating_mul(crate::long_arg(args, 1));
            duration_new(vm, nanos)
        },
    ),
    method(
        "dividedBy",
        "(J)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let divisor = crate::long_arg(args, 1);
            if divisor == 0 {
                return Err(vm.throw_new("java/lang/ArithmeticException", Some("/ by zero")));
            }
            duration_new(vm, read_long_field(vm, receiver(args), "nanos") / divisor)
        },
    ),
    method("toSecondsPart", "()I", ACC_PUBLIC, |vm, _, args| {
        int(((read_long_field(vm, receiver(args), "nanos") / 1_000_000_000) % 60) as i32)
    }),
    method("toMinutesPart", "()I", ACC_PUBLIC, |vm, _, args| {
        int(((read_long_field(vm, receiver(args), "nanos") / 60_000_000_000) % 60) as i32)
    }),
    method("toHoursPart", "()I", ACC_PUBLIC, |vm, _, args| {
        int(((read_long_field(vm, receiver(args), "nanos") / 3_600_000_000_000) % 24) as i32)
    }),
    method("toDaysPart", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "nanos") / 86_400_000_000_000)
    }),
    method("toSeconds", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "nanos") / 1_000_000_000)
    }),
    method("toMinutes", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "nanos") / 60_000_000_000)
    }),
    method("toHours", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "nanos") / 3_600_000_000_000)
    }),
    method("toDays", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "nanos") / 86_400_000_000_000)
    }),
    method(
        "parse",
        "(Ljava/lang/CharSequence;)Ljava/time/Duration;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = crate::string_arg(vm, args[0])?;
            duration_parse(vm, &text)
        },
    ),
];

/// `Duration.toString`: `PT{h}H{m}M{s}S` with zero parts omitted, matching the
/// reference implementation (including the negative-second fraction form).
fn format_duration(nanos: i64) -> String {
    if nanos == 0 {
        return String::from("PT0S");
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
    let mut text = String::from("PT");
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

/// Build a `Duration` from a seconds/nanos pair (as `Duration.ofSeconds(JJ)`).
/// The local epoch-day and nano-of-day of a temporal, for duration arithmetic.
fn temporal_day_nano(vm: &mut Vm<'_>, obj: ObjectRef) -> Result<(i64, i64), VmError> {
    let name = vm.class_name(vm.class_of(obj)).to_string();
    match name.as_str() {
        "java/time/LocalDate" => Ok((read_long_field(vm, obj, "epochDay"), 0)),
        "java/time/LocalTime" | "java/time/OffsetTime" => {
            Ok((0, read_long_field(vm, obj, "nanoOfDay")))
        }
        "java/time/LocalDateTime" | "java/time/OffsetDateTime" => Ok((
            read_long_field(vm, obj, "epochDay"),
            read_long_field(vm, obj, "nanoOfDay"),
        )),
        "java/time/ZonedDateTime" => Ok(zdt_local_parts(vm, obj)),
        "java/time/Instant" => {
            let seconds = read_long_field(vm, obj, "epochSecond");
            let nano = i64::from(read_int_field(vm, obj, "nano"));
            Ok((
                seconds.div_euclid(86_400),
                seconds.rem_euclid(86_400) * 1_000_000_000 + nano,
            ))
        }
        _ => Err(vm.throw_new(
            "java/time/DateTimeException",
            Some("Unsupported temporal type for duration"),
        )),
    }
}

fn duration_of_seconds(vm: &mut Vm<'_>, seconds: i64, nano: i64) -> Result<Value, VmError> {
    duration_new(
        vm,
        seconds.saturating_mul(1_000_000_000).saturating_add(nano),
    )
}

fn duration_new(vm: &mut Vm<'_>, nanos: i64) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/time/Duration")?;
    // Zero durations share the `Duration.ZERO` singleton, as in the JDK.
    if nanos == 0 {
        if let Some(zero) = vm.read_static_ref_if_set(class, "ZERO", "Ljava/time/Duration;") {
            return object(zero);
        }
    }
    let instance = vm.new_instance(class)?;
    write_long_field(vm, instance, "nanos", nanos);
    object(instance)
}

/// Add a nanosecond delta to a `Duration`, returning the receiver unchanged when
/// the delta is zero (the JDK returns `this` for no-op arithmetic).
fn duration_shift(vm: &mut Vm<'_>, this: ObjectRef, delta: i64) -> Result<Value, VmError> {
    if delta == 0 {
        return object(this);
    }
    duration_new(vm, read_long_field(vm, this, "nanos").saturating_add(delta))
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
        field("nanos", "J", ACC_PRIVATE | ACC_FINAL, None),
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

const INSTANT_METHODS: [oxjvm_vm::NativeMethodDef; 31] = [
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
        "now",
        "(Ljava/time/Clock;)Ljava/time/Instant;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let (second, nano) = clock_instant_parts(vm, ref_arg(args, 0));
            instant_new(vm, second, nano)
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
    method(
        "ofEpochSecond",
        "(JJ)Ljava/time/Instant;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let seconds = crate::long_arg(args, 0);
            let nano_adjustment = crate::long_arg(args, 1);
            let total = seconds
                .saturating_mul(1_000_000_000)
                .saturating_add(nano_adjustment);
            instant_new(
                vm,
                total.div_euclid(1_000_000_000),
                total.rem_euclid(1_000_000_000) as i32,
            )
        },
    ),
    method(
        "plusSeconds",
        "(J)Ljava/time/Instant;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            instant_new(
                vm,
                read_long_field(vm, this, "epochSecond").saturating_add(crate::long_arg(args, 1)),
                read_int_field(vm, this, "nano"),
            )
        },
    ),
    method(
        "minusSeconds",
        "(J)Ljava/time/Instant;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            instant_new(
                vm,
                read_long_field(vm, this, "epochSecond").saturating_sub(crate::long_arg(args, 1)),
                read_int_field(vm, this, "nano"),
            )
        },
    ),
    method(
        "plusNanos",
        "(J)Ljava/time/Instant;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let total = i64::from(read_int_field(vm, this, "nano"))
                .saturating_add(crate::long_arg(args, 1));
            instant_new(
                vm,
                read_long_field(vm, this, "epochSecond")
                    .saturating_add(total.div_euclid(1_000_000_000)),
                total.rem_euclid(1_000_000_000) as i32,
            )
        },
    ),
    method(
        "minusNanos",
        "(J)Ljava/time/Instant;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let total = i64::from(read_int_field(vm, this, "nano"))
                .saturating_sub(crate::long_arg(args, 1));
            instant_new(
                vm,
                read_long_field(vm, this, "epochSecond")
                    .saturating_add(total.div_euclid(1_000_000_000)),
                total.rem_euclid(1_000_000_000) as i32,
            )
        },
    ),
    method(
        "parse",
        "(Ljava/lang/CharSequence;)Ljava/time/Instant;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = crate::string_arg(vm, args[0])?;
            instant_parse(vm, &text)
        },
    ),
    method("getEpochSecond", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "epochSecond"))
    }),
    method("getNano", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "nano"))
    }),
    method("toEpochMilli", "()J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let seconds = read_long_field(vm, this, "epochSecond");
        let nanos = i64::from(read_int_field(vm, this, "nano"));
        // Mirror the JDK: for negative seconds with a positive nano part, shift
        // by one second so the multiplication cannot overflow prematurely.
        let (millis, adjustment) = if seconds < 0 && nanos > 0 {
            (
                seconds
                    .checked_add(1)
                    .and_then(|s| s.checked_mul(1000))
                    .ok_or_else(|| arithmetic_overflow(vm))?,
                nanos / 1_000_000 - 1000,
            )
        } else {
            (
                seconds
                    .checked_mul(1000)
                    .ok_or_else(|| arithmetic_overflow(vm))?,
                nanos / 1_000_000,
            )
        };
        long(
            millis
                .checked_add(adjustment)
                .ok_or_else(|| arithmetic_overflow(vm))?,
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
        "plus",
        "(Ljava/time/temporal/TemporalAmount;)Ljava/time/Instant;",
        ACC_PUBLIC,
        |vm, _, args| instant_shift_amount(vm, receiver(args), ref_arg(args, 1), false),
    ),
    method(
        "minus",
        "(Ljava/time/temporal/TemporalAmount;)Ljava/time/Instant;",
        ACC_PUBLIC,
        |vm, _, args| instant_shift_amount(vm, receiver(args), ref_arg(args, 1), true),
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
            boolean(ordering.is_gt())
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
            boolean(ordering.is_lt())
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
                return boolean(false);
            }
            let class = vm.resolve_class("java/time/Instant")?;
            if !vm.is_instance(other, class) {
                return boolean(false);
            }
            let this = receiver(args);
            boolean(
                read_long_field(vm, this, "epochSecond")
                    == read_long_field(vm, other, "epochSecond")
                    && read_int_field(vm, this, "nano") == read_int_field(vm, other, "nano"),
            )
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let seconds = read_long_field(vm, this, "epochSecond");
        int(instant_hash(seconds, read_int_field(vm, this, "nano")))
    }),
    method(
        "isSupported",
        "(Ljava/time/temporal/TemporalField;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let ordinal = chrono_field_ordinal(vm, ref_arg(args, 1));
            boolean(matches!(ordinal, Some(0 | 2 | 4 | 28)))
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
    method(
        "until",
        "(Ljava/time/temporal/Temporal;Ljava/time/temporal/TemporalUnit;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let end = ref_arg(args, 1);
            let start_s = read_long_field(vm, this, "epochSecond");
            let start_n = read_int_field(vm, this, "nano");
            let end_s = read_long_field(vm, end, "epochSecond");
            let end_n = read_int_field(vm, end, "nano");
            let ordinal =
                chrono_unit_ordinal(vm, ref_arg(args, 2)).ok_or_else(|| unsupported_unit(vm))?;
            let result = match ordinal {
                0 => end_s
                    .checked_sub(start_s)
                    .and_then(|s| s.checked_mul(1_000_000_000))
                    .and_then(|n| n.checked_add(i64::from(end_n) - i64::from(start_n))),
                1 => instant_micros_until(start_s, start_n, end_s, end_n),
                2 => instant_millis_until(start_s, start_n, end_s, end_n),
                3 => instant_seconds_until(start_s, start_n, end_s, end_n),
                4 => instant_seconds_until(start_s, start_n, end_s, end_n).map(|s| s / 60),
                5 => instant_seconds_until(start_s, start_n, end_s, end_n).map(|s| s / 3600),
                6 => instant_seconds_until(start_s, start_n, end_s, end_n).map(|s| s / 43_200),
                7 => instant_seconds_until(start_s, start_n, end_s, end_n).map(|s| s / 86_400),
                _ => return Err(unsupported_unit(vm)),
            };
            long(result.ok_or_else(|| arithmetic_overflow(vm))?)
        },
    ),
    method(
        "until",
        "(Ljava/time/Instant;)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let end = ref_arg(args, 1);
            if end.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let secs = read_long_field(vm, end, "epochSecond")
                .checked_sub(read_long_field(vm, this, "epochSecond"))
                .ok_or_else(|| arithmetic_overflow(vm))?;
            let nanos = i64::from(read_int_field(vm, end, "nano"))
                - i64::from(read_int_field(vm, this, "nano"));
            duration_of_seconds(vm, secs, nanos)
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
            let seconds = read_long_field(vm, this, "epochSecond");
            let nano = read_int_field(vm, this, "nano");
            zoned_date_time_new(vm, seconds * 1000 + i64::from(nano) / 1_000_000, nano, zone)
        },
    ),
    method(
        "query",
        "(Ljava/time/temporal/TemporalQuery;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let query = ref_arg(args, 1);
            if query.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            crate::temporal::temporal_query_from(vm, query, receiver(args))
        },
    ),
    method(
        "truncatedTo",
        "(Ljava/time/temporal/TemporalUnit;)Ljava/time/Instant;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let unit = ref_arg(args, 1);
            if unit.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let nanos = crate::temporal::chrono_unit_nanos(vm, unit);
            let day_nanos = 86_400_000_000_000i64;
            if nanos <= 0 || nanos > day_nanos || day_nanos % nanos != 0 {
                return Err(vm.throw_new(
                    "java/time/temporal/UnsupportedTemporalTypeException",
                    Some("Unit is too large to be used for truncation"),
                ));
            }
            let total = read_long_field(vm, this, "epochSecond") * 1_000_000_000
                + i64::from(read_int_field(vm, this, "nano"));
            let truncated = total.div_euclid(nanos) * nanos;
            instant_new(
                vm,
                truncated.div_euclid(1_000_000_000),
                truncated.rem_euclid(1_000_000_000) as i32,
            )
        },
    ),
    method(
        "atOffset",
        "(Ljava/time/ZoneOffset;)Ljava/time/OffsetDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let offset = ref_arg(args, 1);
            if offset.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let offset_seconds = zone_offset_seconds(vm, offset);
            let epoch_second = read_long_field(vm, this, "epochSecond");
            let nano = read_int_field(vm, this, "nano");
            let local_second = epoch_second.saturating_add(i64::from(offset_seconds));
            let epoch_day = local_second.div_euclid(86_400);
            let secs_of_day = local_second.rem_euclid(86_400);
            let nano_of_day = secs_of_day * 1_000_000_000 + i64::from(nano);
            object(offset_date_time_new(vm, epoch_day, nano_of_day, offset)?)
        },
    ),
];

pub(crate) fn instant_new(vm: &mut Vm<'_>, epoch_second: i64, nano: i32) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/time/Instant")?;
    let instance = vm.new_instance(class)?;
    write_long_field(vm, instance, "epochSecond", epoch_second);
    write_int_field(vm, instance, "nano", nano);
    object(instance)
}

/// `Instant.plus(amount)` / `Instant.minus(amount)` for a `TemporalAmount`.
///
/// A `Duration` contributes its whole nanosecond count; a `Period` contributes
/// only its days (years and months are unsupported on the time-line, matching
/// the JDK's `UnsupportedTemporalTypeException`). Anything else is rejected.
fn instant_shift_amount(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    amount: ObjectRef,
    subtract: bool,
) -> Result<Value, VmError> {
    let duration_class = vm.resolve_class("java/time/Duration")?;
    let period_class = vm.resolve_class("java/time/Period")?;
    let (delta_secs, delta_nanos) = if vm.is_instance(amount, duration_class) {
        let mut nanos = read_long_field(vm, amount, "nanos");
        if subtract {
            nanos = -nanos;
        }
        (
            nanos.div_euclid(1_000_000_000),
            nanos.rem_euclid(1_000_000_000),
        )
    } else if vm.is_instance(amount, period_class) {
        if read_int_field(vm, amount, "years") != 0 || read_int_field(vm, amount, "months") != 0 {
            return Err(unsupported_unit(vm));
        }
        let mut secs = i64::from(read_int_field(vm, amount, "days")).saturating_mul(86_400);
        if subtract {
            secs = -secs;
        }
        (secs, 0)
    } else {
        return Err(unsupported_unit(vm));
    };
    let total_nanos = i64::from(read_int_field(vm, this, "nano")) + delta_nanos;
    let epoch_second = read_long_field(vm, this, "epochSecond")
        .saturating_add(delta_secs)
        .saturating_add(total_nanos.div_euclid(1_000_000_000));
    instant_new(
        vm,
        epoch_second,
        total_nanos.rem_euclid(1_000_000_000) as i32,
    )
}

/// Parse an ISO-8601 instant such as `2020-01-15T10:30:45Z` (with an optional
/// fractional-second part). Anything else throws `DateTimeParseException`.
fn instant_parse(vm: &mut Vm<'_>, text: &str) -> Result<Value, VmError> {
    let (date_part, time_part) = text
        .split_once('T')
        .ok_or_else(|| instant_parse_error(vm, text))?;
    let date_fields: Vec<&str> = date_part.split('-').collect();
    if date_fields.len() != 3 {
        return Err(instant_parse_error(vm, text));
    }
    let year: i32 = date_fields[0]
        .parse()
        .map_err(|_| instant_parse_error(vm, text))?;
    let month: i32 = date_fields[1]
        .parse()
        .map_err(|_| instant_parse_error(vm, text))?;
    let day: i32 = date_fields[2]
        .parse()
        .map_err(|_| instant_parse_error(vm, text))?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(instant_parse_error(vm, text));
    }
    // The time must end in `Z` (UTC) or a numeric offset (`+HH`, `+HHMM`,
    // `+HHMMSS`, or the colon-separated `+HH:MM[:SS]` forms).
    let (clock, offset_seconds) = if let Some(stripped) = time_part.strip_suffix('Z') {
        (stripped, 0i64)
    } else if let Some(pos) = time_part.rfind(['+', '-']) {
        let offset_text = &time_part[pos + 1..];
        let sign = if time_part.as_bytes()[pos] == b'-' {
            -1
        } else {
            1
        };
        let two_digits = |field: &str| -> Option<i64> {
            if field.len() == 2 && field.bytes().all(|b| b.is_ascii_digit()) {
                field.parse().ok()
            } else {
                None
            }
        };
        let total = if offset_text.contains(':') {
            let fields: Vec<&str> = offset_text.split(':').collect();
            if fields.len() != 2 && fields.len() != 3 {
                return Err(instant_parse_error(vm, text));
            }
            let oh = two_digits(fields[0]).ok_or_else(|| instant_parse_error(vm, text))?;
            let om = two_digits(fields[1]).ok_or_else(|| instant_parse_error(vm, text))?;
            let os = if fields.len() == 3 {
                two_digits(fields[2]).ok_or_else(|| instant_parse_error(vm, text))?
            } else {
                0
            };
            oh * 3600 + om * 60 + os
        } else {
            let len = offset_text.len();
            if (len != 2 && len != 4 && len != 6)
                || !offset_text.bytes().all(|b| b.is_ascii_digit())
            {
                return Err(instant_parse_error(vm, text));
            }
            let oh: i64 = offset_text[..2]
                .parse()
                .map_err(|_| instant_parse_error(vm, text))?;
            let om: i64 = if len >= 4 {
                offset_text[2..4]
                    .parse()
                    .map_err(|_| instant_parse_error(vm, text))?
            } else {
                0
            };
            let os: i64 = if len == 6 {
                offset_text[4..6]
                    .parse()
                    .map_err(|_| instant_parse_error(vm, text))?
            } else {
                0
            };
            oh * 3600 + om * 60 + os
        };
        if total > 18 * 3600 {
            return Err(instant_parse_error(vm, text));
        }
        (&time_part[..pos], sign * total)
    } else {
        return Err(instant_parse_error(vm, text));
    };
    // Separate the fractional seconds, if any.
    let (hms, nano) = match clock.split_once('.') {
        Some((base, fraction)) => {
            let mut digits = fraction.to_string();
            if digits.is_empty() || digits.len() > 9 || !digits.chars().all(|c| c.is_ascii_digit())
            {
                return Err(instant_parse_error(vm, text));
            }
            while digits.len() < 9 {
                digits.push('0');
            }
            (
                base,
                digits
                    .parse::<i32>()
                    .map_err(|_| instant_parse_error(vm, text))?,
            )
        }
        None => (clock, 0),
    };
    let time_fields: Vec<&str> = hms.split(':').collect();
    if time_fields.len() != 3 {
        return Err(instant_parse_error(vm, text));
    }
    let hour: i64 = time_fields[0]
        .parse()
        .map_err(|_| instant_parse_error(vm, text))?;
    let minute: i64 = time_fields[1]
        .parse()
        .map_err(|_| instant_parse_error(vm, text))?;
    let second: i64 = time_fields[2]
        .parse()
        .map_err(|_| instant_parse_error(vm, text))?;
    if hour > 23 || minute > 59 || second > 59 {
        return Err(instant_parse_error(vm, text));
    }
    let epoch_day = days_from_civil(year, month, day);
    let epoch_second = epoch_day * 86_400 + hour * 3600 + minute * 60 + second - offset_seconds;
    instant_new(vm, epoch_second, nano)
}

fn instant_parse_error(vm: &mut Vm<'_>, text: &str) -> VmError {
    vm.throw_new("java/time/format/DateTimeParseException", Some(text))
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
        field("epochSecond", "J", ACC_PRIVATE | ACC_FINAL, None),
        field("nano", "I", ACC_PRIVATE | ACC_FINAL, None),
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

/// Local epoch-milliseconds of a temporal accessor, for pattern formatting.
fn temporal_local_millis(vm: &mut Vm<'_>, obj: ObjectRef) -> Result<i64, VmError> {
    let name = vm.class_name(vm.class_of(obj)).to_string();
    match name.as_str() {
        "java/time/LocalDate" => Ok(read_long_field(vm, obj, "epochDay") * 86_400_000),
        "java/time/LocalTime" => Ok(read_long_field(vm, obj, "nanoOfDay") / 1_000_000),
        "java/time/LocalDateTime" | "java/time/OffsetDateTime" => {
            Ok(read_long_field(vm, obj, "epochDay") * 86_400_000
                + read_long_field(vm, obj, "nanoOfDay") / 1_000_000)
        }
        "java/time/OffsetTime" => Ok(read_long_field(vm, obj, "nanoOfDay") / 1_000_000),
        "java/time/ZonedDateTime" => {
            let (day, nano) = zdt_local_parts(vm, obj);
            Ok(day * 86_400_000 + nano / 1_000_000)
        }
        "java/time/Instant" => Ok(read_long_field(vm, obj, "epochSecond") * 1000
            + i64::from(read_int_field(vm, obj, "nano")) / 1_000_000),
        _ => Err(vm.throw_new(
            "java/time/DateTimeException",
            Some("Unsupported temporal type for formatting"),
        )),
    }
}

const DATE_TIME_FORMATTER_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
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
    method(
        "format",
        "(Ljava/time/temporal/TemporalAccessor;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let temporal = ref_arg(args, 1);
            if temporal.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let pattern = vm
                .read_string_field(this, vm.class_of(this), "pattern")
                .unwrap_or_default();
            let millis = temporal_local_millis(vm, temporal)?;
            object(vm.make_string(&crate::text::format_pattern(&pattern, millis))?)
        },
    ),
    method(
        "ofLocalizedDate",
        "(Ljava/time/format/FormatStyle;)Ljava/time/format/DateTimeFormatter;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let style = ref_arg(args, 0);
            if style.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let ordinal = read_int_field(vm, style, "ordinal");
            date_time_formatter_with_pattern(vm, localized_date_pattern(ordinal))
        },
    ),
    method(
        "ofLocalizedTime",
        "(Ljava/time/format/FormatStyle;)Ljava/time/format/DateTimeFormatter;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let style = ref_arg(args, 0);
            if style.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let ordinal = read_int_field(vm, style, "ordinal");
            date_time_formatter_with_pattern(vm, localized_time_pattern(ordinal))
        },
    ),
    method(
        "ofLocalizedDateTime",
        "(Ljava/time/format/FormatStyle;)Ljava/time/format/DateTimeFormatter;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let style = ref_arg(args, 0);
            if style.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let ordinal = read_int_field(vm, style, "ordinal");
            let pattern = alloc::format!(
                "{} {}",
                localized_date_pattern(ordinal),
                localized_time_pattern(ordinal)
            );
            date_time_formatter_with_pattern(vm, &pattern)
        },
    ),
    method(
        "ofLocalizedDateTime",
        "(Ljava/time/format/FormatStyle;Ljava/time/format/FormatStyle;)Ljava/time/format/DateTimeFormatter;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let date_style = ref_arg(args, 0);
            let time_style = ref_arg(args, 1);
            if date_style.is_null() || time_style.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let date_ordinal = read_int_field(vm, date_style, "ordinal");
            let time_ordinal = read_int_field(vm, time_style, "ordinal");
            let pattern = alloc::format!(
                "{} {}",
                localized_date_pattern(date_ordinal),
                localized_time_pattern(time_ordinal)
            );
            date_time_formatter_with_pattern(vm, &pattern)
        },
    ),
    method(
        "withLocale",
        "(Ljava/util/Locale;)Ljava/time/format/DateTimeFormatter;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let pattern = vm
                .read_string_field(this, vm.class_of(this), "pattern")
                .unwrap_or_default();
            date_time_formatter_with_pattern(vm, &pattern)
        },
    ),
    method(
        "parse",
        "(Ljava/lang/CharSequence;Ljava/time/temporal/TemporalQuery;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = crate::string_arg(vm, args[1])?;
            let query = ref_arg(args, 2);
            if query.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let pattern = vm
                .read_string_field(this, vm.class_of(this), "pattern")
                .unwrap_or_default();
            let (date_text, time_text) = match text.split_once(['T', ' ']) {
                Some((d, t)) => (d, Some(t)),
                None => (text.as_str(), None),
            };
            let parts: Vec<&str> = date_text.split(['-', '/', '.']).collect();
            if parts.len() != 3 {
                return Err(vm.throw_new("java/time/format/DateTimeParseException", Some(&text)));
            }
            let (y, m, d) = if pattern.starts_with("dd") {
                (parts[2], parts[1], parts[0])
            } else {
                (parts[0], parts[1], parts[2])
            };
            let parsed = (
                y.parse::<i32>().ok(),
                m.parse::<i32>().ok(),
                d.parse::<i32>().ok(),
            );
            let (year, month, day) = match parsed {
                (Some(y), Some(m), Some(d)) => (y, m, d),
                _ => {
                    return Err(
                        vm.throw_new("java/time/format/DateTimeParseException", Some(&text))
                    );
                }
            };
            let epoch_day = days_from_civil(year, month, day);
            let parsed = if let Some(t) = time_text {
                let tp: Vec<&str> = t.split(':').collect();
                let hour = tp.first().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
                let minute = tp.get(1).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
                let second = tp.get(2).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
                let nano_of_day = ((hour * 60 + minute) * 60 + second) * 1_000_000_000;
                Value::Ref(local_date_time_new(vm, epoch_day, nano_of_day)?)
            } else {
                Value::Ref(local_date_new(vm, epoch_day)?)
            };
            crate::temporal::temporal_query_from(vm, query, parsed.as_ref())
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

fn localized_date_pattern(ordinal: i32) -> &'static str {
    match ordinal {
        0 => "EEEE, MMMM d, yyyy",
        1 => "MMMM d, yyyy",
        2 => "MMM d, yyyy",
        _ => "M/d/yy",
    }
}

fn localized_time_pattern(ordinal: i32) -> &'static str {
    match ordinal {
        0 => "h:mm:ss\u{202F}a zzzz",
        1 => "h:mm:ss\u{202F}a z",
        2 => "h:mm:ss\u{202F}a",
        _ => "h:mm\u{202F}a",
    }
}

fn date_time_formatter_with_pattern(vm: &mut Vm<'_>, pattern: &str) -> Result<Value, VmError> {
    let string = vm.make_string(pattern)?;
    date_time_formatter_new(vm, string)
}

/// The ISO pattern constants, initialized once.
fn date_time_formatter_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/time/format/DateTimeFormatter")?;
    const ISO: [(&str, &str); 9] = [
        ("ISO_LOCAL_DATE", "yyyy-MM-dd"),
        ("ISO_DATE", "yyyy-MM-dd"),
        ("ISO_LOCAL_TIME", "HH:mm:ss"),
        ("ISO_TIME", "HH:mm:ss"),
        ("ISO_LOCAL_DATE_TIME", "yyyy-MM-dd'T'HH:mm:ss"),
        ("ISO_DATE_TIME", "yyyy-MM-dd'T'HH:mm:ss"),
        ("BASIC_ISO_DATE", "yyyyMMdd"),
        ("ISO_INSTANT", "yyyy-MM-dd'T'HH:mm:ss'Z'"),
        ("ISO_OFFSET_DATE_TIME", "yyyy-MM-dd'T'HH:mm:ss"),
    ];
    for (name, pattern) in ISO {
        let string = vm.make_string(pattern)?;
        let instance = vm.new_instance(class)?;
        if let Some((declaring, field)) = vm.find_field_opt(class, "pattern", "Ljava/lang/String;")
        {
            vm.set_instance_ref(instance, declaring, field, string);
        }
        vm.set_static_value(
            class,
            name,
            "Ljava/time/format/DateTimeFormatter;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

/// `java.time.format.DateTimeFormatter`, carrying only the pattern.
pub(crate) const DATE_TIME_FORMATTER: oxjvm_vm::NativeClass = class(
    "java/time/format/DateTimeFormatter",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("pattern", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "ISO_LOCAL_DATE",
            "Ljava/time/format/DateTimeFormatter;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ISO_DATE",
            "Ljava/time/format/DateTimeFormatter;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ISO_LOCAL_TIME",
            "Ljava/time/format/DateTimeFormatter;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ISO_TIME",
            "Ljava/time/format/DateTimeFormatter;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ISO_LOCAL_DATE_TIME",
            "Ljava/time/format/DateTimeFormatter;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ISO_DATE_TIME",
            "Ljava/time/format/DateTimeFormatter;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "BASIC_ISO_DATE",
            "Ljava/time/format/DateTimeFormatter;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ISO_INSTANT",
            "Ljava/time/format/DateTimeFormatter;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ISO_OFFSET_DATE_TIME",
            "Ljava/time/format/DateTimeFormatter;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &DATE_TIME_FORMATTER_METHODS,
    Some(date_time_formatter_clinit),
);

/// The local epoch-day and nano-of-day of a `ZonedDateTime` in its own zone.
/// JDK `LocalDate.hashCode`: `(year & 0xFFFFF800) ^ ((year << 11) + (month << 6) + day)`.
fn local_date_hash(epoch_day: i64) -> i32 {
    let (year, month, day) = civil_from_days(epoch_day);
    (year & -2048) ^ (year << 11).wrapping_add(month << 6).wrapping_add(day)
}

/// JDK `LocalTime.hashCode`: `(int)(nano ^ (nano >>> 32))`.
fn local_time_hash(nano_of_day: i64) -> i32 {
    let nano = nano_of_day as u64;
    (nano ^ (nano >> 32)) as i32
}

/// JDK `Instant.hashCode`: `(int)(seconds ^ (seconds >>> 32)) + 51 * nanos`.
fn instant_hash(seconds: i64, nano: i32) -> i32 {
    let secs = seconds as u64;
    ((secs ^ (secs >> 32)) as i32).wrapping_add(51i32.wrapping_mul(nano))
}

fn zdt_local_parts(vm: &Vm<'_>, this: ObjectRef) -> (i64, i64) {
    let millis = read_long_field(vm, this, "millis");
    let nano = i64::from(read_int_field(vm, this, "nano"));
    let zone = vm
        .read_ref_field(this, vm.class_of(this), "zone", "Ljava/time/ZoneId;")
        .unwrap_or(ObjectRef::NULL);
    let offset = i64::from(zone_offset_seconds(vm, zone));
    let local_millis = millis + offset * 1000;
    let epoch_second = local_millis.div_euclid(1000);
    let epoch_day = epoch_second.div_euclid(86_400);
    let nano_of_day = epoch_second.rem_euclid(86_400) * 1_000_000_000 + nano;
    (epoch_day, nano_of_day)
}

const ZONED_DATE_TIME_METHODS: [oxjvm_vm::NativeMethodDef; 33] = [
    method(
        "now",
        "()Ljava/time/ZonedDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let millis = vm.host_time_millis();
            let zone = system_default_zone(vm)?;
            zoned_date_time_new(
                vm,
                millis,
                (millis.rem_euclid(1000) * 1_000_000) as i32,
                zone,
            )
        },
    ),
    method(
        "now",
        "(Ljava/time/ZoneId;)Ljava/time/ZonedDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let millis = vm.host_time_millis();
            zoned_date_time_new(
                vm,
                millis,
                (millis.rem_euclid(1000) * 1_000_000) as i32,
                ref_arg(args, 0),
            )
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
            let nano = (nano_of_day % 1_000_000_000) as i32;
            zoned_date_time_new(
                vm,
                (local_second - i64::from(offset)) * 1000 + i64::from(nano) / 1_000_000,
                nano,
                zone,
            )
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
            let this = receiver(args);
            let millis = read_long_field(vm, this, "millis");
            instant_new(
                vm,
                millis.div_euclid(1000),
                read_int_field(vm, this, "nano"),
            )
        },
    ),
    method("toEpochSecond", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "millis").div_euclid(1000))
    }),
    method("getNano", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "nano"))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let millis = read_long_field(vm, this, "millis");
            let nano = read_int_field(vm, this, "nano");
            let zone = vm
                .read_ref_field(this, vm.class_of(this), "zone", "Ljava/time/ZoneId;")
                .unwrap_or(ObjectRef::NULL);
            let offset = zone_offset_seconds(vm, zone);
            let local_millis = millis + i64::from(offset) * 1000;
            let epoch_second = local_millis.div_euclid(1000);
            let epoch_day = epoch_second.div_euclid(86_400);
            let second_of_day = epoch_second.rem_euclid(86_400);
            let (year, month, day) = civil_from_days(epoch_day);
            let hour = second_of_day / 3600;
            let minute = (second_of_day % 3600) / 60;
            let second = second_of_day % 60;
            let mut text = alloc::format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}");
            if second != 0 || nano != 0 {
                text.push_str(&alloc::format!(":{second:02}"));
            }
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
            text.push_str(&crate::lang::zone_offset_id(offset));
            let is_offset = vm
                .find_field_opt(vm.class_of(zone), "totalSeconds", "I")
                .is_some();
            if !zone.is_null() && !is_offset {
                let id = vm
                    .read_string_field(zone, vm.class_of(zone), "id")
                    .unwrap_or_default();
                text.push('[');
                text.push_str(&id);
                text.push(']');
            }
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
                return boolean(false);
            }
            let class = vm.resolve_class("java/time/ZonedDateTime")?;
            if !vm.is_instance(other, class) {
                return boolean(false);
            }
            boolean(
                read_long_field(vm, receiver(args), "millis")
                    == read_long_field(vm, other, "millis"),
            )
        },
    ),
    method(
        "of",
        "(IIIIIIILjava/time/ZoneId;)Ljava/time/ZonedDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let year = int_arg(args, 0);
            let month = int_arg(args, 1);
            let day = int_arg(args, 2);
            let hour = int_arg(args, 3);
            let minute = int_arg(args, 4);
            let second = int_arg(args, 5);
            let nano = int_arg(args, 6);
            let zone = ref_arg(args, 7);
            if zone.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            if !(1..=12).contains(&month) || day < 1 || day > length_of_month(year, month) {
                return Err(vm.throw_new("java/time/DateTimeException", Some("Invalid date")));
            }
            if !(0..=23).contains(&hour)
                || !(0..=59).contains(&minute)
                || !(0..=59).contains(&second)
                || !(0..=999_999_999).contains(&nano)
            {
                return Err(vm.throw_new("java/time/DateTimeException", Some("Invalid time")));
            }
            let epoch_day = days_from_civil(year, month, day);
            let nano_of_day = (i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second))
                * 1_000_000_000
                + i64::from(nano);
            let local_second = epoch_day * 86_400 + nano_of_day / 1_000_000_000;
            let offset = zone_offset_seconds(vm, zone);
            zoned_date_time_new(
                vm,
                (local_second - i64::from(offset)) * 1000
                    + nano_of_day.rem_euclid(1_000_000_000) / 1_000_000,
                nano,
                zone,
            )
        },
    ),
    method(
        "getZone",
        "()Ljava/time/ZoneId;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let zone = vm
                .read_ref_field(this, vm.class_of(this), "zone", "Ljava/time/ZoneId;")
                .unwrap_or(ObjectRef::NULL);
            object(zone)
        },
    ),
    method(
        "until",
        "(Ljava/time/temporal/Temporal;Ljava/time/temporal/TemporalUnit;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let end = ref_arg(args, 1);
            if end.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let ordinal =
                chrono_unit_ordinal(vm, ref_arg(args, 2)).ok_or_else(|| unsupported_unit(vm))?;
            let class = vm.class_of(this);
            let zone = vm
                .read_ref_field(this, class, "zone", "Ljava/time/ZoneId;")
                .unwrap_or(ObjectRef::NULL);
            let offset = i64::from(zone_offset_seconds(vm, zone));
            // Both instants expressed as local millis in this zone.
            let start_millis = read_long_field(vm, this, "millis") + offset * 1000;
            let start_nano = i64::from(read_int_field(vm, this, "nano"));
            let end_millis = read_long_field(vm, end, "millis") + offset * 1000;
            let end_nano = i64::from(read_int_field(vm, end, "nano"));
            if let Some(unit_nanos) = time_unit_nanos(ordinal) {
                let amount = (end_millis - start_millis) * 1_000_000 + (end_nano - start_nano);
                return long(amount / unit_nanos);
            }
            let start_epoch = start_millis.div_euclid(1000).div_euclid(86_400);
            let start_day_nano =
                start_millis.div_euclid(1000).rem_euclid(86_400) * 1_000_000_000 + start_nano;
            let end_epoch = end_millis.div_euclid(1000).div_euclid(86_400);
            let end_day_nano =
                end_millis.div_euclid(1000).rem_euclid(86_400) * 1_000_000_000 + end_nano;
            let mut adjusted_end = end_epoch;
            if end_epoch > start_epoch && end_day_nano < start_day_nano {
                adjusted_end -= 1;
            } else if end_epoch < start_epoch && end_day_nano > start_day_nano {
                adjusted_end += 1;
            }
            long(
                date_until(start_epoch, adjusted_end, ordinal)
                    .ok_or_else(|| unsupported_unit(vm))?,
            )
        },
    ),
    method(
        "withNano",
        "(I)Ljava/time/ZonedDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let nano = int_arg(args, 1);
            if !(0..=999_999_999).contains(&nano) {
                return Err(vm.throw_new(
                    "java/time/DateTimeException",
                    Some("Invalid value for NanoOfSecond"),
                ));
            }
            let this = receiver(args);
            let old_nano = read_int_field(vm, this, "nano");
            let millis = read_long_field(vm, this, "millis") - i64::from(old_nano) / 1_000_000
                + i64::from(nano) / 1_000_000;
            let zone = vm
                .read_ref_field(this, vm.class_of(this), "zone", "Ljava/time/ZoneId;")
                .unwrap_or(ObjectRef::NULL);
            zoned_date_time_new(vm, millis, nano, zone)
        },
    ),
    method(
        "plusDays",
        "(J)Ljava/time/ZonedDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let days = crate::long_arg(args, 1);
            if days == 0 {
                return object(this);
            }
            let millis = read_long_field(vm, this, "millis") + days * 86_400_000;
            let nano = read_int_field(vm, this, "nano");
            let zone = vm
                .read_ref_field(this, vm.class_of(this), "zone", "Ljava/time/ZoneId;")
                .unwrap_or(ObjectRef::NULL);
            zoned_date_time_new(vm, millis, nano, zone)
        },
    ),
    method(
        "parse",
        "(Ljava/lang/CharSequence;)Ljava/time/ZonedDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = crate::string_arg(vm, args[0])?;
            let error = |vm: &mut Vm<'_>| {
                vm.throw_new("java/time/format/DateTimeParseException", Some(&text))
            };
            let (main, zone_text) = if let Some(open) = text.find('[') {
                if !text.ends_with(']') {
                    return Err(error(vm));
                }
                (&text[..open], Some(&text[open + 1..text.len() - 1]))
            } else {
                (text.as_str(), None)
            };
            let (date_text, time_text) = main.split_once('T').ok_or_else(|| error(vm))?;
            let parts: Vec<&str> = date_text.split('-').collect();
            if parts.len() != 3 {
                return Err(error(vm));
            }
            let year: i32 = parts[0].parse().map_err(|_| error(vm))?;
            let month: i32 = parts[1].parse().map_err(|_| error(vm))?;
            let day: i32 = parts[2].parse().map_err(|_| error(vm))?;
            let (clock_text, offset_total) = if let Some(stripped) = time_text.strip_suffix('Z') {
                (stripped, 0)
            } else {
                let sign_pos = time_text.rfind(['+', '-']).ok_or_else(|| error(vm))?;
                let total = crate::lang::zone_offset_parse(&time_text[sign_pos..])
                    .ok_or_else(|| error(vm))?;
                (&time_text[..sign_pos], total)
            };
            let clock: Vec<&str> = clock_text.split(':').collect();
            if clock.len() < 2 || clock.len() > 3 {
                return Err(error(vm));
            }
            let hour: i64 = clock[0].parse().map_err(|_| error(vm))?;
            let minute: i64 = clock[1].parse().map_err(|_| error(vm))?;
            let (second, nano) = if clock.len() == 3 {
                let mut split = clock[2].splitn(2, '.');
                let sec: i64 = split.next().unwrap_or("0").parse().map_err(|_| error(vm))?;
                let frac = split.next().unwrap_or("");
                let mut padded = String::from(frac);
                while padded.len() < 9 {
                    padded.push('0');
                }
                (sec, padded.parse::<i64>().unwrap_or(0))
            } else {
                (0, 0)
            };
            let epoch_day = days_from_civil(year, month, day);
            let nano_of_day = (hour * 3600 + minute * 60 + second) * 1_000_000_000 + nano;
            let local_second = epoch_day * 86_400 + nano_of_day / 1_000_000_000;
            let zone = match zone_text {
                Some(name) => {
                    let zone_class = vm.resolve_class("java/time/ZoneId")?;
                    let (declaring, of) = vm
                        .find_method(zone_class, "of", "(Ljava/lang/String;)Ljava/time/ZoneId;")
                        .ok_or_else(|| error(vm))?;
                    let string = vm.make_string(name)?;
                    vm.invoke_method(declaring, of, vec![Value::Ref(string)])?
                        .as_ref()
                }
                None => crate::lang::zone_offset_new(vm, offset_total)?,
            };
            let offset = zone_offset_seconds(vm, zone);
            zoned_date_time_new(
                vm,
                (local_second - i64::from(offset)) * 1000 + nano / 1_000_000,
                (nano_of_day % 1_000_000_000) as i32,
                zone,
            )
        },
    ),
    method("getYear", "()I", ACC_PUBLIC, |vm, _, args| {
        int(civil_from_days(zdt_local_parts(vm, receiver(args)).0).0)
    }),
    method("getMonthValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(civil_from_days(zdt_local_parts(vm, receiver(args)).0).1)
    }),
    method("getDayOfMonth", "()I", ACC_PUBLIC, |vm, _, args| {
        int(civil_from_days(zdt_local_parts(vm, receiver(args)).0).2)
    }),
    method("getHour", "()I", ACC_PUBLIC, |vm, _, args| {
        int((zdt_local_parts(vm, receiver(args)).1 / 3_600_000_000_000) as i32)
    }),
    method("getMinute", "()I", ACC_PUBLIC, |vm, _, args| {
        int(((zdt_local_parts(vm, receiver(args)).1 / 60_000_000_000) % 60) as i32)
    }),
    method("getSecond", "()I", ACC_PUBLIC, |vm, _, args| {
        int(((zdt_local_parts(vm, receiver(args)).1 / 1_000_000_000) % 60) as i32)
    }),
    method(
        "toLocalDate",
        "()Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| object(local_date_new(vm, zdt_local_parts(vm, receiver(args)).0)?),
    ),
    method(
        "toLocalTime",
        "()Ljava/time/LocalTime;",
        ACC_PUBLIC,
        |vm, _, args| object(local_time_new(vm, zdt_local_parts(vm, receiver(args)).1)?),
    ),
    method(
        "getOffset",
        "()Ljava/time/ZoneOffset;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let zone = vm
                .read_ref_field(this, vm.class_of(this), "zone", "Ljava/time/ZoneId;")
                .unwrap_or(ObjectRef::NULL);
            object(crate::lang::zone_offset_new(
                vm,
                zone_offset_seconds(vm, zone),
            )?)
        },
    ),
    method(
        "query",
        "(Ljava/time/temporal/TemporalQuery;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let query = ref_arg(args, 1);
            if query.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            crate::temporal::temporal_query_from(vm, query, receiver(args))
        },
    ),
    method(
        "withZoneSameInstant",
        "(Ljava/time/ZoneId;)Ljava/time/ZonedDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let zone = ref_arg(args, 1);
            if zone.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            zoned_date_time_new(
                vm,
                read_long_field(vm, this, "millis"),
                read_int_field(vm, this, "nano"),
                zone,
            )
        },
    ),
    method(
        "withZoneSameLocal",
        "(Ljava/time/ZoneId;)Ljava/time/ZonedDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let zone = ref_arg(args, 1);
            if zone.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let old_zone = vm
                .read_ref_field(this, vm.class_of(this), "zone", "Ljava/time/ZoneId;")
                .unwrap_or(ObjectRef::NULL);
            let shift =
                i64::from(zone_offset_seconds(vm, old_zone) - zone_offset_seconds(vm, zone));
            zoned_date_time_new(
                vm,
                read_long_field(vm, this, "millis") + shift * 1000,
                read_int_field(vm, this, "nano"),
                zone,
            )
        },
    ),
    method(
        "plusHours",
        "(J)Ljava/time/ZonedDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let zone = vm
                .read_ref_field(this, vm.class_of(this), "zone", "Ljava/time/ZoneId;")
                .unwrap_or(ObjectRef::NULL);
            zoned_date_time_new(
                vm,
                read_long_field(vm, this, "millis")
                    + crate::long_arg(args, 1).saturating_mul(3_600_000),
                read_int_field(vm, this, "nano"),
                zone,
            )
        },
    ),
    method(
        "toOffsetDateTime",
        "()Ljava/time/OffsetDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let zone = vm
                .read_ref_field(this, vm.class_of(this), "zone", "Ljava/time/ZoneId;")
                .unwrap_or(ObjectRef::NULL);
            let offset_seconds = zone_offset_seconds(vm, zone);
            let nano = read_int_field(vm, this, "nano");
            let local_millis =
                read_long_field(vm, this, "millis") + i64::from(offset_seconds) * 1000;
            let epoch_day = local_millis.div_euclid(86_400_000);
            let nano_of_day =
                local_millis.rem_euclid(86_400_000) * 1_000_000 + i64::from(nano % 1_000_000);
            let offset_class = vm.resolve_class("java/time/ZoneOffset")?;
            let (declaring, of_total) = vm
                .find_method(offset_class, "ofTotalSeconds", "(I)Ljava/time/ZoneOffset;")
                .ok_or_else(|| VmError::internal("ZoneOffset.ofTotalSeconds missing"))?;
            let offset = vm
                .invoke_method(declaring, of_total, vec![int(offset_seconds)?])?
                .as_ref();
            object(offset_date_time_new(vm, epoch_day, nano_of_day, offset)?)
        },
    ),
    method(
        "of",
        "(Ljava/time/LocalDate;Ljava/time/LocalTime;Ljava/time/ZoneId;)Ljava/time/ZonedDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let date = ref_arg(args, 0);
            let time = ref_arg(args, 1);
            let zone = ref_arg(args, 2);
            if date.is_null() || time.is_null() || zone.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let epoch_day = read_long_field(vm, date, "epochDay");
            let nano_of_day = read_long_field(vm, time, "nanoOfDay");
            let local_second = epoch_day * 86_400 + nano_of_day / 1_000_000_000;
            let offset = zone_offset_seconds(vm, zone);
            let nano = (nano_of_day % 1_000_000_000) as i32;
            zoned_date_time_new(
                vm,
                (local_second - i64::from(offset)) * 1000 + i64::from(nano) / 1_000_000,
                nano,
                zone,
            )
        },
    ),
    method(
        "ofInstant",
        "(Ljava/time/Instant;Ljava/time/ZoneId;)Ljava/time/ZonedDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let instant = ref_arg(args, 0);
            let zone = ref_arg(args, 1);
            if instant.is_null() || zone.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let epoch_second = read_long_field(vm, instant, "epochSecond");
            let nano = read_int_field(vm, instant, "nano");
            zoned_date_time_new(
                vm,
                epoch_second * 1000 + i64::from(nano) / 1_000_000,
                nano,
                zone,
            )
        },
    ),
    method(
        "toLocalDateTime",
        "()Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let millis = read_long_field(vm, this, "millis");
            let nano = read_int_field(vm, this, "nano");
            let zone = vm
                .read_ref_field(this, vm.class_of(this), "zone", "Ljava/time/ZoneId;")
                .unwrap_or(ObjectRef::NULL);
            let offset = zone_offset_seconds(vm, zone);
            let local_millis = millis + i64::from(offset) * 1000;
            let epoch_second = local_millis.div_euclid(1000);
            let epoch_day = epoch_second.div_euclid(86_400);
            let secs_of_day = epoch_second.rem_euclid(86_400);
            let nano_of_day = secs_of_day * 1_000_000_000 + i64::from(nano);
            object(local_date_time_new(vm, epoch_day, nano_of_day)?)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let (epoch_day, nano_of_day) = zdt_local_parts(vm, this);
        let zone = vm
            .read_ref_field(this, vm.class_of(this), "zone", "Ljava/time/ZoneId;")
            .unwrap_or(ObjectRef::NULL);
        let (class, method) = vm.resolve_virtual_method(zone, "hashCode", "()I")?;
        let zhash = vm
            .invoke_method(class, method, vec![Value::Ref(zone)])?
            .as_int();
        int(local_date_hash(epoch_day)
            ^ local_time_hash(nano_of_day)
            ^ zone_offset_seconds(vm, zone)
            ^ ((zhash as u32).rotate_left(3) as i32))
    }),
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
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
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

fn zoned_date_time_new(
    vm: &mut Vm<'_>,
    millis: i64,
    nano: i32,
    zone: ObjectRef,
) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/time/ZonedDateTime")?;
    let instance = vm.new_instance(class)?;
    write_long_field(vm, instance, "millis", millis);
    write_int_field(vm, instance, "nano", nano);
    if let Some((declaring, field)) = vm.find_field_opt(class, "zone", "Ljava/time/ZoneId;") {
        vm.set_instance_ref(instance, declaring, field, zone);
    }
    object(instance)
}

/// `java.time.ZonedDateTime`, stored as an epoch-millisecond count plus the
/// full nano-of-second so sub-millisecond precision survives `toInstant()`.
pub(crate) const ZONED_DATE_TIME: oxjvm_vm::NativeClass = class(
    "java/time/ZonedDateTime",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("millis", "J", ACC_PRIVATE, None),
        field("nano", "I", ACC_PRIVATE, None),
        field("zone", "Ljava/time/ZoneId;", ACC_PRIVATE, None),
    ],
    &ZONED_DATE_TIME_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// LocalDate / LocalTime / LocalDateTime (proleptic Gregorian calendar)
// -------------------------------------------------------------------------------------------

/// Convert a count of days since 1970-01-01 into (year, month, day) using Howard Hinnant's
/// civil-calendar algorithm.
pub(crate) fn civil_from_days(days: i64) -> (i32, i32, i32) {
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
pub(crate) fn days_from_civil(year: i32, month: i32, day: i32) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let m = i64::from(month);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub(crate) fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub(crate) fn length_of_month(year: i32, month: i32) -> i32 {
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

/// Initialize the `LocalDate.MIN` / `LocalDate.MAX` constants.
fn local_date_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/time/LocalDate")?;
    for (name, epoch_day) in [
        ("MIN", days_from_civil(-999_999_999, 1, 1)),
        ("MAX", days_from_civil(999_999_999, 12, 31)),
    ] {
        let instance = local_date_new(vm, epoch_day)?;
        vm.set_static_value(class, name, "Ljava/time/LocalDate;", Value::Ref(instance));
    }
    Ok(())
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
            return boolean(true);
        }
    }
    if time_based {
        let (class, method) = vm.resolve_virtual_method(unit, "isTimeBased", "()Z")?;
        let result = vm.invoke_method(class, method, vec![Value::Ref(unit)])?;
        if result.as_int() != 0 {
            return boolean(true);
        }
    }
    boolean(false)
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

fn arithmetic_overflow(vm: &mut Vm<'_>) -> VmError {
    vm.throw_new("java/lang/ArithmeticException", Some("long overflow"))
}

/// Whole seconds from `start` to `end`, borrowing across the nano boundary so
/// the result is truncated toward zero (as `Instant.secondsUntil` does).
fn instant_seconds_until(start_s: i64, start_n: i32, end_s: i64, end_n: i32) -> Option<i64> {
    let mut secs = end_s.checked_sub(start_s)?;
    let nanos = i64::from(end_n) - i64::from(start_n);
    if secs > 0 && nanos < 0 {
        secs -= 1;
    } else if secs < 0 && nanos > 0 {
        secs += 1;
    }
    Some(secs)
}

/// Milliseconds from `start` to `end`, avoiding overflow for extreme instants.
fn instant_millis_until(start_s: i64, start_n: i32, end_s: i64, end_n: i32) -> Option<i64> {
    let millis = end_s.checked_sub(start_s)?.checked_mul(1000)?;
    let nanos = i64::from(end_n) - i64::from(start_n);
    if millis > 0 && nanos < 0 {
        Some((millis - 1000) + (nanos + 1_000_000_000) / 1_000_000)
    } else if millis < 0 && nanos > 0 {
        Some((millis + 1000) + (nanos - 1_000_000_000) / 1_000_000)
    } else {
        millis.checked_add(nanos / 1_000_000)
    }
}

/// Microseconds from `start` to `end`, avoiding overflow for extreme instants.
fn instant_micros_until(start_s: i64, start_n: i32, end_s: i64, end_n: i32) -> Option<i64> {
    let micros = end_s.checked_sub(start_s)?.checked_mul(1_000_000)?;
    let nanos = i64::from(end_n) - i64::from(start_n);
    if micros > 0 && nanos < 0 {
        Some((micros - 1_000_000) + (nanos + 1_000_000_000) / 1_000)
    } else if micros < 0 && nanos > 0 {
        Some((micros + 1_000_000) + (nanos - 1_000_000_000) / 1_000)
    } else {
        micros.checked_add(nanos / 1_000)
    }
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

pub(crate) fn local_time_new(vm: &mut Vm<'_>, nano_of_day: i64) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/time/LocalTime")?;
    // Exact-hour times share the cached `HOURS` singletons, matching the JDK's
    // identity semantics for the common constants.
    if (0..NANOS_PER_DAY).contains(&nano_of_day) && nano_of_day % 3_600_000_000_000 == 0 {
        if let Some(hours) = vm.read_static_ref_if_set(class, "HOURS", "[Ljava/time/LocalTime;") {
            let hour = (nano_of_day / 3_600_000_000_000) as i32;
            if let Value::Ref(cached) = vm.array_get(hours, hour)? {
                return Ok(cached);
            }
        }
    }
    let instance = vm.new_instance(class)?;
    write_long_field(vm, instance, "nanoOfDay", nano_of_day);
    Ok(instance)
}

/// Initialize the `LocalTime` hour singletons and the named constants.
fn local_time_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/time/LocalTime")?;
    let hours = vm.allocate_object_array(class, 24)?;
    for hour in 0..24i64 {
        let instance = vm.new_instance(class)?;
        write_long_field(vm, instance, "nanoOfDay", hour * 3_600_000_000_000);
        vm.array_set_ref(hours, hour as usize, instance)?;
    }
    vm.set_static_value(class, "HOURS", "[Ljava/time/LocalTime;", Value::Ref(hours));
    let midnight = vm.array_get(hours, 0)?.as_ref();
    let noon = vm.array_get(hours, 12)?.as_ref();
    vm.set_static_value(
        class,
        "MIDNIGHT",
        "Ljava/time/LocalTime;",
        Value::Ref(midnight),
    );
    vm.set_static_value(class, "MIN", "Ljava/time/LocalTime;", Value::Ref(midnight));
    vm.set_static_value(class, "NOON", "Ljava/time/LocalTime;", Value::Ref(noon));
    let max = vm.new_instance(class)?;
    write_long_field(vm, max, "nanoOfDay", NANOS_PER_DAY - 1);
    vm.set_static_value(class, "MAX", "Ljava/time/LocalTime;", Value::Ref(max));
    Ok(())
}

fn local_date_time_new(
    vm: &mut Vm<'_>,
    epoch_day: i64,
    nano_of_day: i64,
) -> Result<ObjectRef, VmError> {
    let date = local_date_new(vm, epoch_day)?;
    let time = local_time_new(vm, nano_of_day)?;
    local_date_time_with_parts(vm, date, time)
}

/// Build a `LocalDateTime` from existing `LocalDate`/`LocalTime` instances. The
/// JDK stores both parts as fields, so `toLocalDate()`/`toLocalTime()` return the
/// very same objects across arithmetic that does not change the respective part.
fn local_date_time_with_parts(
    vm: &mut Vm<'_>,
    date: ObjectRef,
    time: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/time/LocalDateTime")?;
    let instance = vm.new_instance(class)?;
    write_long_field(
        vm,
        instance,
        "epochDay",
        read_long_field(vm, date, "epochDay"),
    );
    write_long_field(
        vm,
        instance,
        "nanoOfDay",
        read_long_field(vm, time, "nanoOfDay"),
    );
    if let Some((declaring, field)) = vm.find_field_opt(class, "date", "Ljava/time/LocalDate;") {
        vm.set_instance_ref(instance, declaring, field, date);
    }
    if let Some((declaring, field)) = vm.find_field_opt(class, "time", "Ljava/time/LocalTime;") {
        vm.set_instance_ref(instance, declaring, field, time);
    }
    Ok(instance)
}

/// The stored `LocalDate` part of a `LocalDateTime` (null when absent).
fn ldt_date_ref(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "date", "Ljava/time/LocalDate;")
        .unwrap_or(ObjectRef::NULL)
}

/// The stored `LocalTime` part of a `LocalDateTime` (null when absent).
fn ldt_time_ref(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "time", "Ljava/time/LocalTime;")
        .unwrap_or(ObjectRef::NULL)
}

/// Finish a `LocalDateTime` shift, preserving identity when nothing changes and
/// reusing whichever stored part (date or time) is unaffected.
fn ldt_finish(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    old_epoch: i64,
    old_nano: i64,
    new_epoch: i64,
    new_nano: i64,
) -> Result<Value, VmError> {
    if new_epoch == old_epoch && new_nano == old_nano {
        return object(this);
    }
    let date = if new_epoch == old_epoch {
        let stored = ldt_date_ref(vm, this);
        if stored.is_null() {
            local_date_new(vm, new_epoch)?
        } else {
            stored
        }
    } else {
        local_date_new(vm, new_epoch)?
    };
    let time = if new_nano == old_nano {
        let stored = ldt_time_ref(vm, this);
        if stored.is_null() {
            local_time_new(vm, new_nano)?
        } else {
            stored
        }
    } else {
        local_time_new(vm, new_nano)?
    };
    object(local_date_time_with_parts(vm, date, time)?)
}

/// Add a signed `ChronoUnit` amount to a `LocalDateTime`, preserving identity
/// for no-op arithmetic and the stored `LocalTime` when only the date moves.
fn ldt_shift_unit(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    amount: i64,
    unit: ObjectRef,
) -> Result<Value, VmError> {
    if amount == 0 {
        return object(this);
    }
    let epoch_day = read_long_field(vm, this, "epochDay");
    let nano = read_long_field(vm, this, "nanoOfDay");
    let ordinal = chrono_unit_ordinal(vm, unit).ok_or_else(|| unsupported_unit(vm))?;
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
    ldt_finish(vm, this, epoch_day, nano, new_epoch, new_nano)
}

/// Shift a `LocalDateTime` by a nanosecond delta, carrying whole days.
fn ldt_shift_nanos(vm: &mut Vm<'_>, this: ObjectRef, delta: i64) -> Result<Value, VmError> {
    if delta == 0 {
        return object(this);
    }
    let epoch_day = read_long_field(vm, this, "epochDay");
    let nano = read_long_field(vm, this, "nanoOfDay");
    let total = nano + delta;
    ldt_finish(
        vm,
        this,
        epoch_day,
        nano,
        epoch_day + total.div_euclid(NANOS_PER_DAY),
        total.rem_euclid(NANOS_PER_DAY),
    )
}

/// Shift a `LocalDateTime`'s date part by a date-based `ChronoUnit` amount.
fn ldt_shift_date(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    amount: i64,
    ordinal: i32,
) -> Result<Value, VmError> {
    if amount == 0 {
        return object(this);
    }
    let epoch_day = read_long_field(vm, this, "epochDay");
    let nano = read_long_field(vm, this, "nanoOfDay");
    let new_epoch = date_plus(epoch_day, amount, ordinal).ok_or_else(|| unsupported_unit(vm))?;
    ldt_finish(vm, this, epoch_day, nano, new_epoch, nano)
}

/// Nanoseconds per base unit of a time-based `ChronoField` (ordinals 0..=14).
fn time_field_unit_nanos(ordinal: i32) -> Option<i64> {
    Some(match ordinal {
        0 | 1 => 1,
        2 | 3 => 1_000,
        4 | 5 => 1_000_000,
        6 | 7 => 1_000_000_000,
        8 | 9 => 60_000_000_000,
        10..=13 => 3_600_000_000_000,
        14 => 43_200_000_000_000,
        _ => return None,
    })
}

/// `LocalDateTime.with(field, value)`: dispatch to the time or date part.
fn ldt_with_field(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    field: ObjectRef,
    value: i64,
) -> Result<Value, VmError> {
    let ordinal = chrono_field_ordinal(vm, field).ok_or_else(|| unsupported_field(vm))?;
    if ordinal <= 14 {
        ldt_with_time_field(vm, this, ordinal, value)
    } else {
        ldt_with_date_field(vm, this, ordinal, value)
    }
}

/// Replace a time-based field of a `LocalDateTime` with a new value.
fn ldt_with_time_field(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    ordinal: i32,
    value: i64,
) -> Result<Value, VmError> {
    let nano = read_long_field(vm, this, "nanoOfDay");
    let current = time_field_long(nano, ordinal).ok_or_else(|| unsupported_field(vm))?;
    if value == current {
        return object(this);
    }
    let unit = time_field_unit_nanos(ordinal).ok_or_else(|| unsupported_field(vm))?;
    ldt_shift_nanos(vm, this, (value - current) * unit)
}

/// Replace a date-based field of a `LocalDateTime` with a new value.
/// `LocalDate.plus(amount, unit)`: ChronoUnit arithmetic, or delegate to the
/// unit's `addTo` for non-Chrono units (e.g. `IsoFields.QUARTER_YEARS`).
fn local_date_plus_unit(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    amount: i64,
    unit: ObjectRef,
) -> Result<Value, VmError> {
    if amount == 0 {
        return object(this);
    }
    if let Some(ordinal) = chrono_unit_ordinal(vm, unit) {
        let epoch_day = read_long_field(vm, this, "epochDay");
        let new_epoch =
            date_plus(epoch_day, amount, ordinal).ok_or_else(|| unsupported_unit(vm))?;
        if new_epoch == epoch_day {
            return object(this);
        }
        object(local_date_new(vm, new_epoch)?)
    } else {
        let (class, method) = vm.resolve_virtual_method(
            unit,
            "addTo",
            "(Ljava/time/temporal/Temporal;J)Ljava/time/temporal/Temporal;",
        )?;
        vm.invoke_method(
            class,
            method,
            vec![Value::Ref(unit), Value::Ref(this), long(amount)?],
        )
    }
}

/// `LocalDate.minus(amount, unit)`, mirroring [`local_date_plus_unit`].
fn local_date_minus_unit(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    amount: i64,
    unit: ObjectRef,
) -> Result<Value, VmError> {
    if amount == 0 {
        return object(this);
    }
    if let Some(ordinal) = chrono_unit_ordinal(vm, unit) {
        let epoch_day = read_long_field(vm, this, "epochDay");
        let new_epoch =
            date_plus(epoch_day, -amount, ordinal).ok_or_else(|| unsupported_unit(vm))?;
        if new_epoch == epoch_day {
            return object(this);
        }
        object(local_date_new(vm, new_epoch)?)
    } else {
        let (class, method) = vm.resolve_virtual_method(
            unit,
            "subtractFrom",
            "(Ljava/time/temporal/Temporal;J)Ljava/time/temporal/Temporal;",
        )?;
        vm.invoke_method(
            class,
            method,
            vec![Value::Ref(unit), Value::Ref(this), long(amount)?],
        )
    }
}

/// `LocalDate.with(field, value)`: replace a date field, returning the receiver
/// unchanged when the value does not change the date.
fn local_date_with_field(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    field: ObjectRef,
    value: i64,
) -> Result<Value, VmError> {
    let epoch_day = read_long_field(vm, this, "epochDay");
    let ordinal = chrono_field_ordinal(vm, field).ok_or_else(|| unsupported_field(vm))?;
    let new_epoch =
        date_with_field(epoch_day, ordinal, value).ok_or_else(|| unsupported_field(vm))?;
    if new_epoch == epoch_day {
        return object(this);
    }
    object(local_date_new(vm, new_epoch)?)
}

/// Replace a date-based `ChronoField` of an epoch day with a new value.
fn date_with_field(epoch_day: i64, ordinal: i32, value: i64) -> Option<i64> {
    let (year, month, day) = civil_from_days(epoch_day);
    Some(match ordinal {
        15 => epoch_day + (value - ((epoch_day + 3).rem_euclid(7) + 1)),
        18 => days_from_civil(year, month, value as i32),
        19 => days_from_civil(year, 1, 1) + value - 1,
        20 => value,
        23 => {
            let new_month = value as i32;
            let new_day = day.min(length_of_month(year, new_month));
            days_from_civil(year, new_month, new_day)
        }
        24 => {
            let new_year = value.div_euclid(12) as i32;
            let new_month = (value.rem_euclid(12) + 1) as i32;
            let new_day = day.min(length_of_month(new_year, new_month));
            days_from_civil(new_year, new_month, new_day)
        }
        26 => {
            let new_year = value as i32;
            let new_day = day.min(length_of_month(new_year, month));
            days_from_civil(new_year, month, new_day)
        }
        _ => return None,
    })
}

fn ldt_with_date_field(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    ordinal: i32,
    value: i64,
) -> Result<Value, VmError> {
    let epoch_day = read_long_field(vm, this, "epochDay");
    let nano = read_long_field(vm, this, "nanoOfDay");
    let (year, month, day) = civil_from_days(epoch_day);
    let new_epoch = match ordinal {
        18 => days_from_civil(year, month, value as i32),
        19 => days_from_civil(year, 1, 1) + value - 1,
        23 => {
            let new_month = value as i32;
            let new_day = day.min(length_of_month(year, new_month));
            days_from_civil(year, new_month, new_day)
        }
        26 => {
            let new_year = value as i32;
            let new_day = day.min(length_of_month(new_year, month));
            days_from_civil(new_year, month, new_day)
        }
        _ => return Err(unsupported_field(vm)),
    };
    ldt_finish(vm, this, epoch_day, nano, new_epoch, nano)
}

fn local_date_string(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let (year, month, day) = civil_from_days(read_long_field(vm, this, "epochDay"));
    let text = if year > 9999 {
        alloc::format!("+{year}-{month:02}-{day:02}")
    } else {
        alloc::format!("{year:04}-{month:02}-{day:02}")
    };
    object(vm.make_string(&text)?)
}

/// The fractional-second suffix for a nano-of-second, as the JDK writes it
/// (3, 6, or 9 digits; empty when zero).
fn nano_fraction(nano_of_second: i64) -> String {
    if nano_of_second == 0 {
        return String::new();
    }
    if nano_of_second % 1_000_000 == 0 {
        alloc::format!(".{:03}", nano_of_second / 1_000_000)
    } else if nano_of_second % 1_000 == 0 {
        alloc::format!(".{:06}", nano_of_second / 1_000)
    } else {
        alloc::format!(".{:09}", nano_of_second)
    }
}

fn local_time_string(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let nano = read_long_field(vm, this, "nanoOfDay");
    let hour = nano / 3_600_000_000_000;
    let minute = (nano / 60_000_000_000) % 60;
    let second = (nano / 1_000_000_000) % 60;
    let nanos = nano % 1_000_000_000;
    let text = if second == 0 && nanos == 0 {
        alloc::format!("{hour:02}:{minute:02}")
    } else {
        alloc::format!("{hour:02}:{minute:02}:{second:02}{}", nano_fraction(nanos))
    };
    object(vm.make_string(&text)?)
}

/// `LocalDate.datesUntil`: an ordered stream of dates from this date (inclusive) to `end`
/// (exclusive), stepping by one day or by the given positive `Period`.
fn local_date_dates_until(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    end: ObjectRef,
    step: Option<ObjectRef>,
) -> Result<Value, VmError> {
    if end.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let end_day = read_long_field(vm, end, "epochDay");
    if end_day < read_long_field(vm, this, "epochDay") {
        let (class, method) = vm.resolve_virtual_method(end, "toString", "()Ljava/lang/String;")?;
        let end_text = vm.invoke_method(class, method, vec![Value::Ref(end)])?;
        let (class, method) =
            vm.resolve_virtual_method(this, "toString", "()Ljava/lang/String;")?;
        let this_text = vm.invoke_method(class, method, vec![Value::Ref(this)])?;
        let message = alloc::format!(
            "{} < {}",
            vm.string_or_empty(end_text.as_ref()),
            vm.string_or_empty(this_text.as_ref())
        );
        return Err(vm.throw_new("java/lang/IllegalArgumentException", Some(&message)));
    }
    let mut elements = Vec::new();
    match step {
        None => {
            let mut day = read_long_field(vm, this, "epochDay");
            while day < end_day {
                elements.push(local_date_new(vm, day)?);
                day += 1;
            }
        }
        Some(period) => {
            let (class, method) = vm.resolve_virtual_method(period, "isZero", "()Z")?;
            let zero = vm
                .invoke_method(class, method, vec![Value::Ref(period)])?
                .as_int()
                != 0;
            let (class, method) = vm.resolve_virtual_method(period, "isNegative", "()Z")?;
            let negative = vm
                .invoke_method(class, method, vec![Value::Ref(period)])?
                .as_int()
                != 0;
            if zero || negative {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("step must be positive"),
                ));
            }
            let years = i64::from(read_int_field(vm, period, "years"));
            let months = i64::from(read_int_field(vm, period, "months"));
            let days = i64::from(read_int_field(vm, period, "days"));
            let total_months = years * 12 + months;
            let date_class = vm.class_of(this);
            let (_, plus_months) = vm
                .find_method(date_class, "plusMonths", "(J)Ljava/time/LocalDate;")
                .ok_or_else(|| VmError::internal("LocalDate.plusMonths missing"))?;
            let (declaring, plus_days) = vm
                .find_method(date_class, "plusDays", "(J)Ljava/time/LocalDate;")
                .ok_or_else(|| VmError::internal("LocalDate.plusDays missing"))?;
            // Each element is `start.plus(step.multipliedBy(n))`, not repeated addition, so
            // end-of-month clamping does not drift (2020-01-30 + 2 months = 2020-03-30).
            let mut index = 0i64;
            loop {
                let months_to_add = total_months.checked_mul(index).ok_or_else(|| {
                    vm.throw_new("java/lang/ArithmeticException", Some("long overflow"))
                })?;
                let days_to_add = days.checked_mul(index).ok_or_else(|| {
                    vm.throw_new("java/lang/ArithmeticException", Some("long overflow"))
                })?;
                let candidate = vm
                    .invoke_method(
                        declaring,
                        plus_months,
                        vec![Value::Ref(this), long(months_to_add)?],
                    )?
                    .as_ref();
                let candidate = vm
                    .invoke_method(
                        declaring,
                        plus_days,
                        vec![Value::Ref(candidate), long(days_to_add)?],
                    )?
                    .as_ref();
                if read_long_field(vm, candidate, "epochDay") >= end_day {
                    break;
                }
                elements.push(candidate);
                index += 1;
            }
        }
    }
    object(crate::stream::stream_from(vm, elements)?)
}

const LOCAL_DATE_METHODS: [oxjvm_vm::NativeMethodDef; 59] = [
    method(
        "of",
        "(III)Ljava/time/LocalDate;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let year = int_arg(args, 0);
            let month = int_arg(args, 1);
            let day = int_arg(args, 2);
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
            let year = int_arg(args, 0);
            let month = read_int_field(vm, ref_arg(args, 1), "value");
            let day = int_arg(args, 2);
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
        |vm, _, args| {
            let epoch_day = crate::long_arg(args, 0);
            let min = days_from_civil(-999_999_999, 1, 1);
            let max = days_from_civil(999_999_999, 12, 31);
            if epoch_day < min || epoch_day > max {
                return Err(vm.throw_new("java/time/DateTimeException", Some("Invalid epoch day")));
            }
            object(local_date_new(vm, epoch_day)?)
        },
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
        "now",
        "(Ljava/time/Clock;)Ljava/time/LocalDate;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let (epoch_day, _) = clock_local_parts(vm, ref_arg(args, 0))?;
            object(local_date_new(vm, epoch_day)?)
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
        boolean(is_leap_year(year))
    }),
    method(
        "plusDays",
        "(J)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let amount = crate::long_arg(args, 1);
            if amount == 0 {
                return object(this);
            }
            object(local_date_new(
                vm,
                read_long_field(vm, this, "epochDay") + amount,
            )?)
        },
    ),
    method(
        "minusDays",
        "(J)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let amount = crate::long_arg(args, 1);
            if amount == 0 {
                return object(this);
            }
            object(local_date_new(
                vm,
                read_long_field(vm, this, "epochDay") - amount,
            )?)
        },
    ),
    method(
        "plusWeeks",
        "(J)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let amount = crate::long_arg(args, 1);
            if amount == 0 {
                return object(this);
            }
            object(local_date_new(
                vm,
                read_long_field(vm, this, "epochDay") + amount * 7,
            )?)
        },
    ),
    method(
        "plusMonths",
        "(J)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let amount = crate::long_arg(args, 1);
            if amount == 0 {
                return object(this);
            }
            let (year, month, day) = civil_from_days(read_long_field(vm, this, "epochDay"));
            let total = i64::from(year) * 12 + i64::from(month - 1) + amount;
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
            let this = receiver(args);
            let amount = crate::long_arg(args, 1);
            if amount == 0 {
                return object(this);
            }
            let (year, month, day) = civil_from_days(read_long_field(vm, this, "epochDay"));
            let new_year = (i64::from(year) + amount) as i32;
            let new_day = day.min(length_of_month(new_year, month));
            object(local_date_new(
                vm,
                days_from_civil(new_year, month, new_day),
            )?)
        },
    ),
    method(
        "minusWeeks",
        "(J)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let amount = crate::long_arg(args, 1);
            if amount == 0 {
                return object(this);
            }
            object(local_date_new(
                vm,
                read_long_field(vm, this, "epochDay") - amount * 7,
            )?)
        },
    ),
    method(
        "minusMonths",
        "(J)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let amount = crate::long_arg(args, 1);
            if amount == 0 {
                return object(this);
            }
            let (year, month, day) = civil_from_days(read_long_field(vm, this, "epochDay"));
            let total = i64::from(year) * 12 + i64::from(month - 1) - amount;
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
        "minusYears",
        "(J)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let amount = crate::long_arg(args, 1);
            if amount == 0 {
                return object(this);
            }
            let (year, month, day) = civil_from_days(read_long_field(vm, this, "epochDay"));
            let new_year = (i64::from(year) - amount) as i32;
            let new_day = day.min(length_of_month(new_year, month));
            object(local_date_new(
                vm,
                days_from_civil(new_year, month, new_day),
            )?)
        },
    ),
    method(
        "getMonth",
        "()Ljava/time/Month;",
        ACC_PUBLIC,
        |vm, _, args| {
            let epoch_day = read_long_field(vm, receiver(args), "epochDay");
            let (_, month, _) = civil_from_days(epoch_day);
            let class = vm.resolve_class("java/time/Month")?;
            let (declaring, method) = vm
                .find_method(class, "of", "(I)Ljava/time/Month;")
                .ok_or_else(|| VmError::internal("Month lacks of"))?;
            vm.invoke_method(declaring, method, vec![Value::Int(month)])
        },
    ),
    method(
        "with",
        "(Ljava/time/temporal/TemporalAdjuster;)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let adjuster = ref_arg(args, 1);
            let (class, method) = vm.resolve_virtual_method(
                adjuster,
                "adjustInto",
                "(Ljava/time/temporal/Temporal;)Ljava/time/temporal/Temporal;",
            )?;
            vm.invoke_method(class, method, vec![Value::Ref(adjuster), Value::Ref(this)])
        },
    ),
    method(
        "withDayOfMonth",
        "(I)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let epoch_day = read_long_field(vm, this, "epochDay");
            let (year, month, day) = civil_from_days(epoch_day);
            let new_day = int_arg(args, 1);
            if new_day == day {
                return object(this);
            }
            object(local_date_new(vm, days_from_civil(year, month, new_day))?)
        },
    ),
    method(
        "withYear",
        "(I)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let epoch_day = read_long_field(vm, this, "epochDay");
            let (_year, month, day) = civil_from_days(epoch_day);
            let new_year = int_arg(args, 1);
            let new_day = day.min(length_of_month(new_year, month));
            let new_epoch = days_from_civil(new_year, month, new_day);
            if new_epoch == epoch_day {
                return object(this);
            }
            object(local_date_new(vm, new_epoch)?)
        },
    ),
    method(
        "withMonth",
        "(I)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let epoch_day = read_long_field(vm, this, "epochDay");
            let (year, _month, day) = civil_from_days(epoch_day);
            let new_month = int_arg(args, 1);
            if !(1..=12).contains(&new_month) {
                return Err(vm.throw_new("java/time/DateTimeException", Some("Invalid month")));
            }
            let new_day = day.min(length_of_month(year, new_month));
            let new_epoch = days_from_civil(year, new_month, new_day);
            if new_epoch == epoch_day {
                return object(this);
            }
            object(local_date_new(vm, new_epoch)?)
        },
    ),
    method(
        "withDayOfYear",
        "(I)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let epoch_day = read_long_field(vm, this, "epochDay");
            let (year, _, _) = civil_from_days(epoch_day);
            let day_of_year = int_arg(args, 1);
            let new_epoch = days_from_civil(year, 1, 1) + i64::from(day_of_year) - 1;
            if new_epoch == epoch_day {
                return object(this);
            }
            object(local_date_new(vm, new_epoch)?)
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
        "atStartOfDay",
        "(Ljava/time/ZoneId;)Ljava/time/ZonedDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let zone = ref_arg(args, 1);
            if zone.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let epoch_day = read_long_field(vm, receiver(args), "epochDay");
            let offset = zone_offset_seconds(vm, zone);
            let local_second = epoch_day.saturating_mul(86_400);
            zoned_date_time_new(
                vm,
                (local_second - i64::from(offset)).saturating_mul(1000),
                0,
                zone,
            )
        },
    ),
    method(
        "atTime",
        "(II)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let epoch_day = read_long_field(vm, receiver(args), "epochDay");
            let hour = int_arg(args, 1);
            let minute = int_arg(args, 2);
            let nano = (i64::from(hour) * 3600 + i64::from(minute) * 60) * 1_000_000_000;
            object(local_date_time_new(vm, epoch_day, nano)?)
        },
    ),
    method(
        "isAfter",
        "(Ljava/time/LocalDate;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            boolean(
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
            boolean(
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
                return boolean(false);
            }
            let class = vm.resolve_class("java/time/LocalDate")?;
            if !vm.is_instance(other, class) {
                return boolean(false);
            }
            boolean(
                read_long_field(vm, receiver(args), "epochDay")
                    == read_long_field(vm, other, "epochDay"),
            )
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(local_date_hash(read_long_field(
            vm,
            receiver(args),
            "epochDay",
        )))
    }),
    method(
        "isSupported",
        "(Ljava/time/temporal/TemporalField;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let field = ref_arg(args, 1);
            match chrono_field_ordinal(vm, field) {
                Some(ordinal) => boolean(matches!(ordinal, 15..=27)),
                None => {
                    let (class, method) = vm.resolve_virtual_method(
                        field,
                        "isSupportedBy",
                        "(Ljava/time/temporal/TemporalAccessor;)Z",
                    )?;
                    vm.invoke_method(
                        class,
                        method,
                        vec![Value::Ref(field), Value::Ref(receiver(args))],
                    )
                }
            }
        },
    ),
    method(
        "getLong",
        "(Ljava/time/temporal/TemporalField;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let field = ref_arg(args, 1);
            let epoch_day = read_long_field(vm, this, "epochDay");
            match chrono_field_ordinal(vm, field) {
                Some(ordinal) => {
                    long(date_field_long(epoch_day, ordinal).ok_or_else(|| unsupported_field(vm))?)
                }
                None => temporal_field_get_from(vm, field, this),
            }
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
        "(JLjava/time/temporal/TemporalUnit;)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            local_date_plus_unit(
                vm,
                receiver(args),
                crate::long_arg(args, 1),
                ref_arg(args, 2),
            )
        },
    ),
    method(
        "plus",
        "(JLjava/time/temporal/TemporalUnit;)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC,
        |vm, _, args| {
            local_date_plus_unit(
                vm,
                receiver(args),
                crate::long_arg(args, 1),
                ref_arg(args, 2),
            )
        },
    ),
    method(
        "minus",
        "(JLjava/time/temporal/TemporalUnit;)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            local_date_minus_unit(
                vm,
                receiver(args),
                crate::long_arg(args, 1),
                ref_arg(args, 2),
            )
        },
    ),
    method(
        "minus",
        "(JLjava/time/temporal/TemporalUnit;)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC,
        |vm, _, args| {
            local_date_minus_unit(
                vm,
                receiver(args),
                crate::long_arg(args, 1),
                ref_arg(args, 2),
            )
        },
    ),
    method(
        "plus",
        "(Ljava/time/temporal/TemporalAmount;)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let amount = ref_arg(args, 1);
            let (class, method) = vm.resolve_virtual_method(
                amount,
                "addTo",
                "(Ljava/time/temporal/Temporal;)Ljava/time/temporal/Temporal;",
            )?;
            vm.invoke_method(class, method, vec![Value::Ref(amount), Value::Ref(this)])
        },
    ),
    method(
        "minus",
        "(Ljava/time/temporal/TemporalAmount;)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let amount = ref_arg(args, 1);
            let (class, method) = vm.resolve_virtual_method(
                amount,
                "subtractFrom",
                "(Ljava/time/temporal/Temporal;)Ljava/time/temporal/Temporal;",
            )?;
            vm.invoke_method(class, method, vec![Value::Ref(amount), Value::Ref(this)])
        },
    ),
    method(
        "with",
        "(Ljava/time/temporal/TemporalField;J)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            local_date_with_field(
                vm,
                receiver(args),
                ref_arg(args, 1),
                crate::long_arg(args, 2),
            )
        },
    ),
    method(
        "with",
        "(Ljava/time/temporal/TemporalField;J)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC,
        |vm, _, args| {
            local_date_with_field(
                vm,
                receiver(args),
                ref_arg(args, 1),
                crate::long_arg(args, 2),
            )
        },
    ),
    method(
        "until",
        "(Ljava/time/temporal/Temporal;Ljava/time/temporal/TemporalUnit;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let end = ref_arg(args, 1);
            let unit = ref_arg(args, 2);
            if let Some(ordinal) = chrono_unit_ordinal(vm, unit) {
                let start = read_long_field(vm, this, "epochDay");
                let end_day = read_long_field(vm, end, "epochDay");
                long(date_until(start, end_day, ordinal).ok_or_else(|| unsupported_unit(vm))?)
            } else {
                let (class, method) = vm.resolve_virtual_method(
                    unit,
                    "between",
                    "(Ljava/time/temporal/Temporal;Ljava/time/temporal/Temporal;)J",
                )?;
                vm.invoke_method(
                    class,
                    method,
                    vec![Value::Ref(unit), Value::Ref(this), Value::Ref(end)],
                )
            }
        },
    ),
    method(
        "datesUntil",
        "(Ljava/time/LocalDate;)Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        |vm, _, args| local_date_dates_until(vm, receiver(args), ref_arg(args, 1), None),
    ),
    method(
        "datesUntil",
        "(Ljava/time/LocalDate;Ljava/time/Period;)Ljava/util/stream/Stream;",
        ACC_PUBLIC,
        |vm, _, args| {
            let step = ref_arg(args, 2);
            if step.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            local_date_dates_until(vm, receiver(args), ref_arg(args, 1), Some(step))
        },
    ),
    method(
        "get",
        "(Ljava/time/temporal/TemporalField;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (class, method) = vm.resolve_virtual_method(
                this,
                "getLong",
                "(Ljava/time/temporal/TemporalField;)J",
            )?;
            let value = vm
                .invoke_method(
                    class,
                    method,
                    vec![Value::Ref(this), Value::Ref(ref_arg(args, 1))],
                )?
                .as_long();
            if value < i64::from(i32::MIN) || value > i64::from(i32::MAX) {
                return Err(vm.throw_new(
                    "java/time/DateTimeException",
                    Some("field value out of range"),
                ));
            }
            int(value as i32)
        },
    ),
    method(
        "format",
        "(Ljava/time/format/DateTimeFormatter;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let formatter = ref_arg(args, 1);
            if formatter.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let pattern = vm
                .read_string_field(formatter, vm.class_of(formatter), "pattern")
                .unwrap_or_default();
            let millis = temporal_local_millis(vm, this)?;
            object(vm.make_string(&crate::text::format_pattern(&pattern, millis))?)
        },
    ),
    method(
        "until",
        "(Ljava/time/chrono/ChronoLocalDate;)Ljava/time/Period;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let end = ref_arg(args, 1);
            if end.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let period_class = vm.resolve_class("java/time/Period")?;
            let (declaring, between) = vm
                .find_method(
                    period_class,
                    "between",
                    "(Ljava/time/LocalDate;Ljava/time/LocalDate;)Ljava/time/Period;",
                )
                .ok_or_else(|| {
                    vm.throw_new("java/lang/NoSuchMethodError", Some("Period.between"))
                })?;
            vm.invoke_method(declaring, between, vec![Value::Ref(this), Value::Ref(end)])
        },
    ),
    method(
        "parse",
        "(Ljava/lang/CharSequence;Ljava/time/format/DateTimeFormatter;)Ljava/time/LocalDate;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 0));
            let formatter = ref_arg(args, 1);
            if formatter.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let pattern = vm
                .read_string_field(formatter, vm.class_of(formatter), "pattern")
                .unwrap_or_default();
            let parts: Vec<&str> = text.split(['-', '/', '.']).collect();
            if parts.len() != 3 {
                return Err(vm.throw_new("java/time/format/DateTimeParseException", Some(&text)));
            }
            let (y, m, d) = if pattern.starts_with("dd") {
                (parts[2], parts[1], parts[0])
            } else {
                (parts[0], parts[1], parts[2])
            };
            let year = y.parse::<i32>().unwrap_or(0);
            let month = m.parse::<i32>().unwrap_or(0);
            let day = d.parse::<i32>().unwrap_or(0);
            object(local_date_new(vm, days_from_civil(year, month, day))?)
        },
    ),
    method(
        "query",
        "(Ljava/time/temporal/TemporalQuery;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let query = ref_arg(args, 1);
            if query.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            crate::temporal::temporal_query_from(vm, query, receiver(args))
        },
    ),
    method(
        "from",
        "(Ljava/time/temporal/TemporalAccessor;)Ljava/time/LocalDate;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let temporal = ref_arg(args, 0);
            if temporal.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let name = String::from(vm.class_name(vm.class_of(temporal)));
            match name.as_str() {
                "java/time/LocalDate" => object(temporal),
                "java/time/LocalDateTime"
                | "java/time/OffsetDateTime"
                | "java/time/ZonedDateTime" => {
                    let (class, method) = vm.resolve_virtual_method(
                        temporal,
                        "toLocalDate",
                        "()Ljava/time/LocalDate;",
                    )?;
                    vm.invoke_method(class, method, alloc::vec![Value::Ref(temporal)])
                }
                _ => {
                    let epoch_day = vm.read_instance_long_named(
                        temporal,
                        vm.class_of(temporal),
                        "epochDay",
                        "J",
                    );
                    match epoch_day {
                        Some(days) => object(local_date_new(vm, days)?),
                        None => Err(vm.throw_new(
                            "java/time/DateTimeException",
                            Some("Unable to obtain LocalDate from TemporalAccessor"),
                        )),
                    }
                }
            }
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
    &[
        field("epochDay", "J", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "MIN",
            "Ljava/time/LocalDate;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MAX",
            "Ljava/time/LocalDate;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &LOCAL_DATE_METHODS,
    Some(local_date_clinit),
);

const LOCAL_TIME_METHODS: [oxjvm_vm::NativeMethodDef; 40] = [
    method(
        "of",
        "(II)Ljava/time/LocalTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let hour = int_arg(args, 0);
            let minute = int_arg(args, 1);
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
            let hour = int_arg(args, 0);
            let minute = int_arg(args, 1);
            let second = int_arg(args, 2);
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
            let hour = int_arg(args, 0);
            let minute = int_arg(args, 1);
            let second = int_arg(args, 2);
            let nano = int_arg(args, 3);
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
    method(
        "now",
        "(Ljava/time/Clock;)Ljava/time/LocalTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let (_, nano_of_day) = clock_local_parts(vm, ref_arg(args, 0))?;
            object(local_time_new(vm, nano_of_day)?)
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
                return boolean(false);
            }
            let class = vm.resolve_class("java/time/LocalTime")?;
            if !vm.is_instance(other, class) {
                return boolean(false);
            }
            boolean(
                read_long_field(vm, receiver(args), "nanoOfDay")
                    == read_long_field(vm, other, "nanoOfDay"),
            )
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(local_time_hash(read_long_field(
            vm,
            receiver(args),
            "nanoOfDay",
        )))
    }),
    method(
        "isSupported",
        "(Ljava/time/temporal/TemporalField;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let ordinal = chrono_field_ordinal(vm, ref_arg(args, 1));
            boolean(matches!(ordinal, Some(0..=14)))
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
    method(
        "adjustInto",
        "(Ljava/time/temporal/Temporal;)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let temporal = ref_arg(args, 1);
            let nano_of_day = read_long_field(vm, this, "nanoOfDay");
            let field = vm.resolve_class("java/time/temporal/ChronoField")?;
            vm.ensure_initialized(field)?;
            let nano_of_day_field = vm
                .read_static_ref_if_set(field, "NANO_OF_DAY", "Ljava/time/temporal/ChronoField;")
                .unwrap_or(ObjectRef::NULL);
            let (class, method) = vm.resolve_virtual_method(
                temporal,
                "with",
                "(Ljava/time/temporal/TemporalField;J)Ljava/time/temporal/Temporal;",
            )?;
            vm.invoke_method(
                class,
                method,
                vec![
                    Value::Ref(temporal),
                    Value::Ref(nano_of_day_field),
                    long(nano_of_day)?,
                ],
            )
        },
    ),
    method(
        "parse",
        "(Ljava/lang/CharSequence;)Ljava/time/LocalTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = crate::string_arg(vm, args[0])?;
            let error = |vm: &mut Vm<'_>| {
                vm.throw_new("java/time/format/DateTimeParseException", Some(&text))
            };
            let parts: Vec<&str> = text.split(':').collect();
            if parts.len() < 2 || parts.len() > 3 {
                return Err(error(vm));
            }
            let hour: i64 = parts[0].parse().map_err(|_| error(vm))?;
            let minute: i64 = parts[1].parse().map_err(|_| error(vm))?;
            let (second, nano) = if parts.len() == 3 {
                let mut split = parts[2].splitn(2, '.');
                let sec: i64 = split.next().unwrap_or("0").parse().map_err(|_| error(vm))?;
                let frac = split.next().unwrap_or("");
                let nano = if frac.is_empty() {
                    if parts[2].contains('.') {
                        return Err(error(vm));
                    }
                    0
                } else {
                    if frac.len() > 9 || !frac.bytes().all(|b| b.is_ascii_digit()) {
                        return Err(error(vm));
                    }
                    let mut padded = String::from(frac);
                    while padded.len() < 9 {
                        padded.push('0');
                    }
                    padded.parse::<i64>().map_err(|_| error(vm))?
                };
                (sec, nano)
            } else {
                (0, 0)
            };
            if hour > 23 || minute > 59 || second > 59 {
                return Err(error(vm));
            }
            object(local_time_new(
                vm,
                ((hour * 3600 + minute * 60 + second) * 1_000_000_000) + nano,
            )?)
        },
    ),
    method(
        "plusMinutes",
        "(J)Ljava/time/LocalTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let nano = read_long_field(vm, receiver(args), "nanoOfDay");
            object(local_time_new(
                vm,
                (nano + crate::long_arg(args, 1) * 60_000_000_000).rem_euclid(86_400_000_000_000),
            )?)
        },
    ),
    method(
        "minusMinutes",
        "(J)Ljava/time/LocalTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let nano = read_long_field(vm, receiver(args), "nanoOfDay");
            object(local_time_new(
                vm,
                (nano - crate::long_arg(args, 1) * 60_000_000_000).rem_euclid(86_400_000_000_000),
            )?)
        },
    ),
    method(
        "plusSeconds",
        "(J)Ljava/time/LocalTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let nano = read_long_field(vm, receiver(args), "nanoOfDay");
            object(local_time_new(
                vm,
                (nano + crate::long_arg(args, 1) * 1_000_000_000).rem_euclid(86_400_000_000_000),
            )?)
        },
    ),
    method(
        "minusSeconds",
        "(J)Ljava/time/LocalTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let nano = read_long_field(vm, receiver(args), "nanoOfDay");
            object(local_time_new(
                vm,
                (nano - crate::long_arg(args, 1) * 1_000_000_000).rem_euclid(86_400_000_000_000),
            )?)
        },
    ),
    method(
        "plusNanos",
        "(J)Ljava/time/LocalTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let nano = read_long_field(vm, receiver(args), "nanoOfDay");
            object(local_time_new(
                vm,
                (nano + crate::long_arg(args, 1)).rem_euclid(86_400_000_000_000),
            )?)
        },
    ),
    method(
        "minusNanos",
        "(J)Ljava/time/LocalTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let nano = read_long_field(vm, receiver(args), "nanoOfDay");
            object(local_time_new(
                vm,
                (nano - crate::long_arg(args, 1)).rem_euclid(86_400_000_000_000),
            )?)
        },
    ),
    method(
        "format",
        "(Ljava/time/format/DateTimeFormatter;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let formatter = ref_arg(args, 1);
            if formatter.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let pattern = vm
                .read_string_field(formatter, vm.class_of(formatter), "pattern")
                .unwrap_or_default();
            let millis = temporal_local_millis(vm, this)?;
            object(vm.make_string(&crate::text::format_pattern(&pattern, millis))?)
        },
    ),
    method(
        "query",
        "(Ljava/time/temporal/TemporalQuery;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let query = ref_arg(args, 1);
            if query.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            crate::temporal::temporal_query_from(vm, query, receiver(args))
        },
    ),
    method(
        "truncatedTo",
        "(Ljava/time/temporal/TemporalUnit;)Ljava/time/LocalTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let unit = ref_arg(args, 1);
            if unit.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let nanos = crate::temporal::chrono_unit_nanos(vm, unit);
            let day_nanos = 86_400_000_000_000i64;
            let nano_of_day = read_long_field(vm, receiver(args), "nanoOfDay");
            let truncated = if nanos == day_nanos {
                0
            } else if nanos <= 0 || nanos > day_nanos {
                return Err(vm.throw_new(
                    "java/time/temporal/UnsupportedTemporalTypeException",
                    Some("Unit is too large to be used for truncation"),
                ));
            } else if day_nanos % nanos != 0 {
                return Err(vm.throw_new(
                    "java/time/temporal/UnsupportedTemporalTypeException",
                    Some("Unit must divide into a standard day without remainder"),
                ));
            } else {
                nano_of_day / nanos * nanos
            };
            object(local_time_new(vm, truncated)?)
        },
    ),
    method(
        "isBefore",
        "(Ljava/time/LocalTime;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            boolean(
                read_long_field(vm, receiver(args), "nanoOfDay")
                    < read_long_field(vm, ref_arg(args, 1), "nanoOfDay"),
            )
        },
    ),
    method(
        "isAfter",
        "(Ljava/time/LocalTime;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            boolean(
                read_long_field(vm, receiver(args), "nanoOfDay")
                    > read_long_field(vm, ref_arg(args, 1), "nanoOfDay"),
            )
        },
    ),
    method(
        "atDate",
        "(Ljava/time/LocalDate;)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let date = ref_arg(args, 1);
            if date.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(local_date_time_new(
                vm,
                read_long_field(vm, date, "epochDay"),
                read_long_field(vm, this, "nanoOfDay"),
            )?)
        },
    ),
];

pub(crate) const LOCAL_TIME: oxjvm_vm::NativeClass = class(
    "java/time/LocalTime",
    Some("java/lang/Object"),
    &[
        "java/time/temporal/TemporalAccessor",
        "java/time/temporal/Temporal",
        "java/time/temporal/TemporalAdjuster",
        "java/lang/Comparable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("nanoOfDay", "J", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "HOURS",
            "[Ljava/time/LocalTime;",
            ACC_PRIVATE | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MIDNIGHT",
            "Ljava/time/LocalTime;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MIN",
            "Ljava/time/LocalTime;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MAX",
            "Ljava/time/LocalTime;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "NOON",
            "Ljava/time/LocalTime;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &LOCAL_TIME_METHODS,
    Some(local_time_clinit),
);

const LOCAL_DATE_TIME_METHODS: [oxjvm_vm::NativeMethodDef; 67] = [
    method(
        "of",
        "(IIIII)Ljava/time/LocalDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let year = int_arg(args, 0);
            let month = int_arg(args, 1);
            let day = int_arg(args, 2);
            let hour = int_arg(args, 3);
            let minute = int_arg(args, 4);
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
            let year = int_arg(args, 0);
            let month = int_arg(args, 1);
            let day = int_arg(args, 2);
            let hour = int_arg(args, 3);
            let minute = int_arg(args, 4);
            let second = int_arg(args, 5);
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
            let year = int_arg(args, 0);
            let month = int_arg(args, 1);
            let day = int_arg(args, 2);
            let hour = int_arg(args, 3);
            let minute = int_arg(args, 4);
            let second = int_arg(args, 5);
            let nano_of_second = int_arg(args, 6);
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
            object(local_date_time_with_parts(vm, date, time)?)
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
    method(
        "now",
        "(Ljava/time/Clock;)Ljava/time/LocalDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let (epoch_day, nano_of_day) = clock_local_parts(vm, ref_arg(args, 0))?;
            object(local_date_time_new(vm, epoch_day, nano_of_day)?)
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
            let this = receiver(args);
            let date = ldt_date_ref(vm, this);
            if !date.is_null() {
                return object(date);
            }
            object(local_date_new(vm, read_long_field(vm, this, "epochDay"))?)
        },
    ),
    method(
        "toLocalTime",
        "()Ljava/time/LocalTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let time = ldt_time_ref(vm, this);
            if !time.is_null() {
                return object(time);
            }
            object(local_time_new(vm, read_long_field(vm, this, "nanoOfDay"))?)
        },
    ),
    method(
        "plusDays",
        "(J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| ldt_shift_date(vm, receiver(args), crate::long_arg(args, 1), 7),
    ),
    method(
        "minusDays",
        "(J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| ldt_shift_date(vm, receiver(args), -crate::long_arg(args, 1), 7),
    ),
    method(
        "plusHours",
        "(J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            ldt_shift_nanos(
                vm,
                receiver(args),
                crate::long_arg(args, 1) * 3_600_000_000_000,
            )
        },
    ),
    method(
        "minusHours",
        "(J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            ldt_shift_nanos(
                vm,
                receiver(args),
                -crate::long_arg(args, 1) * 3_600_000_000_000,
            )
        },
    ),
    method(
        "plusMinutes",
        "(J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            ldt_shift_nanos(
                vm,
                receiver(args),
                crate::long_arg(args, 1) * 60_000_000_000,
            )
        },
    ),
    method(
        "minusMinutes",
        "(J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            ldt_shift_nanos(
                vm,
                receiver(args),
                -crate::long_arg(args, 1) * 60_000_000_000,
            )
        },
    ),
    method(
        "plusSeconds",
        "(J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| ldt_shift_nanos(vm, receiver(args), crate::long_arg(args, 1) * 1_000_000_000),
    ),
    method(
        "minusSeconds",
        "(J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            ldt_shift_nanos(
                vm,
                receiver(args),
                -crate::long_arg(args, 1) * 1_000_000_000,
            )
        },
    ),
    method(
        "plusNanos",
        "(J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| ldt_shift_nanos(vm, receiver(args), crate::long_arg(args, 1)),
    ),
    method(
        "minusNanos",
        "(J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| ldt_shift_nanos(vm, receiver(args), -crate::long_arg(args, 1)),
    ),
    method(
        "plusWeeks",
        "(J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| ldt_shift_date(vm, receiver(args), crate::long_arg(args, 1), 8),
    ),
    method(
        "minusWeeks",
        "(J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| ldt_shift_date(vm, receiver(args), -crate::long_arg(args, 1), 8),
    ),
    method(
        "plusMonths",
        "(J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| ldt_shift_date(vm, receiver(args), crate::long_arg(args, 1), 9),
    ),
    method(
        "minusMonths",
        "(J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| ldt_shift_date(vm, receiver(args), -crate::long_arg(args, 1), 9),
    ),
    method(
        "plusYears",
        "(J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| ldt_shift_date(vm, receiver(args), crate::long_arg(args, 1), 10),
    ),
    method(
        "minusYears",
        "(J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| ldt_shift_date(vm, receiver(args), -crate::long_arg(args, 1), 10),
    ),
    method(
        "withHour",
        "(I)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| ldt_with_time_field(vm, receiver(args), 12, i64::from(int_arg(args, 1))),
    ),
    method(
        "withMinute",
        "(I)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| ldt_with_time_field(vm, receiver(args), 8, i64::from(int_arg(args, 1))),
    ),
    method(
        "withSecond",
        "(I)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| ldt_with_time_field(vm, receiver(args), 6, i64::from(int_arg(args, 1))),
    ),
    method(
        "withNano",
        "(I)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| ldt_with_time_field(vm, receiver(args), 0, i64::from(int_arg(args, 1))),
    ),
    method(
        "withYear",
        "(I)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| ldt_with_date_field(vm, receiver(args), 26, i64::from(int_arg(args, 1))),
    ),
    method(
        "withMonth",
        "(I)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| ldt_with_date_field(vm, receiver(args), 23, i64::from(int_arg(args, 1))),
    ),
    method(
        "withDayOfMonth",
        "(I)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| ldt_with_date_field(vm, receiver(args), 18, i64::from(int_arg(args, 1))),
    ),
    method(
        "withDayOfYear",
        "(I)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| ldt_with_date_field(vm, receiver(args), 19, i64::from(int_arg(args, 1))),
    ),
    method("getDayOfYear", "()I", ACC_PUBLIC, |vm, _, args| {
        let epoch_day = read_long_field(vm, receiver(args), "epochDay");
        let (year, _, _) = civil_from_days(epoch_day);
        int((epoch_day - days_from_civil(year, 1, 1) + 1) as i32)
    }),
    method("getNano", "()I", ACC_PUBLIC, |vm, _, args| {
        int((read_long_field(vm, receiver(args), "nanoOfDay") % 1_000_000_000) as i32)
    }),
    method(
        "with",
        "(Ljava/time/temporal/TemporalField;J)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            ldt_with_field(
                vm,
                receiver(args),
                ref_arg(args, 1),
                crate::long_arg(args, 2),
            )
        },
    ),
    method(
        "with",
        "(Ljava/time/temporal/TemporalField;J)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC,
        |vm, _, args| {
            ldt_with_field(
                vm,
                receiver(args),
                ref_arg(args, 1),
                crate::long_arg(args, 2),
            )
        },
    ),
    method(
        "ofEpochSecond",
        "(JILjava/time/ZoneOffset;)Ljava/time/LocalDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let epoch_second = crate::long_arg(args, 0);
            let nano_of_second = int_arg(args, 1);
            let offset = zone_offset_seconds(vm, ref_arg(args, 2));
            let local_second = epoch_second + i64::from(offset);
            let epoch_day = local_second.div_euclid(86_400);
            let nano_of_day =
                local_second.rem_euclid(86_400) * 1_000_000_000 + i64::from(nano_of_second);
            object(local_date_time_new(vm, epoch_day, nano_of_day)?)
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
            boolean(
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
            boolean(
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
            let nanos = nano % 1_000_000_000;
            let time_text = if second == 0 && nanos == 0 {
                alloc::format!("{hour:02}:{minute:02}")
            } else {
                alloc::format!("{hour:02}:{minute:02}:{second:02}{}", nano_fraction(nanos))
            };
            let text = alloc::format!("{year:04}-{month:02}-{day:02}T{time_text}");
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
                return boolean(false);
            }
            let class = vm.resolve_class("java/time/LocalDateTime")?;
            if !vm.is_instance(other, class) {
                return boolean(false);
            }
            let this = receiver(args);
            boolean(
                read_long_field(vm, this, "epochDay") == read_long_field(vm, other, "epochDay")
                    && read_long_field(vm, this, "nanoOfDay")
                        == read_long_field(vm, other, "nanoOfDay"),
            )
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        int(local_date_hash(read_long_field(vm, this, "epochDay"))
            ^ local_time_hash(read_long_field(vm, this, "nanoOfDay")))
    }),
    method(
        "isSupported",
        "(Ljava/time/temporal/TemporalField;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let ordinal = chrono_field_ordinal(vm, ref_arg(args, 1));
            boolean(matches!(ordinal, Some(0..=27)))
        },
    ),
    method(
        "getLong",
        "(Ljava/time/temporal/TemporalField;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let field = ref_arg(args, 1);
            let nano_of_day = read_long_field(vm, this, "nanoOfDay");
            let epoch_day = read_long_field(vm, this, "epochDay");
            match chrono_field_ordinal(vm, field) {
                Some(ordinal) => {
                    if let Some(value) = time_field_long(nano_of_day, ordinal) {
                        return long(value);
                    }
                    long(date_field_long(epoch_day, ordinal).ok_or_else(|| unsupported_field(vm))?)
                }
                None => temporal_field_get_from(vm, field, this),
            }
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
        "(JLjava/time/temporal/TemporalUnit;)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            ldt_shift_unit(
                vm,
                receiver(args),
                crate::long_arg(args, 1),
                ref_arg(args, 2),
            )
        },
    ),
    method(
        "plus",
        "(JLjava/time/temporal/TemporalUnit;)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC,
        |vm, _, args| {
            ldt_shift_unit(
                vm,
                receiver(args),
                crate::long_arg(args, 1),
                ref_arg(args, 2),
            )
        },
    ),
    method(
        "minus",
        "(JLjava/time/temporal/TemporalUnit;)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            ldt_shift_unit(
                vm,
                receiver(args),
                -crate::long_arg(args, 1),
                ref_arg(args, 2),
            )
        },
    ),
    method(
        "minus",
        "(JLjava/time/temporal/TemporalUnit;)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC,
        |vm, _, args| {
            ldt_shift_unit(
                vm,
                receiver(args),
                -crate::long_arg(args, 1),
                ref_arg(args, 2),
            )
        },
    ),
    method(
        "plus",
        "(Ljava/time/temporal/TemporalAmount;)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let amount = ref_arg(args, 1);
            let (class, method) = vm.resolve_virtual_method(
                amount,
                "addTo",
                "(Ljava/time/temporal/Temporal;)Ljava/time/temporal/Temporal;",
            )?;
            vm.invoke_method(class, method, vec![Value::Ref(amount), Value::Ref(this)])
        },
    ),
    method(
        "minus",
        "(Ljava/time/temporal/TemporalAmount;)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let amount = ref_arg(args, 1);
            let (class, method) = vm.resolve_virtual_method(
                amount,
                "subtractFrom",
                "(Ljava/time/temporal/Temporal;)Ljava/time/temporal/Temporal;",
            )?;
            vm.invoke_method(class, method, vec![Value::Ref(amount), Value::Ref(this)])
        },
    ),
    method(
        "with",
        "(Ljava/time/temporal/TemporalAdjuster;)Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let adjuster = ref_arg(args, 1);
            let (class, method) = vm.resolve_virtual_method(
                adjuster,
                "adjustInto",
                "(Ljava/time/temporal/Temporal;)Ljava/time/temporal/Temporal;",
            )?;
            vm.invoke_method(class, method, vec![Value::Ref(adjuster), Value::Ref(this)])
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
            let nano = (nano_of_day % 1_000_000_000) as i32;
            zoned_date_time_new(
                vm,
                (local_second - i64::from(offset)) * 1000 + i64::from(nano) / 1_000_000,
                nano,
                zone,
            )
        },
    ),
    method(
        "toInstant",
        "(Ljava/time/ZoneOffset;)Ljava/time/Instant;",
        ACC_PUBLIC,
        |vm, _, args| {
            let offset = ref_arg(args, 1);
            if offset.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let this = receiver(args);
            let epoch_day = read_long_field(vm, this, "epochDay");
            let nano_of_day = read_long_field(vm, this, "nanoOfDay");
            let seconds = epoch_day * 86_400 + nano_of_day / 1_000_000_000
                - i64::from(zone_offset_seconds(vm, offset));
            instant_new(vm, seconds, (nano_of_day % 1_000_000_000) as i32)
        },
    ),
    method(
        "getDayOfWeek",
        "()Ljava/time/DayOfWeek;",
        ACC_PUBLIC,
        |vm, _, args| {
            let epoch_day = read_long_field(vm, receiver(args), "epochDay");
            let value = ((epoch_day + 3).rem_euclid(7) + 1) as i32;
            object(day_of_week_of(vm, value)?)
        },
    ),
    method(
        "format",
        "(Ljava/time/format/DateTimeFormatter;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let formatter = ref_arg(args, 1);
            if formatter.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let pattern = vm
                .read_string_field(formatter, vm.class_of(formatter), "pattern")
                .unwrap_or_default();
            let millis = temporal_local_millis(vm, this)?;
            object(vm.make_string(&crate::text::format_pattern(&pattern, millis))?)
        },
    ),
    method(
        "query",
        "(Ljava/time/temporal/TemporalQuery;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let query = ref_arg(args, 1);
            if query.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            crate::temporal::temporal_query_from(vm, query, receiver(args))
        },
    ),
    method(
        "parse",
        "(Ljava/lang/CharSequence;)Ljava/time/LocalDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 0));
            let error = |vm: &mut Vm<'_>| {
                vm.throw_new("java/time/format/DateTimeParseException", Some(&text))
            };
            let (date_text, time_text) = text.split_once('T').ok_or_else(|| error(vm))?;
            let date_parts: Vec<&str> = date_text.split('-').collect();
            if date_parts.len() != 3 {
                return Err(error(vm));
            }
            let year: i32 = date_parts[0].parse().map_err(|_| error(vm))?;
            let month: i32 = date_parts[1].parse().map_err(|_| error(vm))?;
            let day: i32 = date_parts[2].parse().map_err(|_| error(vm))?;
            let epoch_day = days_from_civil(year, month, day);
            let time_parts: Vec<&str> = time_text.split(':').collect();
            if time_parts.len() < 2 || time_parts.len() > 3 {
                return Err(error(vm));
            }
            let hour: i64 = time_parts[0].parse().map_err(|_| error(vm))?;
            let minute: i64 = time_parts[1].parse().map_err(|_| error(vm))?;
            let (second, nano) = if time_parts.len() == 3 {
                let mut split = time_parts[2].splitn(2, '.');
                let sec: i64 = split.next().unwrap_or("0").parse().map_err(|_| error(vm))?;
                let frac = split.next().unwrap_or("");
                let nano = if frac.is_empty() {
                    0
                } else {
                    if frac.len() > 9 || !frac.bytes().all(|b| b.is_ascii_digit()) {
                        return Err(error(vm));
                    }
                    let mut padded = String::from(frac);
                    while padded.len() < 9 {
                        padded.push('0');
                    }
                    padded.parse::<i64>().map_err(|_| error(vm))?
                };
                (sec, nano)
            } else {
                (0, 0)
            };
            if hour > 23 || minute > 59 || second > 59 {
                return Err(error(vm));
            }
            let nano_of_day = ((hour * 60 + minute) * 60 + second) * 1_000_000_000 + nano;
            object(local_date_time_new(vm, epoch_day, nano_of_day)?)
        },
    ),
];

/// The fixed offset (seconds) of a `ZoneId`/`ZoneOffset` argument (region transitions not modelled).
pub(crate) fn zone_offset_seconds(vm: &Vm<'_>, zone: ObjectRef) -> i32 {
    let class = vm.class_of(zone);
    if let Some((declaring, field)) = vm.find_field_opt(class, "totalSeconds", "I") {
        if let Some(value) = vm.read_instance_int(zone, declaring, field) {
            return value;
        }
    }
    // Fixed-offset zones built by ZoneId.ofOffset carry a GMT/UTC/UT-prefixed id.
    let id = vm.read_string_field(zone, class, "id").unwrap_or_default();
    for prefix in ["GMT", "UTC", "UT"] {
        if let Some(rest) = id.strip_prefix(prefix) {
            if rest.is_empty() {
                return 0;
            }
            if let Some(offset) = crate::lang::zone_offset_parse(rest) {
                return offset;
            }
        }
    }
    0
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
        field("epochDay", "J", ACC_PRIVATE | ACC_FINAL, None),
        field("nanoOfDay", "J", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "date",
            "Ljava/time/LocalDate;",
            ACC_PRIVATE | ACC_FINAL,
            None,
        ),
        field(
            "time",
            "Ljava/time/LocalTime;",
            ACC_PRIVATE | ACC_FINAL,
            None,
        ),
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

/// English display name for a day/month constant at a `TextStyle` ordinal
/// (FULL/FULL_STANDALONE, SHORT/SHORT_STANDALONE, NARROW/NARROW_STANDALONE).
fn english_display_name(name: &str, style: i32) -> String {
    let mut full = String::new();
    for (index, ch) in name.chars().enumerate() {
        if index == 0 {
            full.extend(ch.to_uppercase());
        } else {
            full.extend(ch.to_lowercase());
        }
    }
    match style {
        0 | 1 => full,
        2 | 3 => full.chars().take(3).collect(),
        _ => full.chars().take(1).collect(),
    }
}

/// Non-`ChronoField` `getLong`: delegate to the field's own `getFrom`.
fn temporal_field_get_from(
    vm: &mut Vm<'_>,
    field: ObjectRef,
    temporal: ObjectRef,
) -> Result<Value, VmError> {
    let (class, method) =
        vm.resolve_virtual_method(field, "getFrom", "(Ljava/time/temporal/TemporalAccessor;)J")?;
    vm.invoke_method(class, method, vec![Value::Ref(field), Value::Ref(temporal)])
}

/// `DayOfWeek.of`/`plus`/`minus`: return the cached enum singleton (identity).
fn day_of_week_of(vm: &mut Vm<'_>, value: i32) -> Result<ObjectRef, VmError> {
    if !(1..=7).contains(&value) {
        return Err(vm.throw_new("java/time/DateTimeException", Some("Invalid day of week")));
    }
    let class = vm.resolve_class("java/time/DayOfWeek")?;
    vm.ensure_initialized(class)?;
    let name = DAY_OF_WEEK_NAMES[(value - 1) as usize];
    if let Some(cached) = vm.read_static_ref_if_set(class, name, "Ljava/time/DayOfWeek;") {
        return Ok(cached);
    }
    day_of_week_new(vm, value)
}

fn day_of_week_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/time/DayOfWeek")?;
    for (index, name) in DAY_OF_WEEK_NAMES.iter().enumerate() {
        let instance = day_of_week_new(vm, index as i32 + 1)?;
        vm.set_static_value(class, name, "Ljava/time/DayOfWeek;", Value::Ref(instance));
    }
    Ok(())
}

fn day_of_week_new(vm: &mut Vm<'_>, value: i32) -> Result<ObjectRef, VmError> {
    if !(1..=7).contains(&value) {
        return Err(vm.throw_new("java/time/DateTimeException", Some("Invalid day of week")));
    }
    let class = vm.resolve_class("java/time/DayOfWeek")?;
    let instance = vm.new_instance(class)?;
    write_int_field(vm, instance, "value", value);
    Ok(instance)
}

const DAY_OF_WEEK_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "of",
        "(I)Ljava/time/DayOfWeek;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(day_of_week_of(vm, int_arg(args, 0))?),
    ),
    method(
        "values",
        "()[Ljava/time/DayOfWeek;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/time/DayOfWeek")?;
            let array = vm.allocate_object_array(class, 7)?;
            for value in 1..=7 {
                let instance = day_of_week_new(vm, value)?;
                vm.array_set_ref(array, (value - 1) as usize, instance)?;
            }
            object(array)
        },
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
            object(day_of_week_of(vm, shifted as i32)?)
        },
    ),
    method(
        "minus",
        "(J)Ljava/time/DayOfWeek;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = read_int_field(vm, receiver(args), "value");
            let shifted = (i64::from(value - 1) - crate::long_arg(args, 1)).rem_euclid(7) + 1;
            object(day_of_week_of(vm, shifted as i32)?)
        },
    ),
    method(
        "getDisplayName",
        "(Ljava/time/format/TextStyle;Ljava/util/Locale;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let style = ref_arg(args, 1);
            if style.is_null() || ref_arg(args, 2).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let value = read_int_field(vm, receiver(args), "value");
            let name = DAY_OF_WEEK_NAMES
                .get((value - 1) as usize)
                .copied()
                .unwrap_or("");
            let ordinal = read_int_field(vm, style, "ordinal");
            object(vm.make_string(&english_display_name(name, ordinal))?)
        },
    ),
];

pub(crate) const DAY_OF_WEEK: oxjvm_vm::NativeClass = class(
    "java/time/DayOfWeek",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field("value", "I", ACC_PRIVATE, None),
        field(
            "MONDAY",
            "Ljava/time/DayOfWeek;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "TUESDAY",
            "Ljava/time/DayOfWeek;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "WEDNESDAY",
            "Ljava/time/DayOfWeek;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "THURSDAY",
            "Ljava/time/DayOfWeek;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "FRIDAY",
            "Ljava/time/DayOfWeek;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SATURDAY",
            "Ljava/time/DayOfWeek;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SUNDAY",
            "Ljava/time/DayOfWeek;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &DAY_OF_WEEK_METHODS,
    Some(day_of_week_clinit),
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

/// `Month.of`/`plus`/`minus`: `Month` is an enum, so return the cached singleton
/// for the value rather than a fresh instance (identity semantics).
fn month_of(vm: &mut Vm<'_>, value: i32) -> Result<ObjectRef, VmError> {
    if !(1..=12).contains(&value) {
        return Err(vm.throw_new("java/time/DateTimeException", Some("Invalid month")));
    }
    let class = vm.resolve_class("java/time/Month")?;
    vm.ensure_initialized(class)?;
    let name = MONTH_NAMES[(value - 1) as usize];
    if let Some(cached) = vm.read_static_ref_if_set(class, name, "Ljava/time/Month;") {
        return Ok(cached);
    }
    month_new(vm, value)
}

fn month_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/time/Month")?;
    for (index, name) in MONTH_NAMES.iter().enumerate() {
        let instance = month_new(vm, index as i32 + 1)?;
        vm.set_static_value(class, name, "Ljava/time/Month;", Value::Ref(instance));
    }
    Ok(())
}

const MONTH_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method(
        "of",
        "(I)Ljava/time/Month;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(month_of(vm, int_arg(args, 0))?),
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
        object(month_of(vm, shifted as i32)?)
    }),
    method(
        "minus",
        "(J)Ljava/time/Month;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = read_int_field(vm, receiver(args), "value");
            let amount = crate::long_arg(args, 1);
            let shifted = (i64::from(value - 1) - amount).rem_euclid(12) + 1;
            object(month_of(vm, shifted as i32)?)
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
            boolean(matches!(ordinal, Some(23)))
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
    method("length", "(Z)I", ACC_PUBLIC, |vm, _, args| {
        let value = read_int_field(vm, receiver(args), "value");
        let leap = int_arg(args, 1) != 0;
        int(match value {
            2 => {
                if leap {
                    29
                } else {
                    28
                }
            }
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        })
    }),
    method(
        "adjustInto",
        "(Ljava/time/temporal/Temporal;)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let temporal = ref_arg(args, 1);
            let value = i64::from(read_int_field(vm, this, "value"));
            let field = vm.resolve_class("java/time/temporal/ChronoField")?;
            vm.ensure_initialized(field)?;
            let month_of_year = vm
                .read_static_ref_if_set(field, "MONTH_OF_YEAR", "Ljava/time/temporal/ChronoField;")
                .unwrap_or(ObjectRef::NULL);
            let (class, method) = vm.resolve_virtual_method(
                temporal,
                "with",
                "(Ljava/time/temporal/TemporalField;J)Ljava/time/temporal/Temporal;",
            )?;
            vm.invoke_method(
                class,
                method,
                vec![
                    Value::Ref(temporal),
                    Value::Ref(month_of_year),
                    long(value)?,
                ],
            )
        },
    ),
    method(
        "getDisplayName",
        "(Ljava/time/format/TextStyle;Ljava/util/Locale;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let style = ref_arg(args, 1);
            if style.is_null() || ref_arg(args, 2).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let value = read_int_field(vm, receiver(args), "value").clamp(1, 12) as usize;
            let ordinal = read_int_field(vm, style, "ordinal");
            object(vm.make_string(&english_display_name(MONTH_NAMES[value - 1], ordinal))?)
        },
    ),
    method("maxLength", "()I", ACC_PUBLIC, |vm, _, args| {
        let value = read_int_field(vm, receiver(args), "value");
        int(match value {
            2 => 29,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        })
    }),
    method("minLength", "()I", ACC_PUBLIC, |vm, _, args| {
        let value = read_int_field(vm, receiver(args), "value");
        int(match value {
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        })
    }),
    method("length", "(Z)I", ACC_PUBLIC, |vm, _, args| {
        let value = read_int_field(vm, receiver(args), "value");
        let leap = int_arg(args, 1) != 0;
        int(match value {
            2 => {
                if leap {
                    29
                } else {
                    28
                }
            }
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        })
    }),
];

pub(crate) const MONTH: oxjvm_vm::NativeClass = class(
    "java/time/Month",
    Some("java/lang/Object"),
    &[
        "java/time/temporal/TemporalAccessor",
        "java/time/temporal/TemporalAdjuster",
    ],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
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
// java.time.Period
// -------------------------------------------------------------------------------------------

/// `Period.minusX(long)`: negate with `Long.MIN_VALUE` handled as `plus(MAX).plus(1)`.
fn period_minus(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    plus_name: &str,
    amount: i64,
) -> Result<Value, VmError> {
    let (class, method) = vm.resolve_virtual_method(this, plus_name, "(J)Ljava/time/Period;")?;
    if amount == i64::MIN {
        let first = vm.invoke_method(class, method, vec![Value::Ref(this), long(i64::MAX)?])?;
        return vm.invoke_method(class, method, vec![first, long(1)?]);
    }
    vm.invoke_method(class, method, vec![Value::Ref(this), long(-amount)?])
}

fn period_new(vm: &mut Vm<'_>, years: i32, months: i32, days: i32) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/time/Period")?;
    let instance = vm.new_instance(class)?;
    write_int_field(vm, instance, "years", years);
    write_int_field(vm, instance, "months", months);
    write_int_field(vm, instance, "days", days);
    Ok(instance)
}

fn period_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/time/Period")?;
    let zero = period_new(vm, 0, 0, 0)?;
    vm.set_static_value(class, "ZERO", "Ljava/time/Period;", Value::Ref(zero));
    Ok(())
}

/// `Period.of` and the single-unit factories: a zero period is the `ZERO`
/// singleton, matching the JDK's identity guarantee.
fn period_of(vm: &mut Vm<'_>, years: i32, months: i32, days: i32) -> Result<ObjectRef, VmError> {
    if years == 0 && months == 0 && days == 0 {
        let class = vm.resolve_class("java/time/Period")?;
        vm.ensure_initialized(class)?;
        if let Some(zero) = vm.read_static_ref_if_set(class, "ZERO", "Ljava/time/Period;") {
            return Ok(zero);
        }
    }
    period_new(vm, years, months, days)
}

/// A `ChronoUnit` singleton by name, for `Period` arithmetic delegation.
pub(crate) fn chrono_unit_named(vm: &mut Vm<'_>, name: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/time/temporal/ChronoUnit")?;
    vm.ensure_initialized(class)?;
    Ok(vm
        .read_static_ref_if_set(class, name, "Ljava/time/temporal/ChronoUnit;")
        .unwrap_or(ObjectRef::NULL))
}

/// `Period.addTo`/`subtractFrom`: add the total months then the days, mirroring
/// the JDK (a zero period returns the temporal unchanged).
fn period_shift(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    temporal: ObjectRef,
    negate: bool,
) -> Result<Value, VmError> {
    let sign = if negate { -1i64 } else { 1i64 };
    let years = i64::from(read_int_field(vm, this, "years"));
    let months = i64::from(read_int_field(vm, this, "months"));
    let days = i64::from(read_int_field(vm, this, "days"));
    let total_months = (years * 12 + months) * sign;
    let mut result = Value::Ref(temporal);
    if total_months != 0 {
        let unit = chrono_unit_named(vm, "MONTHS")?;
        let (class, method) = vm.resolve_virtual_method(
            temporal,
            "plus",
            "(JLjava/time/temporal/TemporalUnit;)Ljava/time/temporal/Temporal;",
        )?;
        result = vm.invoke_method(
            class,
            method,
            vec![result, long(total_months)?, Value::Ref(unit)],
        )?;
    }
    if days != 0 {
        let unit = chrono_unit_named(vm, "DAYS")?;
        let receiver = result.as_ref();
        let (class, method) = vm.resolve_virtual_method(
            receiver,
            "plus",
            "(JLjava/time/temporal/TemporalUnit;)Ljava/time/temporal/Temporal;",
        )?;
        result = vm.invoke_method(
            class,
            method,
            vec![result, long(days * sign)?, Value::Ref(unit)],
        )?;
    }
    Ok(result)
}

/// Parses the ISO-8601 period text (`PnYnMnD`, `PnW`, optional leading sign).
/// A time section is only accepted when every component is zero.
fn parse_period_text(text: &str) -> Option<(i32, i32, i32)> {
    let (sign, rest) = if let Some(rest) = text.strip_prefix('-') {
        (-1i32, rest)
    } else if let Some(rest) = text.strip_prefix('+') {
        (1, rest)
    } else {
        (1, text)
    };
    let rest = rest.strip_prefix('P')?;
    let (date_part, time_part) = match rest.split_once('T') {
        Some((date, time)) => (date, Some(time)),
        None => (rest, None),
    };
    if date_part.is_empty() {
        return None;
    }
    let mut years = 0i32;
    let mut months = 0i32;
    let mut days = 0i32;
    let mut saw_week = false;
    let mut saw_other = false;
    let mut number = String::new();
    for ch in date_part.chars() {
        if ch.is_ascii_digit() {
            number.push(ch);
            continue;
        }
        let value: i32 = number.parse().ok()?;
        number.clear();
        match ch {
            'Y' => {
                years = value;
                saw_other = true;
            }
            'M' => {
                months = value;
                saw_other = true;
            }
            'D' => {
                days = value;
                saw_other = true;
            }
            'W' => {
                days = value.checked_mul(7)?;
                saw_week = true;
            }
            _ => return None,
        }
    }
    if !number.is_empty() || (saw_week && saw_other) {
        return None;
    }
    if let Some(time) = time_part {
        if time.is_empty() {
            return None;
        }
        let mut number = String::new();
        for ch in time.chars() {
            if ch.is_ascii_digit() || ch == '.' {
                number.push(ch);
                continue;
            }
            if number.is_empty() || !number.chars().all(|c| c == '0' || c == '.') {
                return None;
            }
            number.clear();
            if !matches!(ch, 'H' | 'M' | 'S') {
                return None;
            }
        }
        if !number.is_empty() {
            return None;
        }
    }
    Some((sign * years, sign * months, sign * days))
}

const PERIOD_METHODS: [oxjvm_vm::NativeMethodDef; 28] = [
    method(
        "of",
        "(III)Ljava/time/Period;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(period_of(
                vm,
                int_arg(args, 0),
                int_arg(args, 1),
                int_arg(args, 2),
            )?)
        },
    ),
    method(
        "ofYears",
        "(I)Ljava/time/Period;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(period_of(vm, int_arg(args, 0), 0, 0)?),
    ),
    method(
        "ofMonths",
        "(I)Ljava/time/Period;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(period_of(vm, 0, int_arg(args, 0), 0)?),
    ),
    method(
        "ofDays",
        "(I)Ljava/time/Period;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(period_of(vm, 0, 0, int_arg(args, 0))?),
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let years = read_int_field(vm, this, "years");
        let months = read_int_field(vm, this, "months");
        let days = read_int_field(vm, this, "days");
        int(years
            .wrapping_add(months.rotate_left(8))
            .wrapping_add(days.rotate_left(16)))
    }),
    method("getYears", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "years"))
    }),
    method("getMonths", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "months"))
    }),
    method("getDays", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "days"))
    }),
    method("isZero", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        boolean(
            read_int_field(vm, this, "years") == 0
                && read_int_field(vm, this, "months") == 0
                && read_int_field(vm, this, "days") == 0,
        )
    }),
    method("isNegative", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        boolean(
            read_int_field(vm, this, "years") < 0
                || read_int_field(vm, this, "months") < 0
                || read_int_field(vm, this, "days") < 0,
        )
    }),
    method(
        "addTo",
        "(Ljava/time/temporal/Temporal;)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC,
        |vm, _, args| period_shift(vm, receiver(args), ref_arg(args, 1), false),
    ),
    method(
        "subtractFrom",
        "(Ljava/time/temporal/Temporal;)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC,
        |vm, _, args| period_shift(vm, receiver(args), ref_arg(args, 1), true),
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let years = read_int_field(vm, this, "years");
            let months = read_int_field(vm, this, "months");
            let days = read_int_field(vm, this, "days");
            let text = if years == 0 && months == 0 && days == 0 {
                String::from("P0D")
            } else {
                let mut s = String::from("P");
                if years != 0 {
                    s.push_str(&alloc::format!("{years}Y"));
                }
                if months != 0 {
                    s.push_str(&alloc::format!("{months}M"));
                }
                if days != 0 {
                    s.push_str(&alloc::format!("{days}D"));
                }
                s
            };
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let class = vm.resolve_class("java/time/Period")?;
            if !vm.is_instance(other, class) {
                return boolean(false);
            }
            boolean(
                read_int_field(vm, this, "years") == read_int_field(vm, other, "years")
                    && read_int_field(vm, this, "months") == read_int_field(vm, other, "months")
                    && read_int_field(vm, this, "days") == read_int_field(vm, other, "days"),
            )
        },
    ),
    method(
        "between",
        "(Ljava/time/LocalDate;Ljava/time/LocalDate;)Ljava/time/Period;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let start = ref_arg(args, 0);
            let end = ref_arg(args, 1);
            if start.is_null() || end.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let start_day = read_long_field(vm, start, "epochDay");
            let end_day = read_long_field(vm, end, "epochDay");
            let (sy, sm, sd) = civil_from_days(start_day);
            let (ey, em, ed) = civil_from_days(end_day);
            let mut total_months =
                i64::from(ey) * 12 + i64::from(em) - (i64::from(sy) * 12 + i64::from(sm));
            let mut days = ed - sd;
            if total_months > 0 && days < 0 {
                total_months -= 1;
                let calc_month =
                    (i64::from(sy) * 12 + i64::from(sm) - 1 + total_months).div_euclid(12);
                let calc_m = (i64::from(sy) * 12 + i64::from(sm) - 1 + total_months).rem_euclid(12)
                    as i32
                    + 1;
                let calc_day = days_from_civil(
                    calc_month as i32,
                    calc_m,
                    sd.min(length_of_month(calc_month as i32, calc_m)),
                );
                days = (end_day - calc_day) as i32;
            } else if total_months < 0 && days > 0 {
                total_months += 1;
                days -= length_of_month(ey, em);
            }
            let years = (total_months / 12) as i32;
            let months = (total_months % 12) as i32;
            object(period_of(vm, years, months, days)?)
        },
    ),
    method(
        "normalized",
        "()Ljava/time/Period;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let years = read_int_field(vm, this, "years");
            let months = read_int_field(vm, this, "months");
            let days = read_int_field(vm, this, "days");
            let total = i64::from(years) * 12 + i64::from(months);
            let new_years = (total / 12) as i32;
            let new_months = (total % 12) as i32;
            if new_years == years && new_months == months {
                return object(this);
            }
            object(period_of(vm, new_years, new_months, days)?)
        },
    ),
    method(
        "parse",
        "(Ljava/lang/CharSequence;)Ljava/time/Period;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 0));
            match parse_period_text(&text) {
                Some((years, months, days)) => object(period_of(vm, years, months, days)?),
                None => Err(vm.throw_new("java/time/format/DateTimeParseException", Some(&text))),
            }
        },
    ),
    method(
        "ofWeeks",
        "(I)Ljava/time/Period;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let weeks = int_arg(args, 0);
            let days = weeks.checked_mul(7).ok_or_else(|| {
                vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
            })?;
            object(period_new(vm, 0, 0, days)?)
        },
    ),
    method(
        "plusYears",
        "(J)Ljava/time/Period;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let amount = i32::try_from(crate::long_arg(args, 1)).map_err(|_| {
                vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
            })?;
            let years = read_int_field(vm, this, "years")
                .checked_add(amount)
                .ok_or_else(|| {
                    vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
                })?;
            object(period_new(
                vm,
                years,
                read_int_field(vm, this, "months"),
                read_int_field(vm, this, "days"),
            )?)
        },
    ),
    method(
        "plusMonths",
        "(J)Ljava/time/Period;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let amount = i32::try_from(crate::long_arg(args, 1)).map_err(|_| {
                vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
            })?;
            let months = read_int_field(vm, this, "months")
                .checked_add(amount)
                .ok_or_else(|| {
                    vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
                })?;
            object(period_new(
                vm,
                read_int_field(vm, this, "years"),
                months,
                read_int_field(vm, this, "days"),
            )?)
        },
    ),
    method(
        "plusDays",
        "(J)Ljava/time/Period;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let amount = i32::try_from(crate::long_arg(args, 1)).map_err(|_| {
                vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
            })?;
            let days = read_int_field(vm, this, "days")
                .checked_add(amount)
                .ok_or_else(|| {
                    vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
                })?;
            object(period_new(
                vm,
                read_int_field(vm, this, "years"),
                read_int_field(vm, this, "months"),
                days,
            )?)
        },
    ),
    method(
        "minusYears",
        "(J)Ljava/time/Period;",
        ACC_PUBLIC,
        |vm, _, args| period_minus(vm, receiver(args), "plusYears", crate::long_arg(args, 1)),
    ),
    method(
        "minusMonths",
        "(J)Ljava/time/Period;",
        ACC_PUBLIC,
        |vm, _, args| period_minus(vm, receiver(args), "plusMonths", crate::long_arg(args, 1)),
    ),
    method(
        "minusDays",
        "(J)Ljava/time/Period;",
        ACC_PUBLIC,
        |vm, _, args| period_minus(vm, receiver(args), "plusDays", crate::long_arg(args, 1)),
    ),
    method(
        "multipliedBy",
        "(I)Ljava/time/Period;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let factor = int_arg(args, 1);
            let years = read_int_field(vm, this, "years")
                .checked_mul(factor)
                .ok_or_else(|| {
                    vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
                })?;
            let months = read_int_field(vm, this, "months")
                .checked_mul(factor)
                .ok_or_else(|| {
                    vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
                })?;
            let days = read_int_field(vm, this, "days")
                .checked_mul(factor)
                .ok_or_else(|| {
                    vm.throw_new("java/lang/ArithmeticException", Some("integer overflow"))
                })?;
            object(period_new(vm, years, months, days)?)
        },
    ),
    method(
        "negated",
        "()Ljava/time/Period;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, method) =
                vm.resolve_virtual_method(receiver(args), "multipliedBy", "(I)Ljava/time/Period;")?;
            vm.invoke_method(class, method, vec![Value::Ref(receiver(args)), int(-1)?])
        },
    ),
    method("toTotalMonths", "()J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        long(
            i64::from(read_int_field(vm, this, "years")) * 12
                + i64::from(read_int_field(vm, this, "months")),
        )
    }),
    method(
        "plus",
        "(Ljava/time/temporal/TemporalAmount;)Ljava/time/Period;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let amount = ref_arg(args, 1);
            if amount.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let name = String::from(vm.class_name(vm.class_of(amount)));
            if name != "java/time/Period" {
                return Err(vm.throw_new(
                    "java/time/DateTimeException",
                    Some("Unable to add amount to Period"),
                ));
            }
            let years = read_int_field(vm, this, "years") + read_int_field(vm, amount, "years");
            let months = read_int_field(vm, this, "months") + read_int_field(vm, amount, "months");
            let days = read_int_field(vm, this, "days") + read_int_field(vm, amount, "days");
            object(period_new(vm, years, months, days)?)
        },
    ),
];

pub(crate) const PERIOD: oxjvm_vm::NativeClass = class(
    "java/time/Period",
    Some("java/lang/Object"),
    &["java/time/temporal/TemporalAmount", "java/io/Serializable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("years", "I", ACC_PRIVATE | ACC_FINAL, None),
        field("months", "I", ACC_PRIVATE | ACC_FINAL, None),
        field("days", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "ZERO",
            "Ljava/time/Period;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &PERIOD_METHODS,
    Some(period_clinit),
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
        boolean(index % 2 == 1)
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

/// The system default zone (`ZoneId.systemDefault()`).
pub(crate) fn system_default_zone(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/time/ZoneId")?;
    let (declaring, method) = vm
        .find_method(class, "systemDefault", "()Ljava/time/ZoneId;")
        .ok_or_else(|| VmError::internal("ZoneId.systemDefault"))?;
    Ok(vm.invoke_method(declaring, method, vec![])?.as_ref())
}

/// `LocalDateTime.ofInstant(Instant.ofEpochMilli(millis), zone)` for callers outside this module.
pub(crate) fn local_date_time_of_epoch_millis(
    vm: &mut Vm<'_>,
    millis: i64,
    zone: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let offset = i64::from(zone_offset_seconds(vm, zone));
    let epoch_second = millis.div_euclid(1000);
    let nano = millis.rem_euclid(1000) * 1_000_000;
    let local_second = epoch_second.saturating_add(offset);
    let epoch_day = local_second.div_euclid(86_400);
    let nano_of_day = local_second.rem_euclid(86_400) * 1_000_000_000 + nano;
    local_date_time_new(vm, epoch_day, nano_of_day)
}

const ZONE_RULES_METHODS: [oxjvm_vm::NativeMethodDef; 13] = [
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
    method("isFixedOffset", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method(
        "getValidOffsets",
        "(Ljava/time/LocalDateTime;)Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, args| {
            let total = read_int_field(vm, receiver(args), "totalSeconds");
            let offset = crate::lang::zone_offset_new(vm, total)?;
            object(crate::util::array_list_from(vm, &[offset])?)
        },
    ),
    method(
        "getTransitions",
        "()Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, _| {
            let empty: [ObjectRef; 0] = [];
            object(crate::util::array_list_from(vm, &empty)?)
        },
    ),
    method(
        "getTransition",
        "(Ljava/time/LocalDateTime;)Ljava/time/zone/ZoneOffsetTransition;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "nextTransition",
        "(Ljava/time/Instant;)Ljava/time/zone/ZoneOffsetTransition;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "previousTransition",
        "(Ljava/time/Instant;)Ljava/time/zone/ZoneOffsetTransition;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getTransitionRules",
        "()Ljava/util/List;",
        ACC_PUBLIC,
        |vm, _, _| {
            let empty: [ObjectRef; 0] = [];
            object(crate::util::array_list_from(vm, &empty)?)
        },
    ),
    method(
        "getDaylightSavings",
        "(Ljava/time/Instant;)Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, _| duration_new(vm, 0),
    ),
    method(
        "isDaylightSavings",
        "(Ljava/time/Instant;)Z",
        ACC_PUBLIC,
        |_, _, _| boolean(false),
    ),
    method(
        "of",
        "(Ljava/time/ZoneOffset;)Ljava/time/zone/ZoneRules;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let offset = ref_arg(args, 0);
            if offset.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let total = zone_offset_seconds(vm, offset);
            let class = vm.resolve_class("java/time/zone/ZoneRules")?;
            let instance = vm.new_instance(class)?;
            write_int_field(vm, instance, "totalSeconds", total);
            object(instance)
        },
    ),
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

/// Offsets (in seconds) of a `ZoneOffsetTransition`, before and after.
fn zot_offset_seconds(vm: &Vm<'_>, this: ObjectRef) -> (i32, i32) {
    let class = vm.class_of(this);
    let before = vm
        .read_ref_field(this, class, "offsetBefore", "Ljava/time/ZoneOffset;")
        .unwrap_or(ObjectRef::NULL);
    let after = vm
        .read_ref_field(this, class, "offsetAfter", "Ljava/time/ZoneOffset;")
        .unwrap_or(ObjectRef::NULL);
    (
        zone_offset_seconds(vm, before),
        zone_offset_seconds(vm, after),
    )
}

/// The transition `LocalDateTime` of a `ZoneOffsetTransition`.
fn zot_transition(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(
        this,
        vm.class_of(this),
        "transition",
        "Ljava/time/LocalDateTime;",
    )
    .unwrap_or(ObjectRef::NULL)
}

const ZONE_OFFSET_TRANSITION_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
    method(
        "of",
        "(Ljava/time/LocalDateTime;Ljava/time/ZoneOffset;Ljava/time/ZoneOffset;)Ljava/time/zone/ZoneOffsetTransition;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let transition = ref_arg(args, 0);
            let before = ref_arg(args, 1);
            let after = ref_arg(args, 2);
            if transition.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("transition")));
            }
            if before.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("offsetBefore")));
            }
            if after.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", Some("offsetAfter")));
            }
            let before_secs = zone_offset_seconds(vm, before);
            let after_secs = zone_offset_seconds(vm, after);
            if before_secs == after_secs {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("Offsets must not be equal"),
                ));
            }
            let epoch_day = read_long_field(vm, transition, "epochDay");
            let nano_of_day = read_long_field(vm, transition, "nanoOfDay");
            if nano_of_day % 1_000_000_000 != 0 {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("Nano-of-second must be zero"),
                ));
            }
            let epoch_second =
                epoch_day * 86_400 + nano_of_day / 1_000_000_000 - i64::from(before_secs);
            let class = vm.resolve_class("java/time/zone/ZoneOffsetTransition")?;
            let instance = vm.new_instance(class)?;
            write_long_field(vm, instance, "epochSecond", epoch_second);
            for (name, value) in [
                ("transition", transition),
                ("offsetBefore", before),
                ("offsetAfter", after),
            ] {
                if let Some((declaring, field)) = vm
                    .find_field_opt(class, name, "Ljava/time/LocalDateTime;")
                    .or_else(|| vm.find_field_opt(class, name, "Ljava/time/ZoneOffset;"))
                {
                    vm.set_instance_ref(instance, declaring, field, value);
                }
            }
            object(instance)
        },
    ),
    method(
        "getInstant",
        "()Ljava/time/Instant;",
        ACC_PUBLIC,
        |vm, _, args| instant_new(vm, read_long_field(vm, receiver(args), "epochSecond"), 0),
    ),
    method("toEpochSecond", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "epochSecond"))
    }),
    method(
        "getDateTimeBefore",
        "()Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| object(zot_transition(vm, receiver(args))),
    ),
    method(
        "getDateTimeAfter",
        "()Ljava/time/LocalDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (before, after) = zot_offset_seconds(vm, this);
            let transition = zot_transition(vm, this);
            let (class, method) = vm.resolve_virtual_method(
                transition,
                "plusSeconds",
                "(J)Ljava/time/LocalDateTime;",
            )?;
            vm.invoke_method(
                class,
                method,
                vec![
                    Value::Ref(transition),
                    Value::Long(i64::from(after - before)),
                ],
            )
        },
    ),
    method(
        "getOffsetBefore",
        "()Ljava/time/ZoneOffset;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(
                vm.read_ref_field(
                    this,
                    vm.class_of(this),
                    "offsetBefore",
                    "Ljava/time/ZoneOffset;",
                )
                .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "getOffsetAfter",
        "()Ljava/time/ZoneOffset;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(
                vm.read_ref_field(
                    this,
                    vm.class_of(this),
                    "offsetAfter",
                    "Ljava/time/ZoneOffset;",
                )
                .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "getDuration",
        "()Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (before, after) = zot_offset_seconds(vm, receiver(args));
            duration_of_seconds(vm, i64::from(after - before), 0)
        },
    ),
    method("isGap", "()Z", ACC_PUBLIC, |vm, _, args| {
        let (before, after) = zot_offset_seconds(vm, receiver(args));
        boolean(after > before)
    }),
    method("isOverlap", "()Z", ACC_PUBLIC, |vm, _, args| {
        let (before, after) = zot_offset_seconds(vm, receiver(args));
        boolean(after < before)
    }),
    method(
        "isValidOffset",
        "(Ljava/time/ZoneOffset;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let offset = ref_arg(args, 1);
            if offset.is_null() {
                return boolean(false);
            }
            let (before, after) = zot_offset_seconds(vm, receiver(args));
            let total = zone_offset_seconds(vm, offset);
            boolean(after <= before && (total == before || total == after))
        },
    ),
    method(
        "compareTo",
        "(Ljava/time/zone/ZoneOffsetTransition;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let a = read_long_field(vm, this, "epochSecond");
            let b = read_long_field(vm, other, "epochSecond");
            int(a.cmp(&b) as i32)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let class = vm.resolve_class("java/time/zone/ZoneOffsetTransition")?;
            if other.is_null() || !vm.is_instance(other, class) {
                return boolean(false);
            }
            let (b1, a1) = zot_offset_seconds(vm, this);
            let (b2, a2) = zot_offset_seconds(vm, other);
            boolean(
                read_long_field(vm, this, "epochSecond")
                    == read_long_field(vm, other, "epochSecond")
                    && b1 == b2
                    && a1 == a2,
            )
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let transition = zot_transition(vm, this);
        let (class, method) = vm.resolve_virtual_method(transition, "hashCode", "()I")?;
        let thash = vm
            .invoke_method(class, method, vec![Value::Ref(transition)])?
            .as_int();
        let (before, after) = zot_offset_seconds(vm, this);
        int(thash ^ before ^ ((after as u32).rotate_left(16) as i32))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (before, after) = zot_offset_seconds(vm, this);
            let transition = zot_transition(vm, this);
            let (class, method) =
                vm.resolve_virtual_method(transition, "toString", "()Ljava/lang/String;")?;
            let text = vm.invoke_method(class, method, vec![Value::Ref(transition)])?;
            let kind = if after > before { "Gap" } else { "Overlap" };
            let result = alloc::format!(
                "Transition[{kind} at {}{} to {}]",
                vm.string_or_empty(text.as_ref()),
                crate::lang::zone_offset_id(before),
                crate::lang::zone_offset_id(after),
            );
            object(vm.make_string(&result)?)
        },
    ),
];

/// `java.time.zone.ZoneOffsetTransition`: a gap or overlap between two offsets.
pub(crate) const ZONE_OFFSET_TRANSITION: oxjvm_vm::NativeClass = class(
    "java/time/zone/ZoneOffsetTransition",
    Some("java/lang/Object"),
    &["java/lang/Comparable", "java/io/Serializable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("epochSecond", "J", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "transition",
            "Ljava/time/LocalDateTime;",
            ACC_PRIVATE | ACC_FINAL,
            None,
        ),
        field(
            "offsetBefore",
            "Ljava/time/ZoneOffset;",
            ACC_PRIVATE | ACC_FINAL,
            None,
        ),
        field(
            "offsetAfter",
            "Ljava/time/ZoneOffset;",
            ACC_PRIVATE | ACC_FINAL,
            None,
        ),
    ],
    &ZONE_OFFSET_TRANSITION_METHODS,
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

const OFFSET_DATE_TIME_METHODS: [oxjvm_vm::NativeMethodDef; 34] = [
    method(
        "of",
        "(IIIIIIILjava/time/ZoneOffset;)Ljava/time/OffsetDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let year = int_arg(args, 0);
            let month = int_arg(args, 1);
            let day = int_arg(args, 2);
            let hour = int_arg(args, 3);
            let minute = int_arg(args, 4);
            let second = int_arg(args, 5);
            let nano = int_arg(args, 6);
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
    method(
        "of",
        "(Ljava/time/LocalDate;Ljava/time/LocalTime;Ljava/time/ZoneOffset;)Ljava/time/OffsetDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let date = ref_arg(args, 0);
            let time = ref_arg(args, 1);
            let offset = ref_arg(args, 2);
            if date.is_null() || time.is_null() || offset.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(offset_date_time_new(
                vm,
                read_long_field(vm, date, "epochDay"),
                read_long_field(vm, time, "nanoOfDay"),
                offset,
            )?)
        },
    ),
    method(
        "ofInstant",
        "(Ljava/time/Instant;Ljava/time/ZoneId;)Ljava/time/OffsetDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let instant = ref_arg(args, 0);
            let zone = ref_arg(args, 1);
            if instant.is_null() || zone.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let offset_seconds = zone_offset_seconds(vm, zone);
            let epoch_second = read_long_field(vm, instant, "epochSecond");
            let nano = read_int_field(vm, instant, "nano");
            let local_second = epoch_second.saturating_add(i64::from(offset_seconds));
            let epoch_day = local_second.div_euclid(86_400);
            let min = days_from_civil(-999_999_999, 1, 1);
            let max = days_from_civil(999_999_999, 12, 31);
            if epoch_day < min || epoch_day > max {
                return Err(vm.throw_new("java/time/DateTimeException", Some("Invalid epoch day")));
            }
            let secs_of_day = local_second.rem_euclid(86_400);
            let nano_of_day = secs_of_day * 1_000_000_000 + i64::from(nano);
            let offset = crate::lang::zone_offset_new(vm, offset_seconds)?;
            object(offset_date_time_new(vm, epoch_day, nano_of_day, offset)?)
        },
    ),
    method(
        "getMonth",
        "()Ljava/time/Month;",
        ACC_PUBLIC,
        |vm, _, args| {
            let epoch_day = read_long_field(vm, receiver(args), "epochDay");
            let (_, month, _) = civil_from_days(epoch_day);
            let class = vm.resolve_class("java/time/Month")?;
            let (declaring, method) = vm
                .find_method(class, "of", "(I)Ljava/time/Month;")
                .ok_or_else(|| VmError::internal("Month lacks of"))?;
            vm.invoke_method(declaring, method, vec![Value::Int(month)])
        },
    ),
    method(
        "plusDays",
        "(J)Ljava/time/OffsetDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let days = crate::long_arg(args, 1);
            if days == 0 {
                return object(this);
            }
            object(offset_date_time_new(
                vm,
                read_long_field(vm, this, "epochDay") + days,
                read_long_field(vm, this, "nanoOfDay"),
                offset_date_time_offset(vm, this),
            )?)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let class = vm.resolve_class("java/time/OffsetDateTime")?;
            if !vm.is_instance(other, class) {
                return boolean(false);
            }
            boolean(
                read_long_field(vm, this, "epochDay") == read_long_field(vm, other, "epochDay")
                    && read_long_field(vm, this, "nanoOfDay")
                        == read_long_field(vm, other, "nanoOfDay")
                    && zone_offset_seconds(vm, offset_date_time_offset(vm, this))
                        == zone_offset_seconds(vm, offset_date_time_offset(vm, other)),
            )
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
            let nanos = nano % 1_000_000_000;
            let time_text = if second == 0 && nanos == 0 {
                alloc::format!("{hour:02}:{minute:02}")
            } else {
                alloc::format!("{hour:02}:{minute:02}:{second:02}{}", nano_fraction(nanos))
            };
            let offset = vm
                .read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/ZoneOffset;")
                .unwrap_or(ObjectRef::NULL);
            let text = alloc::format!(
                "{year:04}-{month:02}-{day:02}T{time_text}{}",
                crate::lang::zone_offset_id(zone_offset_seconds(vm, offset))
            );
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "plusSeconds",
        "(J)Ljava/time/OffsetDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let seconds = crate::long_arg(args, 1);
            if seconds == 0 {
                return object(this);
            }
            let total = read_long_field(vm, this, "nanoOfDay") + seconds * 1_000_000_000;
            let offset = vm
                .read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/ZoneOffset;")
                .unwrap_or(ObjectRef::NULL);
            object(offset_date_time_new(
                vm,
                read_long_field(vm, this, "epochDay") + total.div_euclid(NANOS_PER_DAY),
                total.rem_euclid(NANOS_PER_DAY),
                offset,
            )?)
        },
    ),
    method("getDayOfYear", "()I", ACC_PUBLIC, |vm, _, args| {
        let epoch_day = read_long_field(vm, receiver(args), "epochDay");
        let (year, _, _) = civil_from_days(epoch_day);
        int((epoch_day - days_from_civil(year, 1, 1) + 1) as i32)
    }),
    method(
        "compareTo",
        "(Ljava/time/OffsetDateTime;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let this_offset = i64::from(zone_offset_seconds(
                vm,
                vm.read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/ZoneOffset;")
                    .unwrap_or(ObjectRef::NULL),
            ));
            let other_offset = i64::from(zone_offset_seconds(
                vm,
                vm.read_ref_field(
                    other,
                    vm.class_of(other),
                    "offset",
                    "Ljava/time/ZoneOffset;",
                )
                .unwrap_or(ObjectRef::NULL),
            ));
            let this_second = read_long_field(vm, this, "epochDay") * 86_400
                + read_long_field(vm, this, "nanoOfDay") / 1_000_000_000
                - this_offset;
            let other_second = read_long_field(vm, other, "epochDay") * 86_400
                + read_long_field(vm, other, "nanoOfDay") / 1_000_000_000
                - other_offset;
            if this_second != other_second {
                return int(this_second.cmp(&other_second) as i32);
            }
            let this_nano = read_long_field(vm, this, "nanoOfDay") % 1_000_000_000;
            let other_nano = read_long_field(vm, other, "nanoOfDay") % 1_000_000_000;
            if this_nano != other_nano {
                return int(this_nano.cmp(&other_nano) as i32);
            }
            let this_local = read_long_field(vm, this, "epochDay") * NANOS_PER_DAY
                + read_long_field(vm, this, "nanoOfDay");
            let other_local = read_long_field(vm, other, "epochDay") * NANOS_PER_DAY
                + read_long_field(vm, other, "nanoOfDay");
            int(this_local.cmp(&other_local) as i32)
        },
    ),
    method(
        "parse",
        "(Ljava/lang/CharSequence;)Ljava/time/OffsetDateTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 0));
            let parse_error = |vm: &mut Vm<'_>| {
                vm.throw_new("java/time/format/DateTimeParseException", Some(&text))
            };
            let (date_text, time_text) = text.split_once('T').ok_or_else(|| parse_error(vm))?;
            let parts: Vec<&str> = date_text.split('-').collect();
            if parts.len() != 3 {
                return Err(parse_error(vm));
            }
            let year = parts[0].parse::<i32>().unwrap_or(0);
            let month = parts[1].parse::<i32>().unwrap_or(0);
            let day = parts[2].parse::<i32>().unwrap_or(0);
            let (clock_text, offset_total) = if let Some(stripped) = time_text.strip_suffix('Z') {
                (stripped, 0)
            } else {
                let sign_pos = time_text.rfind(['+', '-']).ok_or_else(|| parse_error(vm))?;
                let total = crate::lang::zone_offset_parse(&time_text[sign_pos..])
                    .ok_or_else(|| parse_error(vm))?;
                (&time_text[..sign_pos], total)
            };
            let tparts: Vec<&str> = clock_text.split(':').collect();
            if tparts.len() < 2 {
                return Err(parse_error(vm));
            }
            let hour = tparts[0].parse::<i64>().unwrap_or(0);
            let minute = tparts[1].parse::<i64>().unwrap_or(0);
            let mut second = 0i64;
            let mut nano = 0i64;
            if let Some(sec_text) = tparts.get(2) {
                if let Some((s, frac)) = sec_text.split_once('.') {
                    second = s.parse().unwrap_or(0);
                    let mut frac_text = String::from(frac);
                    while frac_text.len() < 9 {
                        frac_text.push('0');
                    }
                    nano = frac_text[..9].parse().unwrap_or(0);
                } else {
                    second = sec_text.parse().unwrap_or(0);
                }
            }
            let nano_of_day = (hour * 3600 + minute * 60 + second) * 1_000_000_000 + nano;
            let offset = crate::lang::zone_offset_new(vm, offset_total)?;
            object(offset_date_time_new(
                vm,
                days_from_civil(year, month, day),
                nano_of_day,
                offset,
            )?)
        },
    ),
    method(
        "with",
        "(Ljava/time/temporal/TemporalAdjuster;)Ljava/time/OffsetDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let adjuster = ref_arg(args, 1);
            let (class, method) = vm.resolve_virtual_method(
                adjuster,
                "adjustInto",
                "(Ljava/time/temporal/Temporal;)Ljava/time/temporal/Temporal;",
            )?;
            vm.invoke_method(class, method, vec![Value::Ref(adjuster), Value::Ref(this)])
        },
    ),
    method(
        "getLong",
        "(Ljava/time/temporal/TemporalField;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let field = ref_arg(args, 1);
            let epoch_day = read_long_field(vm, this, "epochDay");
            let nano_of_day = read_long_field(vm, this, "nanoOfDay");
            match chrono_field_ordinal(vm, field) {
                Some(ordinal) => {
                    if let Some(value) = time_field_long(nano_of_day, ordinal) {
                        return long(value);
                    }
                    if let Some(value) = date_field_long(epoch_day, ordinal) {
                        return long(value);
                    }
                    let offset = vm
                        .read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/ZoneOffset;")
                        .unwrap_or(ObjectRef::NULL);
                    if ordinal == 28 {
                        return long(
                            epoch_day * 86_400 + nano_of_day / 1_000_000_000
                                - i64::from(zone_offset_seconds(vm, offset)),
                        );
                    }
                    if ordinal == 29 {
                        return long(i64::from(zone_offset_seconds(vm, offset)));
                    }
                    Err(unsupported_field(vm))
                }
                None => temporal_field_get_from(vm, field, this),
            }
        },
    ),
    method(
        "get",
        "(Ljava/time/temporal/TemporalField;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (class, method) = vm.resolve_virtual_method(
                this,
                "getLong",
                "(Ljava/time/temporal/TemporalField;)J",
            )?;
            let value = vm
                .invoke_method(
                    class,
                    method,
                    vec![Value::Ref(this), Value::Ref(ref_arg(args, 1))],
                )?
                .as_long();
            if value < i64::from(i32::MIN) || value > i64::from(i32::MAX) {
                return Err(vm.throw_new(
                    "java/time/DateTimeException",
                    Some("field value out of range"),
                ));
            }
            int(value as i32)
        },
    ),
    method(
        "isSupported",
        "(Ljava/time/temporal/TemporalField;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let field = ref_arg(args, 1);
            match chrono_field_ordinal(vm, field) {
                Some(ordinal) => boolean(matches!(ordinal, 0..=29)),
                None => {
                    let (class, method) = vm.resolve_virtual_method(
                        field,
                        "isSupportedBy",
                        "(Ljava/time/temporal/TemporalAccessor;)Z",
                    )?;
                    vm.invoke_method(
                        class,
                        method,
                        vec![Value::Ref(field), Value::Ref(receiver(args))],
                    )
                }
            }
        },
    ),
    method(
        "query",
        "(Ljava/time/temporal/TemporalQuery;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let query = ref_arg(args, 1);
            if query.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            crate::temporal::temporal_query_from(vm, query, receiver(args))
        },
    ),
    method(
        "withOffsetSameInstant",
        "(Ljava/time/ZoneOffset;)Ljava/time/OffsetDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let new_offset = ref_arg(args, 1);
            if new_offset.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let old_offset = vm
                .read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/ZoneOffset;")
                .unwrap_or(ObjectRef::NULL);
            let shift = i64::from(
                zone_offset_seconds(vm, new_offset) - zone_offset_seconds(vm, old_offset),
            );
            let total = read_long_field(vm, this, "epochDay") * 86_400_000_000_000
                + read_long_field(vm, this, "nanoOfDay")
                + shift * 1_000_000_000;
            object(offset_date_time_new(
                vm,
                total.div_euclid(86_400_000_000_000),
                total.rem_euclid(86_400_000_000_000),
                new_offset,
            )?)
        },
    ),
    method(
        "withOffsetSameLocal",
        "(Ljava/time/ZoneOffset;)Ljava/time/OffsetDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let new_offset = ref_arg(args, 1);
            if new_offset.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(offset_date_time_new(
                vm,
                read_long_field(vm, this, "epochDay"),
                read_long_field(vm, this, "nanoOfDay"),
                new_offset,
            )?)
        },
    ),
    method(
        "toZonedDateTime",
        "()Ljava/time/ZonedDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let offset = vm
                .read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/ZoneOffset;")
                .unwrap_or(ObjectRef::NULL);
            let offset_seconds = zone_offset_seconds(vm, offset);
            let nano_of_day = read_long_field(vm, this, "nanoOfDay");
            let local_second =
                read_long_field(vm, this, "epochDay") * 86_400 + nano_of_day / 1_000_000_000;
            let nano = (nano_of_day % 1_000_000_000) as i32;
            zoned_date_time_new(
                vm,
                (local_second - i64::from(offset_seconds)) * 1000 + i64::from(nano) / 1_000_000,
                nano,
                offset,
            )
        },
    ),
    method(
        "atZoneSameInstant",
        "(Ljava/time/ZoneId;)Ljava/time/ZonedDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let zone = ref_arg(args, 1);
            if zone.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let epoch_second = offset_date_time_epoch_second(vm, this);
            let nano_of_day = read_long_field(vm, this, "nanoOfDay");
            let nano = (nano_of_day % 1_000_000_000) as i32;
            zoned_date_time_new(
                vm,
                epoch_second * 1000 + i64::from(nano) / 1_000_000,
                nano,
                zone,
            )
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let offset = vm
            .read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/ZoneOffset;")
            .unwrap_or(ObjectRef::NULL);
        int(local_date_hash(read_long_field(vm, this, "epochDay"))
            ^ local_time_hash(read_long_field(vm, this, "nanoOfDay"))
            ^ zone_offset_seconds(vm, offset))
    }),
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

// -------------------------------------------------------------------------------------------
// java.time.Clock
// -------------------------------------------------------------------------------------------

/// The instant exposed by a `Clock`: a fixed instant, or the host time, plus
/// any accumulated offset from `Clock.offset`.
fn clock_instant_parts(vm: &mut Vm<'_>, this: ObjectRef) -> (i64, i32) {
    let fixed = read_int_field(vm, this, "fixed");
    let offset = read_long_field(vm, this, "offsetNanos");
    let (second, nano) = if fixed != 0 {
        (
            read_long_field(vm, this, "epochSecond"),
            read_int_field(vm, this, "nano"),
        )
    } else {
        let millis = vm.host_time_millis();
        (
            millis.div_euclid(1000),
            (millis.rem_euclid(1000) * 1_000_000) as i32,
        )
    };
    let total = i64::from(nano) + offset;
    (
        second.saturating_add(total.div_euclid(1_000_000_000)),
        total.rem_euclid(1_000_000_000) as i32,
    )
}

fn clock_new(
    vm: &mut Vm<'_>,
    zone: ObjectRef,
    fixed: bool,
    epoch_second: i64,
    nano: i32,
    offset_nanos: i64,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/time/Clock")?;
    let instance = vm.new_instance(class)?;
    if let Some((declaring, field)) = vm.find_field_opt(class, "zone", "Ljava/time/ZoneId;") {
        vm.set_instance_ref(instance, declaring, field, zone);
    }
    write_int_field(vm, instance, "fixed", i32::from(fixed));
    write_int_field(vm, instance, "kind", i32::from(fixed));
    write_long_field(vm, instance, "epochSecond", epoch_second);
    write_int_field(vm, instance, "nano", nano);
    write_long_field(vm, instance, "offsetNanos", offset_nanos);
    Ok(instance)
}

/// Render any object's `toString()` as a Rust string (for `Clock.toString`).
fn clock_obj_string(vm: &mut Vm<'_>, obj: ObjectRef) -> Result<String, VmError> {
    let (class, method) = vm.resolve_virtual_method(obj, "toString", "()Ljava/lang/String;")?;
    let result = vm.invoke_method(class, method, vec![Value::Ref(obj)])?;
    Ok(vm.string_value(result.as_ref()).unwrap_or_default())
}

/// The id of a clock's zone (e.g. `Z` or `Europe/Paris`).
fn clock_zone_id(vm: &mut Vm<'_>, zone: ObjectRef) -> Result<String, VmError> {
    let (class, method) = vm.resolve_virtual_method(zone, "getId", "()Ljava/lang/String;")?;
    let result = vm.invoke_method(class, method, vec![Value::Ref(zone)])?;
    Ok(vm.string_value(result.as_ref()).unwrap_or_default())
}

/// The stored base clock of an offset/tick clock (null when absent).
fn clock_base_ref(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.read_ref_field(this, vm.class_of(this), "base", "Ljava/time/Clock;")
        .unwrap_or(ObjectRef::NULL)
}

/// Build a `TickClock` wrapping a base clock with a nanosecond tick granularity.
fn clock_tick_new(vm: &mut Vm<'_>, base: ObjectRef, tick_nanos: i64) -> Result<ObjectRef, VmError> {
    let zone = clock_zone(vm, base)?;
    let instance = clock_new(
        vm,
        zone,
        read_int_field(vm, base, "fixed") != 0,
        read_long_field(vm, base, "epochSecond"),
        read_int_field(vm, base, "nano"),
        read_long_field(vm, base, "offsetNanos"),
    )?;
    write_int_field(vm, instance, "kind", 3);
    write_long_field(vm, instance, "tickNanos", tick_nanos);
    let class = vm.class_of(instance);
    if let Some((declaring, field)) = vm.find_field_opt(class, "base", "Ljava/time/Clock;") {
        vm.set_instance_ref(instance, declaring, field, base);
    }
    Ok(instance)
}

/// The instant exposed by a `TickClock`: the base instant truncated to the tick
/// granularity (whole milliseconds when the tick divides evenly into a ms).
fn clock_tick_instant(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let base = clock_base_ref(vm, this);
    let tick_nanos = read_long_field(vm, this, "tickNanos");
    if tick_nanos % 1_000_000 == 0 {
        let (bc, bm) = vm.resolve_virtual_method(base, "millis", "()J")?;
        let base_millis = vm.invoke_method(bc, bm, vec![Value::Ref(base)])?.as_long();
        let tick_millis = tick_nanos / 1_000_000;
        let millis = base_millis - base_millis.rem_euclid(tick_millis);
        return instant_new(
            vm,
            millis.div_euclid(1000),
            (millis.rem_euclid(1000) * 1_000_000) as i32,
        );
    }
    let (bc, bm) = vm.resolve_virtual_method(base, "instant", "()Ljava/time/Instant;")?;
    let base_instant = vm.invoke_method(bc, bm, vec![Value::Ref(base)])?.as_ref();
    let epoch_second = read_long_field(vm, base_instant, "epochSecond");
    let nano = i64::from(read_int_field(vm, base_instant, "nano"));
    let adjust = nano.rem_euclid(tick_nanos);
    instant_new(vm, epoch_second, (nano - adjust) as i32)
}

/// The zone of a clock argument, falling back to UTC when null.
fn clock_zone(vm: &mut Vm<'_>, clock: ObjectRef) -> Result<ObjectRef, VmError> {
    let zone = vm
        .read_ref_field(clock, vm.class_of(clock), "zone", "Ljava/time/ZoneId;")
        .unwrap_or(ObjectRef::NULL);
    if zone.is_null() {
        let class = vm.resolve_class("java/time/ZoneOffset")?;
        return Ok(vm
            .read_static_ref_if_set(class, "UTC", "Ljava/time/ZoneOffset;")
            .unwrap_or(ObjectRef::NULL));
    }
    Ok(zone)
}

/// The local date-time parts (epoch day, nano of day) of a clock's instant
/// resolved in the clock's zone.
fn clock_local_parts(vm: &mut Vm<'_>, clock: ObjectRef) -> Result<(i64, i64), VmError> {
    let (second, nano) = clock_instant_parts(vm, clock);
    let zone = clock_zone(vm, clock)?;
    let local_second = second + i64::from(zone_offset_seconds(vm, zone));
    let epoch_day = local_second.div_euclid(86_400);
    let nano_of_day = local_second.rem_euclid(86_400) * 1_000_000_000 + i64::from(nano);
    Ok((epoch_day, nano_of_day))
}

const CLOCK_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
    method(
        "systemUTC",
        "()Ljava/time/Clock;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/time/ZoneOffset")?;
            vm.ensure_initialized(class)?;
            let utc = vm
                .read_static_ref_if_set(class, "UTC", "Ljava/time/ZoneOffset;")
                .unwrap_or(ObjectRef::NULL);
            object(clock_new(vm, utc, false, 0, 0, 0)?)
        },
    ),
    method(
        "systemDefaultZone",
        "()Ljava/time/Clock;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/time/ZoneId")?;
            let (declaring, method) = vm
                .find_method(class, "systemDefault", "()Ljava/time/ZoneId;")
                .ok_or_else(|| VmError::internal("ZoneId.systemDefault"))?;
            let zone = vm.invoke_method(declaring, method, vec![])?.as_ref();
            object(clock_new(vm, zone, false, 0, 0, 0)?)
        },
    ),
    method(
        "system",
        "(Ljava/time/ZoneId;)Ljava/time/Clock;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(clock_new(vm, ref_arg(args, 0), false, 0, 0, 0)?),
    ),
    method(
        "fixed",
        "(Ljava/time/Instant;Ljava/time/ZoneId;)Ljava/time/Clock;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let instant = ref_arg(args, 0);
            let zone = ref_arg(args, 1);
            object(clock_new(
                vm,
                zone,
                true,
                read_long_field(vm, instant, "epochSecond"),
                read_int_field(vm, instant, "nano"),
                0,
            )?)
        },
    ),
    method(
        "offset",
        "(Ljava/time/Clock;Ljava/time/Duration;)Ljava/time/Clock;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let base = ref_arg(args, 0);
            let duration = ref_arg(args, 1);
            let added = read_long_field(vm, duration, "nanos");
            let zone = clock_zone(vm, base)?;
            let instance = clock_new(
                vm,
                zone,
                read_int_field(vm, base, "fixed") != 0,
                read_long_field(vm, base, "epochSecond"),
                read_int_field(vm, base, "nano"),
                read_long_field(vm, base, "offsetNanos").saturating_add(added),
            )?;
            write_int_field(vm, instance, "kind", 2);
            let class = vm.class_of(instance);
            if let Some((declaring, field)) = vm.find_field_opt(class, "base", "Ljava/time/Clock;")
            {
                vm.set_instance_ref(instance, declaring, field, base);
            }
            if let Some((declaring, field)) =
                vm.find_field_opt(class, "offset", "Ljava/time/Duration;")
            {
                vm.set_instance_ref(instance, declaring, field, duration);
            }
            object(instance)
        },
    ),
    method(
        "tick",
        "(Ljava/time/Clock;Ljava/time/Duration;)Ljava/time/Clock;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let base = ref_arg(args, 0);
            let duration = ref_arg(args, 1);
            if base.is_null() || duration.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let tick_nanos = read_long_field(vm, duration, "nanos");
            if tick_nanos < 0 {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("Tick duration must not be negative"),
                ));
            }
            if tick_nanos % 1_000_000 != 0 && (tick_nanos == 0 || 1_000_000_000 % tick_nanos != 0) {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("Invalid tick duration"),
                ));
            }
            if tick_nanos <= 1 {
                return object(base);
            }
            object(clock_tick_new(vm, base, tick_nanos)?)
        },
    ),
    method(
        "instant",
        "()Ljava/time/Instant;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if read_int_field(vm, this, "kind") == 3 {
                return clock_tick_instant(vm, this);
            }
            let (second, nano) = clock_instant_parts(vm, this);
            instant_new(vm, second, nano)
        },
    ),
    method("millis", "()J", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        if read_int_field(vm, this, "kind") == 3 {
            let base = clock_base_ref(vm, this);
            let tick_nanos = read_long_field(vm, this, "tickNanos");
            let (bc, bm) = vm.resolve_virtual_method(base, "millis", "()J")?;
            let base_millis = vm.invoke_method(bc, bm, vec![Value::Ref(base)])?.as_long();
            if tick_nanos < 1_000_000 {
                return long(base_millis);
            }
            let tick_millis = tick_nanos / 1_000_000;
            return long(base_millis - base_millis.rem_euclid(tick_millis));
        }
        let (second, nano) = clock_instant_parts(vm, this);
        long(second.saturating_mul(1000) + i64::from(nano) / 1_000_000)
    }),
    method(
        "getZone",
        "()Ljava/time/ZoneId;",
        ACC_PUBLIC,
        |vm, _, args| object(clock_zone(vm, receiver(args))?),
    ),
    method(
        "withZone",
        "(Ljava/time/ZoneId;)Ljava/time/Clock;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let new_zone = ref_arg(args, 1);
            let current_zone = clock_zone(vm, this)?;
            let (class, method) =
                vm.resolve_virtual_method(current_zone, "equals", "(Ljava/lang/Object;)Z")?;
            let same = vm.invoke_method(
                class,
                method,
                vec![Value::Ref(current_zone), Value::Ref(new_zone)],
            )?;
            if matches!(same, Value::Int(1)) {
                return object(this);
            }
            let kind = read_int_field(vm, this, "kind");
            if kind == 3 {
                let base = clock_base_ref(vm, this);
                let tick_nanos = read_long_field(vm, this, "tickNanos");
                let (bc, bm) = vm.resolve_virtual_method(
                    base,
                    "withZone",
                    "(Ljava/time/ZoneId;)Ljava/time/Clock;",
                )?;
                let new_base = vm
                    .invoke_method(bc, bm, vec![Value::Ref(base), Value::Ref(new_zone)])?
                    .as_ref();
                return object(clock_tick_new(vm, new_base, tick_nanos)?);
            }
            if kind == 2 {
                let base = vm
                    .read_ref_field(this, vm.class_of(this), "base", "Ljava/time/Clock;")
                    .unwrap_or(ObjectRef::NULL);
                let offset = vm
                    .read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/Duration;")
                    .unwrap_or(ObjectRef::NULL);
                let (bc, bm) = vm.resolve_virtual_method(
                    base,
                    "withZone",
                    "(Ljava/time/ZoneId;)Ljava/time/Clock;",
                )?;
                let new_base = vm
                    .invoke_method(bc, bm, vec![Value::Ref(base), Value::Ref(new_zone)])?
                    .as_ref();
                let clock_class = vm.resolve_class("java/time/Clock")?;
                let (oc, om) = vm
                    .find_method(
                        clock_class,
                        "offset",
                        "(Ljava/time/Clock;Ljava/time/Duration;)Ljava/time/Clock;",
                    )
                    .ok_or_else(|| VmError::internal("Clock.offset"))?;
                return vm.invoke_method(oc, om, vec![Value::Ref(new_base), Value::Ref(offset)]);
            }
            object(clock_new(
                vm,
                new_zone,
                kind == 1,
                read_long_field(vm, this, "epochSecond"),
                read_int_field(vm, this, "nano"),
                read_long_field(vm, this, "offsetNanos"),
            )?)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let kind = read_int_field(vm, this, "kind");
            let zone = clock_zone(vm, this)?;
            let zone_id = clock_zone_id(vm, zone)?;
            let text = match kind {
                1 => {
                    let instant = instant_new(
                        vm,
                        read_long_field(vm, this, "epochSecond"),
                        read_int_field(vm, this, "nano"),
                    )?;
                    let ins = clock_obj_string(vm, instant.as_ref())?;
                    alloc::format!("FixedClock[{ins},{zone_id}]")
                }
                2 => {
                    let class = vm.class_of(this);
                    let base = vm
                        .read_ref_field(this, class, "base", "Ljava/time/Clock;")
                        .unwrap_or(ObjectRef::NULL);
                    let offset = vm
                        .read_ref_field(this, class, "offset", "Ljava/time/Duration;")
                        .unwrap_or(ObjectRef::NULL);
                    let base_str = clock_obj_string(vm, base)?;
                    let off_str = clock_obj_string(vm, offset)?;
                    alloc::format!("OffsetClock[{base_str},{off_str}]")
                }
                3 => {
                    let base = clock_base_ref(vm, this);
                    let tick_nanos = read_long_field(vm, this, "tickNanos");
                    let base_str = clock_obj_string(vm, base)?;
                    let duration = duration_new(vm, tick_nanos)?;
                    let dur_str = clock_obj_string(vm, duration.as_ref())?;
                    alloc::format!("TickClock[{base_str},{dur_str}]")
                }
                _ => alloc::format!("SystemClock[{zone_id}]"),
            };
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
                return boolean(false);
            }
            let class = vm.resolve_class("java/time/Clock")?;
            if !vm.is_instance(other, class) {
                return boolean(false);
            }
            let this = receiver(args);
            let (s1, n1) = clock_instant_parts(vm, this);
            let (s2, n2) = clock_instant_parts(vm, other);
            boolean(s1 == s2 && n1 == n2)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let (second, nano) = clock_instant_parts(vm, receiver(args));
        int((second ^ (second >> 32)) as i32 ^ nano)
    }),
    method(
        "tickSeconds",
        "(Ljava/time/ZoneId;)Ljava/time/Clock;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let zone = ref_arg(args, 0);
            if zone.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let base = clock_new(vm, zone, false, 0, 0, 0)?;
            object(clock_tick_new(vm, base, 1_000_000_000)?)
        },
    ),
    method(
        "tickMinutes",
        "(Ljava/time/ZoneId;)Ljava/time/Clock;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let zone = ref_arg(args, 0);
            if zone.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let base = clock_new(vm, zone, false, 0, 0, 0)?;
            object(clock_tick_new(vm, base, 60_000_000_000)?)
        },
    ),
];

/// `java.time.Clock`: a source of instants, either the host time or a fixed
/// instant, with an optional accumulated offset.
pub(crate) const CLOCK: oxjvm_vm::NativeClass = class(
    "java/time/Clock",
    Some("java/lang/Object"),
    &["java/io/Serializable", "java/time/InstantSource"],
    ACC_PUBLIC,
    &[
        field("zone", "Ljava/time/ZoneId;", ACC_PRIVATE, None),
        field("fixed", "I", ACC_PRIVATE, None),
        field("kind", "I", ACC_PRIVATE, None),
        field("epochSecond", "J", ACC_PRIVATE, None),
        field("nano", "I", ACC_PRIVATE, None),
        field("offsetNanos", "J", ACC_PRIVATE, None),
        field("base", "Ljava/time/Clock;", ACC_PRIVATE, None),
        field("offset", "Ljava/time/Duration;", ACC_PRIVATE, None),
        field("tickNanos", "J", ACC_PRIVATE, None),
    ],
    &CLOCK_METHODS,
    None,
);

const INSTANT_SOURCE_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "system",
        "()Ljava/time/InstantSource;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/time/Clock")?;
            let (declaring, method) = vm
                .find_method(class, "systemUTC", "()Ljava/time/Clock;")
                .ok_or_else(|| VmError::internal("Clock.systemUTC missing"))?;
            vm.invoke_method(declaring, method, vec![])
        },
    ),
    method(
        "fixed",
        "(Ljava/time/Instant;)Ljava/time/InstantSource;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let instant = ref_arg(args, 0);
            let utc = zone_offset_from_seconds(vm, 0)?;
            let class = vm.resolve_class("java/time/Clock")?;
            let (declaring, method) = vm
                .find_method(
                    class,
                    "fixed",
                    "(Ljava/time/Instant;Ljava/time/ZoneId;)Ljava/time/Clock;",
                )
                .ok_or_else(|| VmError::internal("Clock.fixed missing"))?;
            vm.invoke_method(
                declaring,
                method,
                vec![Value::Ref(instant), Value::Ref(utc)],
            )
        },
    ),
    method(
        "instant",
        "()Ljava/time/Instant;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const INSTANT_SOURCE: oxjvm_vm::NativeClass = class(
    "java/time/InstantSource",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &INSTANT_SOURCE_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// YearMonth / MonthDay
// -------------------------------------------------------------------------------------------

fn year_month_new(vm: &mut Vm<'_>, year: i32, month: i32) -> Result<ObjectRef, VmError> {
    if !(1..=12).contains(&month) {
        return Err(vm.throw_new("java/time/DateTimeException", Some("Invalid month")));
    }
    let class = vm.resolve_class("java/time/YearMonth")?;
    let instance = vm.new_instance(class)?;
    write_int_field(vm, instance, "year", year);
    write_int_field(vm, instance, "month", month);
    Ok(instance)
}

fn year_month_parse(vm: &mut Vm<'_>, text: &str) -> Result<Value, VmError> {
    let error =
        |vm: &mut Vm<'_>| vm.throw_new("java/time/format/DateTimeParseException", Some(text));
    let parts: Vec<&str> = text.split('-').collect();
    if parts.len() != 2 {
        return Err(error(vm));
    }
    let year: i32 = parts[0].parse().map_err(|_| error(vm))?;
    let month: i32 = parts[1].parse().map_err(|_| error(vm))?;
    object(year_month_new(vm, year, month)?)
}

const YEAR_MONTH_METHODS: [oxjvm_vm::NativeMethodDef; 25] = [
    method(
        "of",
        "(II)Ljava/time/YearMonth;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(year_month_new(vm, int_arg(args, 0), int_arg(args, 1))?),
    ),
    method(
        "now",
        "()Ljava/time/YearMonth;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let millis = vm.host_time_millis();
            let zone = system_default_zone(vm)?;
            let offset = i64::from(zone_offset_seconds(vm, zone));
            let (year, month, _) = civil_from_days((millis / 1000 + offset).div_euclid(86_400));
            object(year_month_new(vm, year, month)?)
        },
    ),
    method(
        "parse",
        "(Ljava/lang/CharSequence;)Ljava/time/YearMonth;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = crate::string_arg(vm, args[0])?;
            year_month_parse(vm, &text)
        },
    ),
    method("getYear", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "year"))
    }),
    method("getMonthValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "month"))
    }),
    method("lengthOfMonth", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        int(length_of_month(
            read_int_field(vm, this, "year"),
            read_int_field(vm, this, "month"),
        ))
    }),
    method("lengthOfYear", "()I", ACC_PUBLIC, |vm, _, args| {
        int(
            if is_leap_year(read_int_field(vm, receiver(args), "year")) {
                366
            } else {
                365
            },
        )
    }),
    method("isLeapYear", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(is_leap_year(read_int_field(vm, receiver(args), "year")))
    }),
    method(
        "atDay",
        "(I)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let year = read_int_field(vm, this, "year");
            let month = read_int_field(vm, this, "month");
            let day = int_arg(args, 1);
            if day < 1 || day > length_of_month(year, month) {
                return Err(vm.throw_new("java/time/DateTimeException", Some("Invalid day")));
            }
            object(local_date_new(vm, days_from_civil(year, month, day))?)
        },
    ),
    method(
        "atEndOfMonth",
        "()Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let year = read_int_field(vm, this, "year");
            let month = read_int_field(vm, this, "month");
            object(local_date_new(
                vm,
                days_from_civil(year, month, length_of_month(year, month)),
            )?)
        },
    ),
    method(
        "plusMonths",
        "(J)Ljava/time/YearMonth;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let total = i64::from(read_int_field(vm, this, "year")) * 12
                + i64::from(read_int_field(vm, this, "month"))
                - 1
                + crate::long_arg(args, 1);
            object(year_month_new(
                vm,
                total.div_euclid(12) as i32,
                total.rem_euclid(12) as i32 + 1,
            )?)
        },
    ),
    method(
        "minusMonths",
        "(J)Ljava/time/YearMonth;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let total = i64::from(read_int_field(vm, this, "year")) * 12
                + i64::from(read_int_field(vm, this, "month"))
                - 1
                - crate::long_arg(args, 1);
            object(year_month_new(
                vm,
                total.div_euclid(12) as i32,
                total.rem_euclid(12) as i32 + 1,
            )?)
        },
    ),
    method(
        "plusYears",
        "(J)Ljava/time/YearMonth;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(year_month_new(
                vm,
                (i64::from(read_int_field(vm, this, "year")) + crate::long_arg(args, 1)) as i32,
                read_int_field(vm, this, "month"),
            )?)
        },
    ),
    method(
        "minusYears",
        "(J)Ljava/time/YearMonth;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(year_month_new(
                vm,
                (i64::from(read_int_field(vm, this, "year")) - crate::long_arg(args, 1)) as i32,
                read_int_field(vm, this, "month"),
            )?)
        },
    ),
    method(
        "withYear",
        "(I)Ljava/time/YearMonth;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(year_month_new(
                vm,
                int_arg(args, 1),
                read_int_field(vm, this, "month"),
            )?)
        },
    ),
    method(
        "withMonth",
        "(I)Ljava/time/YearMonth;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(year_month_new(
                vm,
                read_int_field(vm, this, "year"),
                int_arg(args, 1),
            )?)
        },
    ),
    method(
        "compareTo",
        "(Ljava/time/YearMonth;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let a = read_int_field(vm, this, "year") * 12 + read_int_field(vm, this, "month");
            let b = read_int_field(vm, other, "year") * 12 + read_int_field(vm, other, "month");
            int(a.cmp(&b) as i32)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(other) != vm.class_of(this) {
                return boolean(false);
            }
            boolean(
                read_int_field(vm, this, "year") == read_int_field(vm, other, "year")
                    && read_int_field(vm, this, "month") == read_int_field(vm, other, "month"),
            )
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = alloc::format!(
                "{:04}-{:02}",
                read_int_field(vm, this, "year"),
                read_int_field(vm, this, "month")
            );
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "isBefore",
        "(Ljava/time/YearMonth;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            boolean(
                (
                    read_int_field(vm, this, "year"),
                    read_int_field(vm, this, "month"),
                ) < (
                    read_int_field(vm, other, "year"),
                    read_int_field(vm, other, "month"),
                ),
            )
        },
    ),
    method(
        "isAfter",
        "(Ljava/time/YearMonth;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            boolean(
                (
                    read_int_field(vm, this, "year"),
                    read_int_field(vm, this, "month"),
                ) > (
                    read_int_field(vm, other, "year"),
                    read_int_field(vm, other, "month"),
                ),
            )
        },
    ),
    method(
        "isSupported",
        "(Ljava/time/temporal/TemporalField;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let ordinal = chrono_field_ordinal(vm, ref_arg(args, 1));
            boolean(matches!(ordinal, Some(23..=27)))
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
            let year = i64::from(read_int_field(vm, this, "year"));
            let month = i64::from(read_int_field(vm, this, "month"));
            let value = match ordinal {
                23 => month,
                24 => year * 12 + month - 1,
                26 => year,
                25 => {
                    if year >= 1 {
                        year
                    } else {
                        1 - year
                    }
                }
                27 => i64::from(year >= 1),
                _ => return Err(unsupported_field(vm)),
            };
            long(value)
        },
    ),
    method(
        "range",
        "(Ljava/time/temporal/TemporalField;)Ljava/time/temporal/ValueRange;",
        ACC_PUBLIC,
        |vm, _, args| {
            let ordinal =
                chrono_field_ordinal(vm, ref_arg(args, 1)).ok_or_else(|| unsupported_field(vm))?;
            let (min_low, min_high, max_low, max_high) = match ordinal {
                23 => (1, 1, 12, 12),
                24 => (
                    -11_999_999_988,
                    -11_999_999_988,
                    11_999_999_999,
                    11_999_999_999,
                ),
                26 => (-999_999_999, -999_999_999, 999_999_999, 999_999_999),
                25 => (1, 1, 999_999_999, 999_999_999),
                27 => (0, 0, 1, 1),
                _ => return Err(unsupported_field(vm)),
            };
            object(crate::temporal::value_range_new(
                vm, min_low, min_high, max_low, max_high,
            )?)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        int(read_int_field(vm, this, "year") ^ (read_int_field(vm, this, "month") << 27))
    }),
];

fn year_new(vm: &mut Vm<'_>, year: i32) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/time/Year")?;
    let instance = vm.new_instance(class)?;
    write_int_field(vm, instance, "value", year);
    Ok(instance)
}

const YEAR_METHODS: [oxjvm_vm::NativeMethodDef; 21] = [
    method(
        "of",
        "(I)Ljava/time/Year;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(year_new(vm, int_arg(args, 0))?),
    ),
    method(
        "now",
        "()Ljava/time/Year;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let millis = vm.host_time_millis();
            let zone = system_default_zone(vm)?;
            let offset = i64::from(zone_offset_seconds(vm, zone));
            let (year, _, _) = civil_from_days((millis / 1000 + offset).div_euclid(86_400));
            object(year_new(vm, year)?)
        },
    ),
    method(
        "parse",
        "(Ljava/lang/CharSequence;)Ljava/time/Year;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = crate::string_arg(vm, args[0])?;
            let year: i32 = text.parse().map_err(|_| {
                vm.throw_new("java/time/format/DateTimeParseException", Some(&text))
            })?;
            object(year_new(vm, year)?)
        },
    ),
    method("isLeap", "(J)Z", ACC_PUBLIC | ACC_STATIC, |_, _, args| {
        boolean(is_leap_year(crate::long_arg(args, 0) as i32))
    }),
    method("isLeap", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(is_leap_year(read_int_field(vm, receiver(args), "value")))
    }),
    method("getValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "value"))
    }),
    method("length", "()I", ACC_PUBLIC, |vm, _, args| {
        int(
            if is_leap_year(read_int_field(vm, receiver(args), "value")) {
                366
            } else {
                365
            },
        )
    }),
    method(
        "atDay",
        "(I)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let year = read_int_field(vm, receiver(args), "value");
            object(local_date_new(
                vm,
                days_from_civil(year, 1, int_arg(args, 1)),
            )?)
        },
    ),
    method(
        "atMonth",
        "(I)Ljava/time/YearMonth;",
        ACC_PUBLIC,
        |vm, _, args| {
            let year = read_int_field(vm, receiver(args), "value");
            object(year_month_new(vm, year, int_arg(args, 1))?)
        },
    ),
    method(
        "atMonth",
        "(Ljava/time/Month;)Ljava/time/YearMonth;",
        ACC_PUBLIC,
        |vm, _, args| {
            let year = read_int_field(vm, receiver(args), "value");
            let month = read_int_field(vm, ref_arg(args, 1), "value");
            object(year_month_new(vm, year, month)?)
        },
    ),
    method(
        "plusYears",
        "(J)Ljava/time/Year;",
        ACC_PUBLIC,
        |vm, _, args| {
            let year = read_int_field(vm, receiver(args), "value");
            object(year_new(
                vm,
                (i64::from(year) + crate::long_arg(args, 1)) as i32,
            )?)
        },
    ),
    method(
        "minusYears",
        "(J)Ljava/time/Year;",
        ACC_PUBLIC,
        |vm, _, args| {
            let year = read_int_field(vm, receiver(args), "value");
            object(year_new(
                vm,
                (i64::from(year) - crate::long_arg(args, 1)) as i32,
            )?)
        },
    ),
    method(
        "isAfter",
        "(Ljava/time/Year;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let year = read_int_field(vm, receiver(args), "value");
            boolean(year > read_int_field(vm, ref_arg(args, 1), "value"))
        },
    ),
    method(
        "isBefore",
        "(Ljava/time/Year;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let year = read_int_field(vm, receiver(args), "value");
            boolean(year < read_int_field(vm, ref_arg(args, 1), "value"))
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_name(vm.class_of(other)) != "java/time/Year" {
                return boolean(false);
            }
            boolean(
                read_int_field(vm, receiver(args), "value") == read_int_field(vm, other, "value"),
            )
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "value"))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = read_int_field(vm, receiver(args), "value");
            let text = if value < 0 {
                alloc::format!("-{:04}", -(i64::from(value)))
            } else if value > 9999 {
                alloc::format!("+{}", value)
            } else {
                alloc::format!("{}", value)
            };
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "compareTo",
        "(Ljava/time/Year;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let year = read_int_field(vm, receiver(args), "value");
            int(year - read_int_field(vm, ref_arg(args, 1), "value"))
        },
    ),
    method(
        "isSupported",
        "(Ljava/time/temporal/TemporalField;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let ordinal = chrono_field_ordinal(vm, ref_arg(args, 1));
            boolean(matches!(ordinal, Some(25..=27)))
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
            let year = i64::from(read_int_field(vm, this, "value"));
            let value = match ordinal {
                26 => year,
                25 => {
                    if year >= 1 {
                        year
                    } else {
                        1 - year
                    }
                }
                27 => i64::from(year >= 1),
                _ => return Err(unsupported_field(vm)),
            };
            long(value)
        },
    ),
    method(
        "range",
        "(Ljava/time/temporal/TemporalField;)Ljava/time/temporal/ValueRange;",
        ACC_PUBLIC,
        |vm, _, args| {
            let ordinal =
                chrono_field_ordinal(vm, ref_arg(args, 1)).ok_or_else(|| unsupported_field(vm))?;
            let (min, max) = match ordinal {
                26 => (-999_999_999, 999_999_999),
                25 => (1, 999_999_999),
                27 => (0, 1),
                _ => return Err(unsupported_field(vm)),
            };
            object(crate::temporal::value_range_new(vm, min, min, max, max)?)
        },
    ),
];

/// `java.time.Year`.
pub(crate) const YEAR: oxjvm_vm::NativeClass = class(
    "java/time/Year",
    Some("java/lang/Object"),
    &[
        "java/time/temporal/TemporalAccessor",
        "java/time/temporal/TemporalAdjuster",
        "java/lang/Comparable",
    ],
    ACC_PUBLIC | ACC_FINAL,
    &[field("value", "I", ACC_PRIVATE | ACC_FINAL, None)],
    &YEAR_METHODS,
    None,
);

/// Build an `OffsetTime` from a nano-of-day and a `ZoneOffset`.
fn offset_time_new(
    vm: &mut Vm<'_>,
    nano_of_day: i64,
    offset: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/time/OffsetTime")?;
    let instance = vm.new_instance(class)?;
    write_long_field(vm, instance, "nanoOfDay", nano_of_day);
    if let Some((declaring, field)) = vm.find_field_opt(class, "offset", "Ljava/time/ZoneOffset;") {
        vm.set_instance_ref(instance, declaring, field, offset);
    }
    Ok(instance)
}

/// Obtain a `ZoneOffset` for a total-seconds value via `ZoneOffset.ofTotalSeconds`.
fn zone_offset_from_seconds(vm: &mut Vm<'_>, seconds: i32) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/time/ZoneOffset")?;
    let (declaring, method) = vm
        .find_method(class, "ofTotalSeconds", "(I)Ljava/time/ZoneOffset;")
        .ok_or_else(|| VmError::internal("ZoneOffset.ofTotalSeconds missing"))?;
    Ok(vm
        .invoke_method(declaring, method, vec![int(seconds)?])?
        .as_ref())
}

/// Shift an `OffsetTime` by nanoseconds, wrapping within a single day.
fn offset_time_plus_nanos(vm: &mut Vm<'_>, this: ObjectRef, nanos: i64) -> Result<Value, VmError> {
    let offset = vm
        .read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/ZoneOffset;")
        .unwrap_or(ObjectRef::NULL);
    let total = read_long_field(vm, this, "nanoOfDay") + nanos;
    object(offset_time_new(
        vm,
        total.rem_euclid(86_400_000_000_000),
        offset,
    )?)
}

const OFFSET_TIME_METHODS: [oxjvm_vm::NativeMethodDef; 32] = [
    method(
        "of",
        "(IIIILjava/time/ZoneOffset;)Ljava/time/OffsetTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let hour = int_arg(args, 0);
            let minute = int_arg(args, 1);
            let second = int_arg(args, 2);
            let nano = int_arg(args, 3);
            let offset = ref_arg(args, 4);
            if offset.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            if !(0..24).contains(&hour)
                || !(0..60).contains(&minute)
                || !(0..60).contains(&second)
                || !(0..1_000_000_000).contains(&nano)
            {
                return Err(vm.throw_new("java/time/DateTimeException", None));
            }
            let nano_of_day = (i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second))
                * 1_000_000_000
                + i64::from(nano);
            object(offset_time_new(vm, nano_of_day, offset)?)
        },
    ),
    method(
        "of",
        "(IIILjava/time/ZoneOffset;)Ljava/time/OffsetTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/time/OffsetTime")?;
            let (declaring, method) = vm
                .find_method(
                    class,
                    "of",
                    "(IIIILjava/time/ZoneOffset;)Ljava/time/OffsetTime;",
                )
                .ok_or_else(|| VmError::internal("OffsetTime.of missing"))?;
            vm.invoke_method(
                declaring,
                method,
                vec![
                    int(int_arg(args, 0))?,
                    int(int_arg(args, 1))?,
                    int(int_arg(args, 2))?,
                    int(0)?,
                    Value::Ref(ref_arg(args, 3)),
                ],
            )
        },
    ),
    method(
        "of",
        "(IILjava/time/ZoneOffset;)Ljava/time/OffsetTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let class = vm.resolve_class("java/time/OffsetTime")?;
            let (declaring, method) = vm
                .find_method(
                    class,
                    "of",
                    "(IIIILjava/time/ZoneOffset;)Ljava/time/OffsetTime;",
                )
                .ok_or_else(|| VmError::internal("OffsetTime.of missing"))?;
            vm.invoke_method(
                declaring,
                method,
                vec![
                    int(int_arg(args, 0))?,
                    int(int_arg(args, 1))?,
                    int(0)?,
                    int(0)?,
                    Value::Ref(ref_arg(args, 2)),
                ],
            )
        },
    ),
    method(
        "of",
        "(Ljava/time/LocalTime;Ljava/time/ZoneOffset;)Ljava/time/OffsetTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let local = ref_arg(args, 0);
            let offset = ref_arg(args, 1);
            if local.is_null() || offset.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(offset_time_new(
                vm,
                read_long_field(vm, local, "nanoOfDay"),
                offset,
            )?)
        },
    ),
    method(
        "now",
        "()Ljava/time/OffsetTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let zone = system_default_zone(vm)?;
            let seconds = zone_offset_seconds(vm, zone);
            let offset = zone_offset_from_seconds(vm, seconds)?;
            let local_millis = vm.host_time_millis() + i64::from(seconds) * 1000;
            let nano = local_millis.rem_euclid(86_400_000) * 1_000_000;
            object(offset_time_new(vm, nano, offset)?)
        },
    ),
    method(
        "now",
        "(Ljava/time/ZoneId;)Ljava/time/OffsetTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let zone = ref_arg(args, 0);
            if zone.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let seconds = zone_offset_seconds(vm, zone);
            let offset = zone_offset_from_seconds(vm, seconds)?;
            let local_millis = vm.host_time_millis() + i64::from(seconds) * 1000;
            let nano = local_millis.rem_euclid(86_400_000) * 1_000_000;
            object(offset_time_new(vm, nano, offset)?)
        },
    ),
    method(
        "parse",
        "(Ljava/lang/CharSequence;)Ljava/time/OffsetTime;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = crate::string_arg(vm, args[0])?;
            let error = |vm: &mut Vm<'_>| {
                vm.throw_new("java/time/format/DateTimeParseException", Some(&text))
            };
            let split = text
                .bytes()
                .enumerate()
                .skip(1)
                .find(|(_, b)| *b == b'+' || *b == b'-' || *b == b'Z')
                .map(|(i, _)| i);
            let Some(split) = split else {
                return Err(error(vm));
            };
            let (local_text, offset_text) = text.split_at(split);
            let parts: Vec<&str> = local_text.split(':').collect();
            if parts.len() < 2 || parts.len() > 3 {
                return Err(error(vm));
            }
            let hour: i64 = parts[0].parse().map_err(|_| error(vm))?;
            let minute: i64 = parts[1].parse().map_err(|_| error(vm))?;
            let (second, nano) = if parts.len() == 3 {
                let (sec_text, frac) = match parts[2].split_once('.') {
                    Some((sec, frac)) => (sec, frac),
                    None => (parts[2], ""),
                };
                let second: i64 = sec_text.parse().map_err(|_| error(vm))?;
                if frac.is_empty() || frac.len() > 9 || !frac.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(error(vm));
                }
                let mut padded = String::from(frac);
                while padded.len() < 9 {
                    padded.push('0');
                }
                let nano: i64 = padded.parse().map_err(|_| error(vm))?;
                (second, nano)
            } else {
                (0, 0)
            };
            if !(0..24).contains(&hour) || !(0..60).contains(&minute) || !(0..60).contains(&second)
            {
                return Err(error(vm));
            }
            let offset_seconds = if offset_text == "Z" {
                0
            } else {
                let sign = if offset_text.starts_with('-') { -1 } else { 1 };
                let rest = &offset_text[1..];
                let oparts: Vec<&str> = rest.split(':').collect();
                if oparts.len() < 2 || oparts.len() > 3 {
                    return Err(error(vm));
                }
                let oh: i32 = oparts[0].parse().map_err(|_| error(vm))?;
                let om: i32 = oparts[1].parse().map_err(|_| error(vm))?;
                let os: i32 = if oparts.len() == 3 {
                    oparts[2].parse().map_err(|_| error(vm))?
                } else {
                    0
                };
                if !(0..=18).contains(&oh) || !(0..60).contains(&om) || !(0..60).contains(&os) {
                    return Err(error(vm));
                }
                sign * (oh * 3600 + om * 60 + os)
            };
            let offset = zone_offset_from_seconds(vm, offset_seconds)?;
            let nano_of_day = (hour * 3600 + minute * 60 + second) * 1_000_000_000 + nano;
            object(offset_time_new(vm, nano_of_day, offset)?)
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
        "getOffset",
        "()Ljava/time/ZoneOffset;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(
                vm.read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/ZoneOffset;")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "withOffsetSameInstant",
        "(Ljava/time/ZoneOffset;)Ljava/time/OffsetTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let new_offset = ref_arg(args, 1);
            if new_offset.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let old_offset = vm
                .read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/ZoneOffset;")
                .unwrap_or(ObjectRef::NULL);
            let shift = i64::from(
                zone_offset_seconds(vm, new_offset) - zone_offset_seconds(vm, old_offset),
            );
            let total = read_long_field(vm, this, "nanoOfDay") + shift * 1_000_000_000;
            object(offset_time_new(
                vm,
                total.rem_euclid(86_400_000_000_000),
                new_offset,
            )?)
        },
    ),
    method(
        "withOffsetSameLocal",
        "(Ljava/time/ZoneOffset;)Ljava/time/OffsetTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let new_offset = ref_arg(args, 1);
            if new_offset.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(offset_time_new(
                vm,
                read_long_field(vm, this, "nanoOfDay"),
                new_offset,
            )?)
        },
    ),
    method(
        "atDate",
        "(Ljava/time/LocalDate;)Ljava/time/OffsetDateTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let date = ref_arg(args, 1);
            if date.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let offset = vm
                .read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/ZoneOffset;")
                .unwrap_or(ObjectRef::NULL);
            object(offset_date_time_new(
                vm,
                read_long_field(vm, date, "epochDay"),
                read_long_field(vm, this, "nanoOfDay"),
                offset,
            )?)
        },
    ),
    method(
        "isBefore",
        "(Ljava/time/OffsetTime;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let this_offset = vm
                .read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/ZoneOffset;")
                .unwrap_or(ObjectRef::NULL);
            let other_offset = vm
                .read_ref_field(
                    other,
                    vm.class_of(other),
                    "offset",
                    "Ljava/time/ZoneOffset;",
                )
                .unwrap_or(ObjectRef::NULL);
            let a = read_long_field(vm, this, "nanoOfDay")
                - i64::from(zone_offset_seconds(vm, this_offset)) * 1_000_000_000;
            let b = read_long_field(vm, other, "nanoOfDay")
                - i64::from(zone_offset_seconds(vm, other_offset)) * 1_000_000_000;
            boolean(a < b)
        },
    ),
    method(
        "isAfter",
        "(Ljava/time/OffsetTime;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let this_offset = vm
                .read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/ZoneOffset;")
                .unwrap_or(ObjectRef::NULL);
            let other_offset = vm
                .read_ref_field(
                    other,
                    vm.class_of(other),
                    "offset",
                    "Ljava/time/ZoneOffset;",
                )
                .unwrap_or(ObjectRef::NULL);
            let a = read_long_field(vm, this, "nanoOfDay")
                - i64::from(zone_offset_seconds(vm, this_offset)) * 1_000_000_000;
            let b = read_long_field(vm, other, "nanoOfDay")
                - i64::from(zone_offset_seconds(vm, other_offset)) * 1_000_000_000;
            boolean(a > b)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_name(vm.class_of(other)) != "java/time/OffsetTime" {
                return boolean(false);
            }
            if read_long_field(vm, this, "nanoOfDay") != read_long_field(vm, other, "nanoOfDay") {
                return boolean(false);
            }
            let this_offset = vm
                .read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/ZoneOffset;")
                .unwrap_or(ObjectRef::NULL);
            let other_offset = vm
                .read_ref_field(
                    other,
                    vm.class_of(other),
                    "offset",
                    "Ljava/time/ZoneOffset;",
                )
                .unwrap_or(ObjectRef::NULL);
            boolean(zone_offset_seconds(vm, this_offset) == zone_offset_seconds(vm, other_offset))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let nano = read_long_field(vm, this, "nanoOfDay");
        let offset = vm
            .read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/ZoneOffset;")
            .unwrap_or(ObjectRef::NULL);
        int(((nano ^ (nano >> 32)) as i32) ^ zone_offset_seconds(vm, offset))
    }),
    method(
        "compareTo",
        "(Ljava/time/OffsetTime;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let this_offset = vm
                .read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/ZoneOffset;")
                .unwrap_or(ObjectRef::NULL);
            let other_offset = vm
                .read_ref_field(
                    other,
                    vm.class_of(other),
                    "offset",
                    "Ljava/time/ZoneOffset;",
                )
                .unwrap_or(ObjectRef::NULL);
            let a = read_long_field(vm, this, "nanoOfDay")
                - i64::from(zone_offset_seconds(vm, this_offset)) * 1_000_000_000;
            let b = read_long_field(vm, other, "nanoOfDay")
                - i64::from(zone_offset_seconds(vm, other_offset)) * 1_000_000_000;
            if a != b {
                return int(if a < b { -1 } else { 1 });
            }
            let local_a = read_long_field(vm, this, "nanoOfDay");
            let local_b = read_long_field(vm, other, "nanoOfDay");
            int(if local_a < local_b {
                -1
            } else if local_a > local_b {
                1
            } else {
                0
            })
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let nano = read_long_field(vm, this, "nanoOfDay");
            let hour = nano / 3_600_000_000_000;
            let minute = (nano / 60_000_000_000) % 60;
            let second = (nano / 1_000_000_000) % 60;
            let nanos = nano % 1_000_000_000;
            let time_text = if second == 0 && nanos == 0 {
                alloc::format!("{hour:02}:{minute:02}")
            } else {
                alloc::format!("{hour:02}:{minute:02}:{second:02}{}", nano_fraction(nanos))
            };
            let offset = vm
                .read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/ZoneOffset;")
                .unwrap_or(ObjectRef::NULL);
            let text = alloc::format!(
                "{time_text}{}",
                crate::lang::zone_offset_id(zone_offset_seconds(vm, offset))
            );
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "format",
        "(Ljava/time/format/DateTimeFormatter;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let formatter = ref_arg(args, 1);
            if formatter.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let pattern = vm
                .read_string_field(formatter, vm.class_of(formatter), "pattern")
                .unwrap_or_default();
            let millis = temporal_local_millis(vm, this)?;
            object(vm.make_string(&crate::text::format_pattern(&pattern, millis))?)
        },
    ),
    method(
        "truncatedTo",
        "(Ljava/time/temporal/TemporalUnit;)Ljava/time/OffsetTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let unit = ref_arg(args, 1);
            if unit.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let nanos = crate::temporal::chrono_unit_nanos(vm, unit);
            let day_nanos = 86_400_000_000_000i64;
            let truncated = if nanos == day_nanos {
                0
            } else if nanos <= 0 || nanos > day_nanos {
                return Err(vm.throw_new(
                    "java/time/temporal/UnsupportedTemporalTypeException",
                    Some("Unit is too large to be used for truncation"),
                ));
            } else if day_nanos % nanos != 0 {
                return Err(vm.throw_new(
                    "java/time/temporal/UnsupportedTemporalTypeException",
                    Some("Unit must divide into a standard day without remainder"),
                ));
            } else {
                read_long_field(vm, this, "nanoOfDay") / nanos * nanos
            };
            let offset = vm
                .read_ref_field(this, vm.class_of(this), "offset", "Ljava/time/ZoneOffset;")
                .unwrap_or(ObjectRef::NULL);
            object(offset_time_new(vm, truncated, offset)?)
        },
    ),
    method(
        "plusHours",
        "(J)Ljava/time/OffsetTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            offset_time_plus_nanos(
                vm,
                receiver(args),
                crate::long_arg(args, 1).saturating_mul(3_600_000_000_000),
            )
        },
    ),
    method(
        "plusMinutes",
        "(J)Ljava/time/OffsetTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            offset_time_plus_nanos(
                vm,
                receiver(args),
                crate::long_arg(args, 1).saturating_mul(60_000_000_000),
            )
        },
    ),
    method(
        "plusSeconds",
        "(J)Ljava/time/OffsetTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            offset_time_plus_nanos(
                vm,
                receiver(args),
                crate::long_arg(args, 1).saturating_mul(1_000_000_000),
            )
        },
    ),
    method(
        "plusNanos",
        "(J)Ljava/time/OffsetTime;",
        ACC_PUBLIC,
        |vm, _, args| offset_time_plus_nanos(vm, receiver(args), crate::long_arg(args, 1)),
    ),
    method(
        "minusHours",
        "(J)Ljava/time/OffsetTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, method) = vm.resolve_virtual_method(
                receiver(args),
                "plusHours",
                "(J)Ljava/time/OffsetTime;",
            )?;
            vm.invoke_method(
                class,
                method,
                vec![
                    Value::Ref(receiver(args)),
                    long(crate::long_arg(args, 1).wrapping_neg())?,
                ],
            )
        },
    ),
    method(
        "minusMinutes",
        "(J)Ljava/time/OffsetTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, method) = vm.resolve_virtual_method(
                receiver(args),
                "plusMinutes",
                "(J)Ljava/time/OffsetTime;",
            )?;
            vm.invoke_method(
                class,
                method,
                vec![
                    Value::Ref(receiver(args)),
                    long(crate::long_arg(args, 1).wrapping_neg())?,
                ],
            )
        },
    ),
    method(
        "minusSeconds",
        "(J)Ljava/time/OffsetTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, method) = vm.resolve_virtual_method(
                receiver(args),
                "plusSeconds",
                "(J)Ljava/time/OffsetTime;",
            )?;
            vm.invoke_method(
                class,
                method,
                vec![
                    Value::Ref(receiver(args)),
                    long(crate::long_arg(args, 1).wrapping_neg())?,
                ],
            )
        },
    ),
    method(
        "minusNanos",
        "(J)Ljava/time/OffsetTime;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (class, method) = vm.resolve_virtual_method(
                receiver(args),
                "plusNanos",
                "(J)Ljava/time/OffsetTime;",
            )?;
            vm.invoke_method(
                class,
                method,
                vec![
                    Value::Ref(receiver(args)),
                    long(crate::long_arg(args, 1).wrapping_neg())?,
                ],
            )
        },
    ),
];

pub(crate) const OFFSET_TIME: oxjvm_vm::NativeClass = class(
    "java/time/OffsetTime",
    Some("java/lang/Object"),
    &[
        "java/time/temporal/TemporalAccessor",
        "java/time/temporal/Temporal",
        "java/lang/Comparable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("nanoOfDay", "J", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "offset",
            "Ljava/time/ZoneOffset;",
            ACC_PRIVATE | ACC_FINAL,
            None,
        ),
    ],
    &OFFSET_TIME_METHODS,
    None,
);

pub(crate) const YEAR_MONTH: oxjvm_vm::NativeClass = class(
    "java/time/YearMonth",
    Some("java/lang/Object"),
    &[
        "java/time/temporal/TemporalAccessor",
        "java/time/temporal/TemporalAdjuster",
        "java/lang/Comparable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("year", "I", ACC_PRIVATE | ACC_FINAL, None),
        field("month", "I", ACC_PRIVATE | ACC_FINAL, None),
    ],
    &YEAR_MONTH_METHODS,
    None,
);

fn month_day_new(vm: &mut Vm<'_>, month: i32, day: i32) -> Result<ObjectRef, VmError> {
    if !(1..=12).contains(&month) {
        return Err(vm.throw_new("java/time/DateTimeException", Some("Invalid month")));
    }
    let max = if month == 2 {
        29
    } else {
        length_of_month(2000, month)
    };
    if day < 1 || day > max {
        return Err(vm.throw_new("java/time/DateTimeException", Some("Invalid day")));
    }
    let class = vm.resolve_class("java/time/MonthDay")?;
    let instance = vm.new_instance(class)?;
    write_int_field(vm, instance, "month", month);
    write_int_field(vm, instance, "day", day);
    Ok(instance)
}

const MONTH_DAY_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
    method(
        "of",
        "(II)Ljava/time/MonthDay;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| object(month_day_new(vm, int_arg(args, 0), int_arg(args, 1))?),
    ),
    method(
        "parse",
        "(Ljava/lang/CharSequence;)Ljava/time/MonthDay;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = crate::string_arg(vm, args[0])?;
            let error = |vm: &mut Vm<'_>| {
                vm.throw_new("java/time/format/DateTimeParseException", Some(&text))
            };
            let stripped = text.strip_prefix("--").ok_or_else(|| error(vm))?;
            let parts: Vec<&str> = stripped.split('-').collect();
            if parts.len() != 2 {
                return Err(error(vm));
            }
            let month: i32 = parts[0].parse().map_err(|_| error(vm))?;
            let day: i32 = parts[1].parse().map_err(|_| error(vm))?;
            object(month_day_new(vm, month, day)?)
        },
    ),
    method("getMonthValue", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "month"))
    }),
    method("getDayOfMonth", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "day"))
    }),
    method("isValidYear", "(I)Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let month = read_int_field(vm, this, "month");
        let day = read_int_field(vm, this, "day");
        boolean(day <= length_of_month(int_arg(args, 1), month))
    }),
    method(
        "atYear",
        "(I)Ljava/time/LocalDate;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let year = int_arg(args, 1);
            let month = read_int_field(vm, this, "month");
            let day = read_int_field(vm, this, "day");
            if day > length_of_month(year, month) {
                return Err(vm.throw_new("java/time/DateTimeException", Some("Invalid day")));
            }
            object(local_date_new(vm, days_from_civil(year, month, day))?)
        },
    ),
    method(
        "compareTo",
        "(Ljava/time/MonthDay;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            let a = read_int_field(vm, this, "month") * 100 + read_int_field(vm, this, "day");
            let b = read_int_field(vm, other, "month") * 100 + read_int_field(vm, other, "day");
            int(a.cmp(&b) as i32)
        },
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(other) != vm.class_of(this) {
                return boolean(false);
            }
            boolean(
                read_int_field(vm, this, "month") == read_int_field(vm, other, "month")
                    && read_int_field(vm, this, "day") == read_int_field(vm, other, "day"),
            )
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        int((read_int_field(vm, this, "month") << 6) ^ read_int_field(vm, this, "day"))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = alloc::format!(
                "--{:02}-{:02}",
                read_int_field(vm, this, "month"),
                read_int_field(vm, this, "day")
            );
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "withMonth",
        "(I)Ljava/time/MonthDay;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(month_day_new(
                vm,
                int_arg(args, 1),
                read_int_field(vm, this, "day"),
            )?)
        },
    ),
    method(
        "now",
        "()Ljava/time/MonthDay;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let millis = vm.host_time_millis();
            let zone = system_default_zone(vm)?;
            let offset = i64::from(zone_offset_seconds(vm, zone));
            let (_, month, day) = civil_from_days((millis / 1000 + offset).div_euclid(86_400));
            object(month_day_new(vm, month, day)?)
        },
    ),
    method(
        "isSupported",
        "(Ljava/time/temporal/TemporalField;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let ordinal = chrono_field_ordinal(vm, ref_arg(args, 1));
            boolean(matches!(ordinal, Some(18 | 23)))
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
            let value = match ordinal {
                23 => i64::from(read_int_field(vm, this, "month")),
                18 => i64::from(read_int_field(vm, this, "day")),
                _ => return Err(unsupported_field(vm)),
            };
            long(value)
        },
    ),
    method(
        "range",
        "(Ljava/time/temporal/TemporalField;)Ljava/time/temporal/ValueRange;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let ordinal =
                chrono_field_ordinal(vm, ref_arg(args, 1)).ok_or_else(|| unsupported_field(vm))?;
            let (min_low, min_high, max_low, max_high) = match ordinal {
                23 => (1, 1, 12, 12),
                18 => {
                    let month = read_int_field(vm, this, "month");
                    match month {
                        2 => (1, 1, 28, 29),
                        4 | 6 | 9 | 11 => (1, 1, 30, 30),
                        _ => (1, 1, 31, 31),
                    }
                }
                _ => return Err(unsupported_field(vm)),
            };
            object(crate::temporal::value_range_new(
                vm, min_low, min_high, max_low, max_high,
            )?)
        },
    ),
];

pub(crate) const MONTH_DAY: oxjvm_vm::NativeClass = class(
    "java/time/MonthDay",
    Some("java/lang/Object"),
    &[
        "java/time/temporal/TemporalAccessor",
        "java/time/temporal/TemporalAdjuster",
        "java/lang/Comparable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("month", "I", ACC_PRIVATE | ACC_FINAL, None),
        field("day", "I", ACC_PRIVATE | ACC_FINAL, None),
    ],
    &MONTH_DAY_METHODS,
    None,
);
