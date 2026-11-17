//! `java.time.temporal`: the field/unit/accessor abstractions behind `java.time`.
//!
//! The interfaces are declared so the time classes can implement them and tests can load
//! them; `ChronoField`/`ChronoUnit` are enums carrying their range/duration metadata, and
//! `ValueRange` models a bounded range of valid values.

use alloc::vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::lang::{abstract_method_pub, read_int_field, read_long_field, write_int_field};
use crate::{boolean, class, field, int, long, long_arg, method, object, receiver, ref_arg};

// -------------------------------------------------------------------------------------------
// Interfaces
// -------------------------------------------------------------------------------------------

const TEMPORAL_ACCESSOR_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "isSupported",
        "(Ljava/time/temporal/TemporalField;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "get",
        "(Ljava/time/temporal/TemporalField;)I",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "getLong",
        "(Ljava/time/temporal/TemporalField;)J",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "range",
        "(Ljava/time/temporal/TemporalField;)Ljava/time/temporal/ValueRange;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "query",
        "(Ljava/time/temporal/TemporalQuery;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
];

pub(crate) const TEMPORAL_ACCESSOR: oxjvm_vm::NativeClass = class(
    "java/time/temporal/TemporalAccessor",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &TEMPORAL_ACCESSOR_METHODS,
    None,
);

const TEMPORAL_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "isSupported",
        "(Ljava/time/temporal/TemporalUnit;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "with",
        "(Ljava/time/temporal/TemporalField;J)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "plus",
        "(JLjava/time/temporal/TemporalUnit;)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "minus",
        "(JLjava/time/temporal/TemporalUnit;)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "until",
        "(Ljava/time/temporal/Temporal;Ljava/time/temporal/TemporalUnit;)J",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
];

pub(crate) const TEMPORAL: oxjvm_vm::NativeClass = class(
    "java/time/temporal/Temporal",
    Some("java/lang/Object"),
    &["java/time/temporal/TemporalAccessor"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &TEMPORAL_METHODS,
    None,
);

const TEMPORAL_FIELD_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method(
        "getBaseUnit",
        "()Ljava/time/temporal/TemporalUnit;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "getRangeUnit",
        "()Ljava/time/temporal/TemporalUnit;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "range",
        "()Ljava/time/temporal/ValueRange;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "isDateBased",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "isTimeBased",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "isSupportedBy",
        "(Ljava/time/temporal/TemporalAccessor;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "rangeRefinedBy",
        "(Ljava/time/temporal/TemporalAccessor;)Ljava/time/temporal/ValueRange;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "getFrom",
        "(Ljava/time/temporal/TemporalAccessor;)J",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "adjustInto",
        "(Ljava/time/temporal/Temporal;J)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "displayName",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "getDisplayName",
        "(Ljava/util/Locale;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
];

pub(crate) const TEMPORAL_FIELD: oxjvm_vm::NativeClass = class(
    "java/time/temporal/TemporalField",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &TEMPORAL_FIELD_METHODS,
    None,
);

const TEMPORAL_UNIT_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "isDurationEstimated",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "isDateBased",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "isTimeBased",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "isSupportedBy",
        "(Ljava/time/temporal/Temporal;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "addTo",
        "(Ljava/time/temporal/Temporal;J)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "between",
        "(Ljava/time/temporal/Temporal;Ljava/time/temporal/Temporal;)J",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "getDuration",
        "()Ljava/time/Duration;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
];

pub(crate) const TEMPORAL_UNIT: oxjvm_vm::NativeClass = class(
    "java/time/temporal/TemporalUnit",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &TEMPORAL_UNIT_METHODS,
    None,
);

const TEMPORAL_QUERY_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "queryFrom",
    "(Ljava/time/temporal/TemporalAccessor;)Ljava/lang/Object;",
    ACC_PUBLIC | ACC_ABSTRACT,
    abstract_method_pub,
)];

pub(crate) const TEMPORAL_QUERY: oxjvm_vm::NativeClass = class(
    "java/time/temporal/TemporalQuery",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &TEMPORAL_QUERY_METHODS,
    None,
);

const TEMPORAL_ADJUSTER_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "adjustInto",
    "(Ljava/time/temporal/Temporal;)Ljava/time/temporal/Temporal;",
    ACC_PUBLIC | ACC_ABSTRACT,
    abstract_method_pub,
)];

pub(crate) const TEMPORAL_ADJUSTER: oxjvm_vm::NativeClass = class(
    "java/time/temporal/TemporalAdjuster",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &TEMPORAL_ADJUSTER_METHODS,
    None,
);

