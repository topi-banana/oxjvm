//! `java.text.BreakIterator`: line/word/character boundary iteration.
//!
//! The full Unicode line breaking algorithm is not implemented; boundaries are placed after
//! whitespace runs, hyphens, and slashes (and after mandatory line breaks), which matches the
//! needs of help/usage text wrapping.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::lang::{read_int_field, write_int_field};
use crate::{class, field, int, int_arg, method, object, receiver, ref_arg, void};

const DONE: i32 = -1;

fn break_iterator_text(vm: &Vm<'_>, this: ObjectRef) -> String {
    vm.read_string_field(this, vm.class_of(this), "text")
        .unwrap_or_default()
}

fn break_iterator_units(vm: &Vm<'_>, this: ObjectRef) -> Vec<u16> {
    break_iterator_text(vm, this).encode_utf16().collect()
}

/// Whether `index` is a line break opportunity in `units`.
fn line_boundary(units: &[u16], index: usize) -> bool {
    let length = units.len();
    if index == 0 || index > length {
        return false;
    }
    if index == length {
        return true;
    }
    let previous = char::from_u32(u32::from(units[index - 1]));
    let current = char::from_u32(u32::from(units[index]));
    match previous {
        Some('\n') | Some('\r') => return true,
        Some('-') => return current != Some('-'),
        Some('/') => return current != Some('/'),
        _ => {}
    }
    previous.is_some_and(|ch| ch.is_whitespace()) && !current.is_some_and(|ch| ch.is_whitespace())
}

fn break_iterator_next_from(units: &[u16], from: i32) -> i32 {
    let length = units.len() as i32;
    let mut index = from + 1;
    while index < length && !line_boundary(units, index as usize) {
        index += 1;
    }
    if index > length { DONE } else { index }
}

fn break_iterator_previous_from(units: &[u16], from: i32) -> i32 {
    if from <= 0 {
        return DONE;
    }
    let mut index = from - 1;
    while index > 0 && !line_boundary(units, index as usize) {
        index -= 1;
    }
    index
}

fn break_iterator_index(vm: &Vm<'_>, this: ObjectRef) -> i32 {
    read_int_field(vm, this, "index")
}

fn set_break_iterator_index(vm: &mut Vm<'_>, this: ObjectRef, index: i32) {
    write_int_field(vm, this, "index", index);
}

fn new_break_iterator(vm: &mut Vm<'_>) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/text/BreakIterator")?;
    let instance = vm.new_instance(class)?;
    set_break_iterator_index(vm, instance, 0);
    object(instance)
}

const BREAK_ITERATOR_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    method(
        "getLineInstance",
        "()Ljava/text/BreakIterator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| new_break_iterator(vm),
    ),
    method(
        "getWordInstance",
        "()Ljava/text/BreakIterator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| new_break_iterator(vm),
    ),
    method(
        "getCharacterInstance",
        "()Ljava/text/BreakIterator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| new_break_iterator(vm),
    ),
    method(
        "getSentenceInstance",
        "()Ljava/text/BreakIterator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| new_break_iterator(vm),
    ),
    method(
        "setText",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm.string_or_empty(ref_arg(args, 1));
            let (declaring, slot) =
                vm.find_field(vm.class_of(this), "text", "Ljava/lang/String;")?;
            let string = vm.make_string(&text)?;
            vm.set_instance_ref(this, declaring, slot, string);
            set_break_iterator_index(vm, this, 0);
            void()
        },
    ),
    method("first", "()I", ACC_PUBLIC, |vm, _, args| {
        set_break_iterator_index(vm, receiver(args), 0);
        int(0)
    }),
    method("last", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let length = break_iterator_units(vm, this).len() as i32;
        set_break_iterator_index(vm, this, length);
        int(length)
    }),
    method("next", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let units = break_iterator_units(vm, this);
        let index = break_iterator_next_from(&units, break_iterator_index(vm, this));
        set_break_iterator_index(vm, this, index.max(0));
        int(index)
    }),
    method("previous", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let units = break_iterator_units(vm, this);
        let index = break_iterator_previous_from(&units, break_iterator_index(vm, this));
        set_break_iterator_index(vm, this, index.max(0));
        int(index)
    }),
    method("current", "()I", ACC_PUBLIC, |vm, _, args| {
        int(break_iterator_index(vm, receiver(args)))
    }),
    method("isBoundary", "(I)Z", ACC_PUBLIC, |vm, _, args| {
        let units = break_iterator_units(vm, receiver(args));
        let offset = int_arg(args, 1);
        if offset < 0 || offset as usize > units.len() {
            return crate::boolean(false);
        }
        crate::boolean(offset == 0 || line_boundary(&units, offset as usize))
    }),
    method("following", "(I)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let units = break_iterator_units(vm, this);
        let offset = int_arg(args, 1);
        let mut index = offset + 1;
        while index <= units.len() as i32 && !line_boundary(&units, index as usize) {
            index += 1;
        }
        if index > units.len() as i32 {
            int(DONE)
        } else {
            set_break_iterator_index(vm, this, index);
            int(index)
        }
    }),
    method("preceding", "(I)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let units = break_iterator_units(vm, this);
        let offset = int_arg(args, 1);
        let index = break_iterator_previous_from(&units, offset);
        if index != DONE {
            set_break_iterator_index(vm, this, index);
        }
        int(index)
    }),
    method(
        "setText",
        "(Ljava/text/CharacterIterator;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm.string_or_empty(ref_arg(args, 1));
            let (declaring, slot) =
                vm.find_field(vm.class_of(this), "text", "Ljava/lang/String;")?;
            let string = vm.make_string(&text)?;
            vm.set_instance_ref(this, declaring, slot, string);
            set_break_iterator_index(vm, this, 0);
            void()
        },
    ),
];