const TEMPORAL_AMOUNT_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "get",
        "(Ljava/time/temporal/TemporalUnit;)J",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "getUnits",
        "()Ljava/util/List;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
    method(
        "addTo",
        "(Ljava/time/temporal/Temporal;)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    ),
];

pub(crate) const TEMPORAL_AMOUNT: oxjvm_vm::NativeClass = class(
    "java/time/temporal/TemporalAmount",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &TEMPORAL_AMOUNT_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// ValueRange
// -------------------------------------------------------------------------------------------

pub(crate) fn value_range_new(
    vm: &mut Vm<'_>,
    min_low: i64,
    min_high: i64,
    max_low: i64,
    max_high: i64,
) -> Result<ObjectRef, VmError> {
    if min_low > min_high {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("smallest minimum must be <= largest minimum"),
        ));
    }
    if max_low > max_high {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("smallest maximum must be <= largest maximum"),
        ));
    }
    if min_low > max_low {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("smallest minimum must be <= smallest maximum"),
        ));
    }
    if min_high > max_high {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("largest minimum must be <= largest maximum"),
        ));
    }
    let class = vm.resolve_class("java/time/temporal/ValueRange")?;
    let instance = vm.new_instance(class)?;
    write_long(vm, instance, "minLow", min_low);
    write_long(vm, instance, "minHigh", min_high);
    write_long(vm, instance, "maxLow", max_low);
    write_long(vm, instance, "maxHigh", max_high);
    Ok(instance)
}

fn write_long(vm: &mut Vm<'_>, object: ObjectRef, name: &str, value: i64) {
    let class = vm.class_of(object);
    if let Some((declaring, field)) = vm.find_field_opt(class, name, "J") {
        vm.set_instance_long(object, declaring, field, value);
    }
}

/// The four bounds of a `ValueRange`, in declaration order.
fn value_range_bounds(vm: &Vm<'_>, this: ObjectRef) -> (i64, i64, i64, i64) {
    (
        read_long_field(vm, this, "minLow"),
        read_long_field(vm, this, "minHigh"),
        read_long_field(vm, this, "maxLow"),
        read_long_field(vm, this, "maxHigh"),
    )
}

const VALUE_RANGE_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
    method("<init>", "(JJJJ)V", ACC_PRIVATE, |vm, _, args| {
        let this = receiver(args);
        write_long(vm, this, "minLow", long_arg(args, 1));
        write_long(vm, this, "minHigh", long_arg(args, 2));
        write_long(vm, this, "maxLow", long_arg(args, 3));
        write_long(vm, this, "maxHigh", long_arg(args, 4));
        crate::void()
    }),
    method(
        "of",
        "(JJ)Ljava/time/temporal/ValueRange;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let min = long_arg(args, 0);
            let max = long_arg(args, 1);
            object(value_range_new(vm, min, min, max, max)?)
        },
    ),
    method(
        "of",
        "(JJJ)Ljava/time/temporal/ValueRange;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let min = long_arg(args, 0);
            object(value_range_new(
                vm,
                min,
                min,
                long_arg(args, 1),
                long_arg(args, 2),
            )?)
        },
    ),
    method(
        "of",
        "(JJJJ)Ljava/time/temporal/ValueRange;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            object(value_range_new(
                vm,
                long_arg(args, 0),
                long_arg(args, 1),
                long_arg(args, 2),
                long_arg(args, 3),
            )?)
        },
    ),
    method("getMinimum", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "minLow"))
    }),
    method("getLargestMinimum", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "minHigh"))
    }),
    method("getSmallestMaximum", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "maxLow"))
    }),
    method("getMaximum", "()J", ACC_PUBLIC, |vm, _, args| {
        long(read_long_field(vm, receiver(args), "maxHigh"))
    }),
    method("isFixed", "()Z", ACC_PUBLIC, |vm, _, args| {
        let (min_low, min_high, max_low, max_high) = value_range_bounds(vm, receiver(args));
        boolean(min_low == min_high && max_low == max_high)
    }),
    method("isIntValue", "()Z", ACC_PUBLIC, |vm, _, args| {
        let (min_low, _, _, max_high) = value_range_bounds(vm, receiver(args));
        boolean(min_low >= i64::from(i32::MIN) && max_high <= i64::from(i32::MAX))
    }),
    method("isValidValue", "(J)Z", ACC_PUBLIC, |vm, _, args| {
        let (min_low, _, _, max_high) = value_range_bounds(vm, receiver(args));
        let value = long_arg(args, 1);
        boolean(value >= min_low && value <= max_high)
    }),
    method("isValidIntValue", "(J)Z", ACC_PUBLIC, |vm, _, args| {
        let (min_low, _, _, max_high) = value_range_bounds(vm, receiver(args));
        let value = long_arg(args, 1);
        boolean(
            min_low >= i64::from(i32::MIN)
                && max_high <= i64::from(i32::MAX)
                && value >= min_low
                && value <= max_high,
        )
    }),
    method(
        "checkValidValue",
        "(JLjava/time/temporal/TemporalField;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let (min_low, _, _, max_high) = value_range_bounds(vm, receiver(args));
            let value = long_arg(args, 1);
            if value < min_low || value > max_high {
                return Err(vm.throw_new("java/time/DateTimeException", Some("value out of range")));
            }
            long(value)
        },
    ),
    method(
        "checkValidIntValue",
        "(JLjava/time/temporal/TemporalField;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let (min_low, _, _, max_high) = value_range_bounds(vm, receiver(args));
            let value = long_arg(args, 1);
            if value < i64::from(i32::MIN)
                || value > i64::from(i32::MAX)
                || value < min_low
                || value > max_high
            {
                return Err(vm.throw_new("java/time/DateTimeException", Some("value out of range")));
            }
            int(value as i32)
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
            let class = vm.resolve_class("java/time/temporal/ValueRange")?;
            if !vm.is_instance(other, class) {
                return boolean(false);
            }
            boolean(value_range_bounds(vm, receiver(args)) == value_range_bounds(vm, other))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let (a, b, c, d) = value_range_bounds(vm, receiver(args));
        let mut hash = (a ^ (a >> 32)) as i32;
        for bound in [b, c, d] {
            hash = hash
                .wrapping_mul(31)
                .wrapping_add((bound ^ (bound >> 32)) as i32);
        }
        int(hash)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let (min_low, min_high, max_low, max_high) = value_range_bounds(vm, receiver(args));
            let mut text = alloc::format!("{min_low}");
            if min_low != min_high {
                text.push('/');
                text.push_str(&alloc::format!("{min_high}"));
            }
            text.push_str(" - ");
            text.push_str(&alloc::format!("{max_low}"));
            if max_low != max_high {
                text.push('/');
                text.push_str(&alloc::format!("{max_high}"));
            }
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const VALUE_RANGE: oxjvm_vm::NativeClass = class(
    "java/time/temporal/ValueRange",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("minLow", "J", ACC_PRIVATE | ACC_FINAL, None),
        field("minHigh", "J", ACC_PRIVATE | ACC_FINAL, None),
        field("maxLow", "J", ACC_PRIVATE | ACC_FINAL, None),
        field("maxHigh", "J", ACC_PRIVATE | ACC_FINAL, None),
    ],
    &VALUE_RANGE_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// ChronoUnit
// -------------------------------------------------------------------------------------------

/// (name, duration nanos, isDurationEstimated, isDateBased, isTimeBased).
const CHRONO_UNITS: [(&str, i64, bool, bool, bool); 16] = [
    ("NANOS", 1, false, false, true),
    ("MICROS", 1_000, false, false, true),
    ("MILLIS", 1_000_000, false, false, true),
    ("SECONDS", 1_000_000_000, false, false, true),
    ("MINUTES", 60_000_000_000, false, false, true),
    ("HOURS", 3_600_000_000_000, false, false, true),
    ("HALF_DAYS", 43_200_000_000_000, false, false, true),
    ("DAYS", 86_400_000_000_000, true, true, false),
    ("WEEKS", 604_800_000_000_000, true, true, false),
    ("MONTHS", 2_629_746_000_000_000, true, true, false),
    ("YEARS", 31_556_952_000_000_000, true, true, false),
    ("DECADES", 315_569_520_000_000_000, true, true, false),
    ("CENTURIES", 3_155_695_200_000_000_000, true, true, false),
    ("MILLENNIA", i64::MAX, true, true, false),
    ("ERAS", i64::MAX, true, true, false),
    ("FOREVER", i64::MAX, true, false, false),
];

fn chrono_unit_new(vm: &mut Vm<'_>, ordinal: i32) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/time/temporal/ChronoUnit")?;
    let instance = vm.new_instance(class)?;
    write_int_field(vm, instance, "ordinal", ordinal);
    Ok(instance)
}

fn chrono_unit_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/time/temporal/ChronoUnit")?;
    for (index, (name, ..)) in CHRONO_UNITS.iter().enumerate() {
        let instance = chrono_unit_new(vm, index as i32)?;
        vm.set_static_value(
            class,
            name,
            "Ljava/time/temporal/ChronoUnit;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

fn chrono_unit_duration(vm: &mut Vm<'_>, ordinal: i32) -> Result<Value, VmError> {
    let nanos = CHRONO_UNITS
        .get(ordinal as usize)
        .map(|entry| entry.1)
        .unwrap_or(0);
    let class = vm.resolve_class("java/time/Duration")?;
    let instance = vm.new_instance(class)?;
    write_long(vm, instance, "nanos", nanos);
    object(instance)
}

const CHRONO_UNIT_METHODS: [oxjvm_vm::NativeMethodDef; 13] = [
    method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "ordinal"))
    }),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let ordinal = read_int_field(vm, receiver(args), "ordinal").clamp(0, 15) as usize;
        object(vm.make_string(CHRONO_UNITS[ordinal].0)?)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let ordinal = read_int_field(vm, receiver(args), "ordinal").clamp(0, 15) as usize;
            object(vm.make_string(CHRONO_UNITS[ordinal].0)?)
        },
    ),
    method(
        "getDuration",
        "()Ljava/time/Duration;",
        ACC_PUBLIC,
        |vm, _, args| chrono_unit_duration(vm, read_int_field(vm, receiver(args), "ordinal")),
    ),
    method("isDurationEstimated", "()Z", ACC_PUBLIC, |vm, _, args| {
        let ordinal = read_int_field(vm, receiver(args), "ordinal").clamp(0, 15) as usize;
        boolean(CHRONO_UNITS[ordinal].2)
    }),
    method("isDateBased", "()Z", ACC_PUBLIC, |vm, _, args| {
        let ordinal = read_int_field(vm, receiver(args), "ordinal").clamp(0, 15) as usize;
        boolean(CHRONO_UNITS[ordinal].3)
    }),
    method("isTimeBased", "()Z", ACC_PUBLIC, |vm, _, args| {
        let ordinal = read_int_field(vm, receiver(args), "ordinal").clamp(0, 15) as usize;
        boolean(CHRONO_UNITS[ordinal].4)
    }),
    method(
        "isSupportedBy",
        "(Ljava/time/temporal/Temporal;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let temporal = ref_arg(args, 1);
            let (class, method) = vm.resolve_virtual_method(
                temporal,
                "isSupported",
                "(Ljava/time/temporal/TemporalUnit;)Z",
            )?;
            vm.invoke_method(class, method, vec![Value::Ref(temporal), args[0]])
        },
    ),
    method(
        "addTo",
        "(Ljava/time/temporal/Temporal;J)Ljava/time/temporal/Temporal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let temporal = ref_arg(args, 1);
            let (class, method) = vm.resolve_virtual_method(
                temporal,
                "plus",
                "(JLjava/time/temporal/TemporalUnit;)Ljava/time/temporal/Temporal;",
            )?;
            vm.invoke_method(class, method, vec![Value::Ref(temporal), args[2], args[0]])
        },
    ),
    method(
        "between",
        "(Ljava/time/temporal/Temporal;Ljava/time/temporal/Temporal;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let start = ref_arg(args, 1);
            let end = ref_arg(args, 2);
            let (class, method) = vm.resolve_virtual_method(
                start,
                "until",
                "(Ljava/time/temporal/Temporal;Ljava/time/temporal/TemporalUnit;)J",
            )?;
            vm.invoke_method(
                class,
                method,
                vec![Value::Ref(start), Value::Ref(end), args[0]],
            )
        },
    ),
    method(
        "values",
        "()[Ljava/time/temporal/ChronoUnit;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/time/temporal/ChronoUnit")?;
            vm.ensure_initialized(class)?;
            let array = vm.allocate_object_array(class, CHRONO_UNITS.len())?;
            for (index, (name, ..)) in CHRONO_UNITS.iter().enumerate() {
                if let Some(value) =
                    vm.read_static_ref_if_set(class, name, "Ljava/time/temporal/ChronoUnit;")
                {
                    vm.array_set_ref(array, index, value)?;
                }
            }
            object(array)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/time/temporal/ChronoUnit;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 0));
            let class = vm.resolve_class("java/time/temporal/ChronoUnit")?;
            vm.ensure_initialized(class)?;
            for candidate in CHRONO_UNITS {
                if candidate.0 == name {
                    if let Some(value) = vm.read_static_ref_if_set(
                        class,
                        candidate.0,
                        "Ljava/time/temporal/ChronoUnit;",
                    ) {
                        return object(value);
                    }
                }
            }
            Err(vm.throw_new("java/lang/IllegalArgumentException", None))
        },
    ),
    method(
        "compareTo",
        "(Ljava/time/temporal/ChronoUnit;)I",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = read_int_field(vm, receiver(args), "ordinal");
            let other = read_int_field(vm, ref_arg(args, 1), "ordinal");
            int(this - other)
        },
    ),
];