/// `java.text.BreakIterator`: a concrete iterator with `text` and `index` state.
pub(crate) const BREAK_ITERATOR: oxjvm_vm::NativeClass = class(
    "java/text/BreakIterator",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field(
            "DONE",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(-1)),
        ),
        field("text", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("index", "I", ACC_PRIVATE, None),
    ],
    &BREAK_ITERATOR_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// SimpleDateFormat
// -------------------------------------------------------------------------------------------

/// Split epoch milliseconds into UTC calendar fields (proleptic Gregorian).
fn civil_from_millis(millis: i64) -> (i64, i64, i64, i64, i64, i64, i64) {
    let days = millis.div_euclid(86_400_000);
    let in_day = millis.rem_euclid(86_400_000);
    let second = in_day / 1000;
    let milli = in_day % 1000;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    if month <= 2 {
        year += 1;
    }
    (
        year,
        month,
        day,
        second / 3600,
        (second / 60) % 60,
        second % 60,
        milli,
    )
}

pub(crate) fn format_pattern(pattern: &str, millis: i64) -> String {
    let (year, month, day, hour, minute, second, milli) = civil_from_millis(millis);
    let mut out = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if ch == '\'' {
            index += 1;
            if index < chars.len() && chars[index] == '\'' {
                out.push('\'');
                index += 1;
                continue;
            }
            while index < chars.len() && chars[index] != '\'' {
                out.push(chars[index]);
                index += 1;
            }
            index += 1;
            continue;
        }
        let mut run = 1;
        while index + run < chars.len() && chars[index + run] == ch {
            run += 1;
        }
        match ch {
            'y' => {
                if run == 2 {
                    out.push_str(&alloc::format!("{:02}", year % 100));
                } else {
                    out.push_str(&alloc::format!("{:04}", year));
                }
            }
            'M' => push_number(&mut out, month, run),
            'd' => push_number(&mut out, day, run),
            'H' => push_number(&mut out, hour, run),
            'm' => push_number(&mut out, minute, run),
            's' => push_number(&mut out, second, run),
            'S' => push_number(&mut out, milli, run),
            _ => {
                for _ in 0..run {
                    out.push(ch);
                }
            }
        }
        index += run;
    }
    out
}

fn push_number(out: &mut String, value: i64, width: usize) {
    if width == 1 {
        out.push_str(&oxjvm_vm::format::long_to_string(value));
    } else {
        out.push_str(&alloc::format!("{:0width$}", value, width = width));
    }
}

fn simple_date_format_pattern(vm: &Vm<'_>, this: ObjectRef) -> String {
    vm.read_string_field(this, vm.class_of(this), "pattern")
        .unwrap_or_default()
}

fn simple_date_format_millis(vm: &mut Vm<'_>, date: ObjectRef) -> Result<i64, VmError> {
    if date.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    // `DateFormat.format(Object)` accepts a `Date` or any `Number` (as a long).
    if let Some(date_class) = vm.resolve_class_lenient_pub("java/util/Date") {
        if vm.is_instance(date, date_class) {
            let (class, method) = vm.resolve_virtual_method(date, "getTime", "()J")?;
            return Ok(vm
                .invoke_method(class, method, vec![Value::Ref(date)])?
                .as_long());
        }
    }
    let class_name = vm.class_name(vm.class_of(date)).to_string();
    match class_name.as_str() {
        "java/lang/Long" => Ok(crate::boxed::read_value_long(vm, date)),
        "java/lang/Integer" | "java/lang/Short" | "java/lang/Byte" => {
            Ok(i64::from(crate::boxed::read_value_int(vm, date)))
        }
        _ => Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("Cannot format given Object as a Date"),
        )),
    }
}