pub(crate) const CHRONO_UNIT: oxjvm_vm::NativeClass = class(
    "java/time/temporal/ChronoUnit",
    Some("java/lang/Object"),
    &["java/time/temporal/TemporalUnit", "java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "NANOS",
            "Ljava/time/temporal/ChronoUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MICROS",
            "Ljava/time/temporal/ChronoUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MILLIS",
            "Ljava/time/temporal/ChronoUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SECONDS",
            "Ljava/time/temporal/ChronoUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MINUTES",
            "Ljava/time/temporal/ChronoUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "HOURS",
            "Ljava/time/temporal/ChronoUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "HALF_DAYS",
            "Ljava/time/temporal/ChronoUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "DAYS",
            "Ljava/time/temporal/ChronoUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "WEEKS",
            "Ljava/time/temporal/ChronoUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MONTHS",
            "Ljava/time/temporal/ChronoUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "YEARS",
            "Ljava/time/temporal/ChronoUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "DECADES",
            "Ljava/time/temporal/ChronoUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "CENTURIES",
            "Ljava/time/temporal/ChronoUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MILLENNIA",
            "Ljava/time/temporal/ChronoUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ERAS",
            "Ljava/time/temporal/ChronoUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "FOREVER",
            "Ljava/time/temporal/ChronoUnit;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &CHRONO_UNIT_METHODS,
    Some(chrono_unit_clinit),
);

// -------------------------------------------------------------------------------------------
// ChronoField
// -------------------------------------------------------------------------------------------

/// (name, baseUnit ordinal, rangeUnit ordinal, min, maxLow, maxHigh, dateBased, timeBased).
type ChronoFieldEntry = (&'static str, i32, i32, i64, i64, i64, bool, bool);

const CHRONO_FIELDS: [ChronoFieldEntry; 30] = [
    (
        "NANO_OF_SECOND",
        0,
        3,
        0,
        999_999_999,
        999_999_999,
        false,
        true,
    ),
    (
        "NANO_OF_DAY",
        0,
        7,
        0,
        86_399_999_999_999,
        86_399_999_999_999,
        false,
        true,
    ),
    ("MICRO_OF_SECOND", 1, 3, 0, 999_999, 999_999, false, true),
    (
        "MICRO_OF_DAY",
        1,
        7,
        0,
        86_399_999_999,
        86_399_999_999,
        false,
        true,
    ),
    ("MILLI_OF_SECOND", 2, 3, 0, 999, 999, false, true),
    ("MILLI_OF_DAY", 2, 7, 0, 86_399_999, 86_399_999, false, true),
    ("SECOND_OF_MINUTE", 3, 4, 0, 59, 59, false, true),
    ("SECOND_OF_DAY", 3, 7, 0, 86_399, 86_399, false, true),
    ("MINUTE_OF_HOUR", 4, 5, 0, 59, 59, false, true),
    ("MINUTE_OF_DAY", 4, 7, 0, 1_439, 1_439, false, true),
    ("HOUR_OF_AMPM", 5, 6, 0, 11, 11, false, true),
    ("CLOCK_HOUR_OF_AMPM", 5, 6, 1, 12, 12, false, true),
    ("HOUR_OF_DAY", 5, 7, 0, 23, 23, false, true),
    ("CLOCK_HOUR_OF_DAY", 5, 7, 1, 24, 24, false, true),
    ("AMPM_OF_DAY", 6, 7, 0, 1, 1, false, true),
    ("DAY_OF_WEEK", 7, 8, 1, 7, 7, true, false),
    ("ALIGNED_DAY_OF_WEEK_IN_MONTH", 7, 8, 1, 7, 7, true, false),
    ("ALIGNED_DAY_OF_WEEK_IN_YEAR", 7, 8, 1, 7, 7, true, false),
    ("DAY_OF_MONTH", 7, 9, 1, 28, 31, true, false),
    ("DAY_OF_YEAR", 7, 10, 1, 365, 366, true, false),
    (
        "EPOCH_DAY",
        7,
        15,
        -365_243_219_162,
        365_241_780_471,
        365_241_780_471,
        true,
        false,
    ),
    ("ALIGNED_WEEK_OF_MONTH", 8, 9, 1, 4, 5, true, false),
    ("ALIGNED_WEEK_OF_YEAR", 8, 10, 1, 52, 53, true, false),
    ("MONTH_OF_YEAR", 9, 10, 1, 12, 12, true, false),
    (
        "PROLEPTIC_MONTH",
        9,
        15,
        -11_999_999_998,
        11_999_999_999,
        11_999_999_999,
        true,
        false,
    ),
    (
        "YEAR_OF_ERA",
        10,
        15,
        1,
        999_999_999,
        1_000_000_000,
        true,
        false,
    ),
    (
        "YEAR",
        10,
        15,
        -999_999_999,
        999_999_999,
        999_999_999,
        true,
        false,
    ),
    ("ERA", 14, 15, 0, 1, 1, true, false),
    (
        "INSTANT_SECONDS",
        3,
        15,
        -31_557_014_167_219_200,
        31_556_889_864_403_199,
        31_556_889_864_403_199,
        false,
        true,
    ),
    (
        "OFFSET_SECONDS",
        3,
        15,
        -64_800,
        64_800,
        64_800,
        false,
        true,
    ),
];

fn chrono_field_new(vm: &mut Vm<'_>, ordinal: i32) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/time/temporal/ChronoField")?;
    let instance = vm.new_instance(class)?;
    write_int_field(vm, instance, "ordinal", ordinal);
    Ok(instance)
}

fn chrono_field_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/time/temporal/ChronoField")?;
    for (index, (name, ..)) in CHRONO_FIELDS.iter().enumerate() {
        let instance = chrono_field_new(vm, index as i32)?;
        vm.set_static_value(
            class,
            name,
            "Ljava/time/temporal/ChronoField;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

fn chrono_field_entry(vm: &Vm<'_>, this: ObjectRef) -> ChronoFieldEntry {
    let ordinal = read_int_field(vm, this, "ordinal").clamp(0, 29) as usize;
    CHRONO_FIELDS[ordinal]
}

fn chrono_unit_constant(vm: &mut Vm<'_>, ordinal: i32) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/time/temporal/ChronoUnit")?;
    vm.ensure_initialized(class)?;
    let name = CHRONO_UNITS
        .get(ordinal as usize)
        .map(|e| e.0)
        .unwrap_or("NANOS");
    let value = vm
        .read_static_ref_if_set(class, name, "Ljava/time/temporal/ChronoUnit;")
        .ok_or_else(|| VmError::internal("ChronoUnit constants not initialized"))?;
    object(value)
}

const CHRONO_FIELD_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "ordinal"))
    }),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        object(vm.make_string(chrono_field_entry(vm, receiver(args)).0)?)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(vm.make_string(chrono_field_entry(vm, receiver(args)).0)?),
    ),
    method(
        "getBaseUnit",
        "()Ljava/time/temporal/TemporalUnit;",
        ACC_PUBLIC,
        |vm, _, args| chrono_unit_constant(vm, chrono_field_entry(vm, receiver(args)).1),
    ),
    method(
        "getRangeUnit",
        "()Ljava/time/temporal/TemporalUnit;",
        ACC_PUBLIC,
        |vm, _, args| chrono_unit_constant(vm, chrono_field_entry(vm, receiver(args)).2),
    ),
    method(
        "range",
        "()Ljava/time/temporal/ValueRange;",
        ACC_PUBLIC,
        |vm, _, args| {
            let entry = chrono_field_entry(vm, receiver(args));
            object(value_range_new(vm, entry.3, entry.3, entry.4, entry.5)?)
        },
    ),
    method("isDateBased", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(chrono_field_entry(vm, receiver(args)).6)
    }),
    method("isTimeBased", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(chrono_field_entry(vm, receiver(args)).7)
    }),
    method(
        "isSupportedBy",
        "(Ljava/time/temporal/TemporalAccessor;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let accessor = ref_arg(args, 1);
            let (class, method) = vm.resolve_virtual_method(
                accessor,
                "isSupported",
                "(Ljava/time/temporal/TemporalField;)Z",
            )?;
            vm.invoke_method(class, method, vec![Value::Ref(accessor), args[0]])
        },
    ),
    method(
        "getFrom",
        "(Ljava/time/temporal/TemporalAccessor;)J",
        ACC_PUBLIC,
        |vm, _, args| {
            let accessor = ref_arg(args, 1);
            let (class, method) = vm.resolve_virtual_method(
                accessor,
                "getLong",
                "(Ljava/time/temporal/TemporalField;)J",
            )?;
            vm.invoke_method(class, method, vec![Value::Ref(accessor), args[0]])
        },
    ),
    method(
        "rangeRefinedBy",
        "(Ljava/time/temporal/TemporalAccessor;)Ljava/time/temporal/ValueRange;",
        ACC_PUBLIC,
        |vm, _, args| {
            let accessor = ref_arg(args, 1);
            let (class, method) = vm.resolve_virtual_method(
                accessor,
                "range",
                "(Ljava/time/temporal/TemporalField;)Ljava/time/temporal/ValueRange;",
            )?;
            vm.invoke_method(class, method, vec![Value::Ref(accessor), args[0]])
        },
    ),
    method(
        "values",
        "()[Ljava/time/temporal/ChronoField;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/time/temporal/ChronoField")?;
            vm.ensure_initialized(class)?;
            let array = vm.allocate_object_array(class, CHRONO_FIELDS.len())?;
            for (index, (name, ..)) in CHRONO_FIELDS.iter().enumerate() {
                if let Some(value) =
                    vm.read_static_ref_if_set(class, name, "Ljava/time/temporal/ChronoField;")
                {
                    vm.array_set_ref(array, index, value)?;
                }
            }
            object(array)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/time/temporal/ChronoField;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 0));
            let class = vm.resolve_class("java/time/temporal/ChronoField")?;
            vm.ensure_initialized(class)?;
            for candidate in CHRONO_FIELDS {
                if candidate.0 == name {
                    if let Some(value) = vm.read_static_ref_if_set(
                        class,
                        candidate.0,
                        "Ljava/time/temporal/ChronoField;",
                    ) {
                        return object(value);
                    }
                }
            }
            Err(vm.throw_new("java/lang/IllegalArgumentException", None))
        },
    ),
    method(
        "getDisplayName",
        "(Ljava/util/Locale;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let ordinal = read_int_field(vm, receiver(args), "ordinal").clamp(0, 29) as usize;
            let name = match ordinal {
                23 => "month",
                18 => "day of month",
                15 => "day of week",
                26 => "year",
                27 => "era",
                14 => "AM/PM",
                12 => "hour",
                8 => "minute",
                6 => "second",
                _ => CHRONO_FIELDS[ordinal].0,
            };
            object(vm.make_string(name)?)
        },
    ),
];

pub(crate) const CHRONO_FIELD: oxjvm_vm::NativeClass = class(
    "java/time/temporal/ChronoField",
    Some("java/lang/Object"),
    &["java/time/temporal/TemporalField", "java/lang/Comparable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "NANO_OF_SECOND",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "NANO_OF_DAY",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MICRO_OF_SECOND",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MICRO_OF_DAY",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MILLI_OF_SECOND",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MILLI_OF_DAY",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SECOND_OF_MINUTE",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SECOND_OF_DAY",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MINUTE_OF_HOUR",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MINUTE_OF_DAY",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "HOUR_OF_AMPM",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "CLOCK_HOUR_OF_AMPM",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "HOUR_OF_DAY",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "CLOCK_HOUR_OF_DAY",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "AMPM_OF_DAY",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "DAY_OF_WEEK",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ALIGNED_DAY_OF_WEEK_IN_MONTH",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ALIGNED_DAY_OF_WEEK_IN_YEAR",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "DAY_OF_MONTH",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "DAY_OF_YEAR",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "EPOCH_DAY",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ALIGNED_WEEK_OF_MONTH",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ALIGNED_WEEK_OF_YEAR",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "MONTH_OF_YEAR",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "PROLEPTIC_MONTH",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "YEAR_OF_ERA",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "YEAR",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ERA",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "INSTANT_SECONDS",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "OFFSET_SECONDS",
            "Ljava/time/temporal/ChronoField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &CHRONO_FIELD_METHODS,
    Some(chrono_field_clinit),
);

// -------------------------------------------------------------------------------------------
// UnsupportedTemporalTypeException
// -------------------------------------------------------------------------------------------

const UNSUPPORTED_TEMPORAL_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| crate::void()),
    method("<init>", "(Ljava/lang/String;)V", ACC_PUBLIC, |_, _, _| {
        crate::void()
    }),
];