const SIMPLE_DATE_FORMAT_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let (declaring, field) =
            vm.find_field(vm.class_of(this), "pattern", "Ljava/lang/String;")?;
        let text = vm.make_string("yyyy-MM-dd HH:mm:ss")?;
        vm.set_instance_ref(this, declaring, field, text);
        void()
    }),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| simple_date_format_init(vm, args),
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/util/Locale;)V",
        ACC_PUBLIC,
        |vm, _, args| simple_date_format_init(vm, args),
    ),
    method(
        "format",
        "(Ljava/util/Date;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pattern = simple_date_format_pattern(vm, receiver(args));
            let millis = simple_date_format_millis(vm, ref_arg(args, 1))?;
            object(vm.make_string(&format_pattern(&pattern, millis))?)
        },
    ),
    method(
        "format",
        "(Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pattern = simple_date_format_pattern(vm, receiver(args));
            let millis = simple_date_format_millis(vm, ref_arg(args, 1))?;
            object(vm.make_string(&format_pattern(&pattern, millis))?)
        },
    ),
    method(
        "setTimeZone",
        "(Ljava/util/TimeZone;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "timeZone", "Ljava/util/TimeZone;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "getTimeZone",
        "()Ljava/util/TimeZone;",
        ACC_PUBLIC,
        |vm, _, args| {
            let zone = vm
                .read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "timeZone",
                    "Ljava/util/TimeZone;",
                )
                .unwrap_or(ObjectRef::NULL);
            object(zone)
        },
    ),
    method(
        "toPattern",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(vm.make_string(&simple_date_format_pattern(vm, receiver(args)))?),
    ),
    method(
        "applyPattern",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| simple_date_format_init(vm, args),
    ),
];

fn simple_date_format_init(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let this = receiver(args);
    let (declaring, field) = vm.find_field(vm.class_of(this), "pattern", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
    void()
}

/// `java.text.SimpleDateFormat`: pattern formatting in UTC (time zone offsets are ignored).
pub(crate) const SIMPLE_DATE_FORMAT: oxjvm_vm::NativeClass = class(
    "java/text/SimpleDateFormat",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("pattern", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("timeZone", "Ljava/util/TimeZone;", ACC_PRIVATE, None),
    ],
    &SIMPLE_DATE_FORMAT_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// CharacterIterator / StringCharacterIterator
// -------------------------------------------------------------------------------------------

const CHARACTER_ITERATOR_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "first",
        "()C",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "last",
        "()C",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "current",
        "()C",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "next",
        "()C",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "previous",
        "()C",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "setIndex",
        "(I)C",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "getIndex",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

/// `java.text.CharacterIterator`: iteration over a sequence of UTF-16 units.
pub(crate) const CHARACTER_ITERATOR: oxjvm_vm::NativeClass = class(
    "java/text/CharacterIterator",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &CHARACTER_ITERATOR_METHODS,
    None,
);

const STRING_CHARACTER_ITERATOR_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "text", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method("first", "()C", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        write_int_field(vm, this, "index", 0);
        char_at(vm, this, 0)
    }),
    method("last", "()C", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let length = character_iterator_units(vm, this).len();
        if length == 0 {
            return char_at(vm, this, 0);
        }
        write_int_field(vm, this, "index", (length - 1) as i32);
        char_at(vm, this, (length - 1) as i32)
    }),
    method("current", "()C", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let index = read_int_field(vm, this, "index");
        char_at(vm, this, index)
    }),
    method("next", "()C", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let index = read_int_field(vm, this, "index");
        let length = character_iterator_units(vm, this).len();
        if index as usize >= length {
            return int(0xFFFF);
        }
        let next = index + 1;
        if next as usize >= length {
            write_int_field(vm, this, "index", length as i32);
            return int(0xFFFF);
        }
        write_int_field(vm, this, "index", next);
        char_at(vm, this, next)
    }),
    method("previous", "()C", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let index = read_int_field(vm, this, "index");
        if index <= 0 {
            return int(0xFFFF);
        }
        let previous = index - 1;
        write_int_field(vm, this, "index", previous);
        char_at(vm, this, previous)
    }),
    method("setIndex", "(I)C", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let index = int_arg(args, 1);
        let length = character_iterator_units(vm, this).len() as i32;
        if index < 0 || index > length {
            return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
        }
        write_int_field(vm, this, "index", index);
        char_at(vm, this, index)
    }),
    method("getIndex", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "index"))
    }),
    method("getEndIndex", "()I", ACC_PUBLIC, |vm, _, args| {
        int(character_iterator_units(vm, receiver(args)).len() as i32)
    }),
];

fn character_iterator_units(vm: &Vm<'_>, this: ObjectRef) -> Vec<u16> {
    vm.read_ref_field(this, vm.class_of(this), "text", "Ljava/lang/String;")
        .and_then(|text| vm.string_utf16(text))
        .map(<[u16]>::to_vec)
        .unwrap_or_default()
}

fn char_at(vm: &Vm<'_>, this: ObjectRef, index: i32) -> Result<Value, VmError> {
    let units = character_iterator_units(vm, this);
    match units.get(index.max(0) as usize) {
        Some(unit) => int(i32::from(*unit)),
        None => int(0xFFFF),
    }
}

/// `java.text.StringCharacterIterator`: a char iterator over a string.
pub(crate) const STRING_CHARACTER_ITERATOR: oxjvm_vm::NativeClass = class(
    "java/text/StringCharacterIterator",
    Some("java/lang/Object"),
    &["java/text/CharacterIterator"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("text", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("index", "I", ACC_PRIVATE, None),
    ],
    &STRING_CHARACTER_ITERATOR_METHODS,
    None,
);