pub(crate) const UNSUPPORTED_TEMPORAL_TYPE_EXCEPTION: oxjvm_vm::NativeClass = class(
    "java/time/temporal/UnsupportedTemporalTypeException",
    Some("java/time/DateTimeException"),
    &[],
    ACC_PUBLIC,
    &[],
    &UNSUPPORTED_TEMPORAL_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// IsoFields
// -------------------------------------------------------------------------------------------

const ISO_FIELD_NAMES: [&str; 4] = [
    "DAY_OF_QUARTER",
    "QUARTER_OF_YEAR",
    "WEEK_OF_WEEK_BASED_YEAR",
    "WEEK_BASED_YEAR",
];

fn iso_field_new(vm: &mut Vm<'_>, ordinal: i32) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/time/temporal/IsoFields$Field")?;
    let instance = vm.new_instance(class)?;
    write_int_field(vm, instance, "ordinal", ordinal);
    Ok(instance)
}

fn iso_fields_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/time/temporal/IsoFields")?;
    for (index, name) in ISO_FIELD_NAMES.iter().enumerate() {
        let instance = iso_field_new(vm, index as i32)?;
        vm.set_static_value(
            class,
            name,
            "Ljava/time/temporal/TemporalField;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

const ISO_FIELDS_FIELD_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "ordinal"))
    }),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let ordinal = read_int_field(vm, receiver(args), "ordinal").clamp(0, 3) as usize;
        object(vm.make_string(ISO_FIELD_NAMES[ordinal])?)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let ordinal = read_int_field(vm, receiver(args), "ordinal").clamp(0, 3) as usize;
            object(vm.make_string(ISO_FIELD_NAMES[ordinal])?)
        },
    ),
    method(
        "getDisplayName",
        "(Ljava/util/Locale;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let ordinal = read_int_field(vm, receiver(args), "ordinal");
            let name = if ordinal == 2 {
                "week"
            } else {
                ISO_FIELD_NAMES[ordinal.clamp(0, 3) as usize]
            };
            object(vm.make_string(name)?)
        },
    ),
];

pub(crate) const ISO_FIELDS_FIELD: oxjvm_vm::NativeClass = class(
    "java/time/temporal/IsoFields$Field",
    Some("java/lang/Object"),
    &["java/time/temporal/TemporalField"],
    ACC_PUBLIC | ACC_FINAL,
    &[field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None)],
    &ISO_FIELDS_FIELD_METHODS,
    None,
);

pub(crate) const ISO_FIELDS: oxjvm_vm::NativeClass = class(
    "java/time/temporal/IsoFields",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field(
            "DAY_OF_QUARTER",
            "Ljava/time/temporal/TemporalField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "QUARTER_OF_YEAR",
            "Ljava/time/temporal/TemporalField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "WEEK_OF_WEEK_BASED_YEAR",
            "Ljava/time/temporal/TemporalField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "WEEK_BASED_YEAR",
            "Ljava/time/temporal/TemporalField;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &[],
    Some(iso_fields_clinit),
);

// -------------------------------------------------------------------------------------------
// WeekFields
// -------------------------------------------------------------------------------------------

fn week_fields_computed_new(vm: &mut Vm<'_>, kind: i32) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/time/temporal/WeekFields$ComputedDayOfField")?;
    let instance = vm.new_instance(class)?;
    write_int_field(vm, instance, "kind", kind);
    Ok(instance)
}

const WEEK_FIELDS_COMPUTED_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("ComputedDayOfField")?),
    ),
    method(
        "getDisplayName",
        "(Ljava/util/Locale;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let kind = read_int_field(vm, receiver(args), "kind");
            let name = match kind {
                2 => "week",
                0 => "day of week",
                1 => "week of month",
                _ => "week based year",
            };
            object(vm.make_string(name)?)
        },
    ),
];

pub(crate) const WEEK_FIELDS_COMPUTED_DAY_OF_FIELD: oxjvm_vm::NativeClass = class(
    "java/time/temporal/WeekFields$ComputedDayOfField",
    Some("java/lang/Object"),
    &["java/time/temporal/TemporalField"],
    ACC_PUBLIC | ACC_FINAL,
    &[field("kind", "I", ACC_PRIVATE | ACC_FINAL, None)],
    &WEEK_FIELDS_COMPUTED_METHODS,
    None,
);

fn week_fields_new(vm: &mut Vm<'_>, first_day: i32) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/time/temporal/WeekFields")?;
    let instance = vm.new_instance(class)?;
    write_int_field(vm, instance, "firstDay", first_day);
    Ok(instance)
}

fn week_fields_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/time/temporal/WeekFields")?;
    for (name, first_day) in [("SUNDAY_START", 7), ("ISO", 1)] {
        let instance = week_fields_new(vm, first_day)?;
        vm.set_static_value(
            class,
            name,
            "Ljava/time/temporal/WeekFields;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

const WEEK_FIELDS_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "of",
        "(Ljava/time/DayOfWeek;I)Ljava/time/temporal/WeekFields;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let day = read_int_field(vm, ref_arg(args, 0), "value");
            object(week_fields_new(vm, day)?)
        },
    ),
    method(
        "weekOfYear",
        "()Ljava/time/temporal/TemporalField;",
        ACC_PUBLIC,
        |vm, _, _| object(week_fields_computed_new(vm, 2)?),
    ),
];

pub(crate) const WEEK_FIELDS: oxjvm_vm::NativeClass = class(
    "java/time/temporal/WeekFields",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("firstDay", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "SUNDAY_START",
            "Ljava/time/temporal/WeekFields;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ISO",
            "Ljava/time/temporal/WeekFields;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &WEEK_FIELDS_METHODS,
    Some(week_fields_clinit),
);
