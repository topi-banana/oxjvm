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

/// Whether `index` is a word boundary: letters/digits form one segment,
/// whitespace runs and punctuation form separate segments (as in the JDK).
fn word_boundary(units: &[u16], index: usize) -> bool {
    let length = units.len();
    if index == 0 || index > length {
        return false;
    }
    if index == length {
        return true;
    }
    word_class(units, index - 1) != word_class(units, index)
}

/// Letters and digits share a class; an apostrophe between letters stays in
/// the word; everything else (whitespace, punctuation) is its own class.
fn word_class(units: &[u16], index: usize) -> u8 {
    let ch = char::from_u32(u32::from(units[index]));
    if ch.is_some_and(|c| c.is_alphanumeric()) {
        return 1;
    }
    if ch == Some('\'') {
        let before = index
            .checked_sub(1)
            .and_then(|i| char::from_u32(u32::from(units[i])));
        let after = char::from_u32(u32::from(units[index + 1]));
        if before.is_some_and(|c| c.is_alphabetic()) && after.is_some_and(|c| c.is_alphabetic()) {
            return 1;
        }
    }
    0
}

/// Whether `index` is a character boundary: every code point, with surrogate
/// pairs kept together.
fn character_boundary(units: &[u16], index: usize) -> bool {
    let length = units.len();
    if index > length {
        return false;
    }
    if index == length {
        return true;
    }
    !(0xDC00..=0xDFFF).contains(&units[index])
}

/// Whether `index` is a sentence boundary: after `.`, `!` or `?` followed by
/// whitespace or the end of text, and after hard line breaks.
fn sentence_boundary(units: &[u16], index: usize) -> bool {
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
        Some('\n') | Some('\r') => true,
        Some('.') | Some('!') | Some('?') => {
            index == length || current.is_some_and(|ch| ch.is_whitespace())
        }
        _ => false,
    }
}

/// Boundary predicate for the iterator kind (0 line, 1 word, 2 character,
/// 3 sentence).
fn boundary_at(kind: i32, units: &[u16], index: usize) -> bool {
    match kind {
        1 => word_boundary(units, index),
        2 => character_boundary(units, index),
        3 => sentence_boundary(units, index),
        _ => line_boundary(units, index),
    }
}

fn break_iterator_next_from(kind: i32, units: &[u16], from: i32) -> i32 {
    let length = units.len() as i32;
    let mut index = from + 1;
    while index < length && !boundary_at(kind, units, index as usize) {
        index += 1;
    }
    if index > length { DONE } else { index }
}

fn break_iterator_previous_from(kind: i32, units: &[u16], from: i32) -> i32 {
    if from <= 0 {
        return DONE;
    }
    let mut index = from - 1;
    while index > 0 && !boundary_at(kind, units, index as usize) {
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

fn new_break_iterator(vm: &mut Vm<'_>, kind: i32) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/text/BreakIterator")?;
    let instance = vm.new_instance(class)?;
    set_break_iterator_index(vm, instance, 0);
    write_int_field(vm, instance, "kind", kind);
    object(instance)
}

const BREAK_ITERATOR_METHODS: [oxjvm_vm::NativeMethodDef; 18] = [
    method(
        "getLineInstance",
        "()Ljava/text/BreakIterator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| new_break_iterator(vm, 0),
    ),
    method(
        "getWordInstance",
        "()Ljava/text/BreakIterator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| new_break_iterator(vm, 1),
    ),
    method(
        "getCharacterInstance",
        "()Ljava/text/BreakIterator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| new_break_iterator(vm, 2),
    ),
    method(
        "getSentenceInstance",
        "()Ljava/text/BreakIterator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| new_break_iterator(vm, 3),
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
        let kind = read_int_field(vm, this, "kind");
        let index = break_iterator_next_from(kind, &units, break_iterator_index(vm, this));
        set_break_iterator_index(vm, this, index.max(0));
        int(index)
    }),
    method("previous", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let units = break_iterator_units(vm, this);
        let kind = read_int_field(vm, this, "kind");
        let index = break_iterator_previous_from(kind, &units, break_iterator_index(vm, this));
        set_break_iterator_index(vm, this, index.max(0));
        int(index)
    }),
    method("current", "()I", ACC_PUBLIC, |vm, _, args| {
        int(break_iterator_index(vm, receiver(args)))
    }),
    method("isBoundary", "(I)Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let units = break_iterator_units(vm, this);
        let offset = int_arg(args, 1);
        if offset < 0 || offset as usize > units.len() {
            return crate::boolean(false);
        }
        let kind = read_int_field(vm, this, "kind");
        crate::boolean(offset == 0 || boundary_at(kind, &units, offset as usize))
    }),
    method("following", "(I)I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let units = break_iterator_units(vm, this);
        let offset = int_arg(args, 1);
        let kind = read_int_field(vm, this, "kind");
        let mut index = offset + 1;
        while index <= units.len() as i32 && !boundary_at(kind, &units, index as usize) {
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
        let kind = read_int_field(vm, this, "kind");
        let index = break_iterator_previous_from(kind, &units, offset);
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
    method(
        "getLineInstance",
        "(Ljava/util/Locale;)Ljava/text/BreakIterator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            new_break_iterator(vm, 0)
        },
    ),
    method(
        "getWordInstance",
        "(Ljava/util/Locale;)Ljava/text/BreakIterator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            new_break_iterator(vm, 1)
        },
    ),
    method(
        "getCharacterInstance",
        "(Ljava/util/Locale;)Ljava/text/BreakIterator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            new_break_iterator(vm, 2)
        },
    ),
    method(
        "getSentenceInstance",
        "(Ljava/util/Locale;)Ljava/text/BreakIterator;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            new_break_iterator(vm, 3)
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
        field("kind", "I", ACC_PRIVATE, None),
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
                } else if run == 1 {
                    out.push_str(&alloc::format!("{year}"));
                } else {
                    out.push_str(&alloc::format!("{:0width$}", year, width = run));
                }
            }
            'M' if run >= 3 => {
                // Three letters name the month, four name it in full.
                out.push_str(month_name(month, run >= 4));
            }
            'M' => push_number(&mut out, month, run),
            'd' => push_number(&mut out, day, run),
            'H' => push_number(&mut out, hour, run),
            'h' => {
                let twelve = match hour % 12 {
                    0 => 12,
                    other => other,
                };
                push_number(&mut out, twelve, run);
            }
            'm' => push_number(&mut out, minute, run),
            's' => push_number(&mut out, second, run),
            'S' => push_number(&mut out, milli, run),
            'a' => out.push_str(if hour < 12 { "AM" } else { "PM" }),
            'E' => out.push_str(day_name(millis, run >= 4)),
            'D' => {
                let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
                const CUMULATIVE: [i64; 12] =
                    [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
                let day_of_year =
                    CUMULATIVE[(month - 1) as usize] + day + i64::from(leap && month > 2);
                push_number(&mut out, day_of_year, run);
            }
            'Q' => {
                let quarter = (month - 1) / 3 + 1;
                if run >= 4 {
                    out.push_str(match quarter {
                        1 => "1st quarter",
                        2 => "2nd quarter",
                        3 => "3rd quarter",
                        _ => "4th quarter",
                    });
                } else if run == 3 {
                    out.push_str(&alloc::format!("Q{quarter}"));
                } else {
                    push_number(&mut out, quarter, run);
                }
            }
            'z' => out.push_str("UTC"),
            'Z' => out.push_str("UTC"),
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

/// The name of a month, in full or abbreviated.
pub(crate) fn month_name(month: i64, full: bool) -> &'static str {
    const SHORT: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    const FULL: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let index = (month - 1).clamp(0, 11) as usize;
    if full { FULL[index] } else { SHORT[index] }
}

/// The name of the day of the week, in full or abbreviated.
pub(crate) fn day_name(millis: i64, full: bool) -> &'static str {
    const SHORT: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const FULL: [&str; 7] = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];
    // The epoch itself was a Thursday.
    let days = millis.div_euclid(86_400_000);
    let weekday = (days + 4).rem_euclid(7) as usize;
    if full { FULL[weekday] } else { SHORT[weekday] }
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

/// Whether this format parses strictly (`setLenient(false)`). The flag is
/// stored inverted so the zero-initialized field means the JDK default of
/// lenient parsing.
fn sdf_non_lenient(vm: &Vm<'_>, this: ObjectRef) -> bool {
    let class = vm.class_of(this);
    vm.read_instance_int_named(this, class, "nonLenient", "Z")
        .unwrap_or(0)
        != 0
}

/// The year the two-digit-year window opens at (`set2DigitYearStart`),
/// defaulting to eighty years before the format was created.
fn sdf_two_digit_start_year(vm: &Vm<'_>, this: ObjectRef) -> i64 {
    let class = vm.class_of(this);
    let start = vm
        .read_ref_field(this, class, "twoDigitYearStart", "Ljava/util/Date;")
        .unwrap_or(ObjectRef::NULL);
    if start.is_null() {
        return 1970;
    }
    let millis = crate::lang::read_long_field(vm, start, "time");
    civil_from_millis(millis).0
}

/// In strict mode the JDK rejects a numeric field whose value falls outside
/// the range its pattern letter allows, reporting the index just past the
/// offending number. Returns that error index, or `None` when every numeric
/// field is in range (or the text does not line up with the pattern closely
/// enough to judge, leaving the value parse in charge).
fn non_lenient_field_error(pattern: &str, text: &str) -> Option<usize> {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let mut pi = 0;
    let mut ti = 0;
    while pi < p.len() {
        let ch = p[pi];
        if ch == '\'' {
            pi += 1;
            if pi < p.len() && p[pi] == '\'' {
                if ti < t.len() && t[ti] == '\'' {
                    ti += 1;
                } else {
                    return None;
                }
                pi += 1;
                continue;
            }
            while pi < p.len() && p[pi] != '\'' {
                if ti < t.len() && t[ti] == p[pi] {
                    ti += 1;
                } else {
                    return None;
                }
                pi += 1;
            }
            pi += 1;
            continue;
        }
        let mut run = 1;
        while pi + run < p.len() && p[pi + run] == ch {
            run += 1;
        }
        let numeric = matches!(
            ch,
            'y' | 'd' | 'H' | 'h' | 'k' | 'K' | 'm' | 's' | 'S' | 'D'
        ) || (ch == 'M' && run < 3)
            || (ch == 'Q' && run < 3);
        if numeric {
            let mut neg = false;
            if ti < t.len() && (t[ti] == '-' || t[ti] == '+') {
                neg = t[ti] == '-';
                ti += 1;
            }
            let digits_start = ti;
            let mut value: i64 = 0;
            while ti < t.len() && t[ti].is_ascii_digit() {
                value = value * 10 + i64::from(t[ti] as u32 - '0' as u32);
                ti += 1;
            }
            if ti == digits_start {
                return None;
            }
            if neg {
                value = -value;
            }
            let (lo, hi) = match ch {
                'h' => (1, 12),
                'k' => (1, 24),
                'H' => (0, 23),
                'K' => (0, 11),
                'm' | 's' => (0, 59),
                'M' => (1, 12),
                'd' => (1, 31),
                'S' => (0, 999),
                'D' => (1, 366),
                _ => (i64::MIN, i64::MAX),
            };
            if value < lo || value > hi {
                return Some(ti);
            }
        } else if matches!(ch, 'a' | 'E' | 'z' | 'Z')
            || (ch == 'M' && run >= 3)
            || (ch == 'Q' && run >= 3)
        {
            while ti < t.len() && t[ti].is_alphabetic() {
                ti += 1;
            }
        } else {
            for _ in 0..run {
                if ti < t.len() && t[ti] == ch {
                    ti += 1;
                } else {
                    return None;
                }
            }
        }
        pi += run;
    }
    None
}

/// Pattern-aware parse for `SimpleDateFormat`: extract each field the pattern
/// names (including the AM/PM marker, which the loose numeric scan ignores) so
/// times of day parse correctly. Falls back to the loose scan when the pattern
/// yields no field at all.
fn sdf_parse_millis(pattern: &str, text: &str, two_digit_start: i64) -> i64 {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let mut pi = 0;
    let mut ti = 0;
    let mut year: Option<i64> = None;
    let mut month: Option<i64> = None;
    let mut day: Option<i64> = None;
    let mut hour: Option<i64> = None;
    let mut minute: Option<i64> = None;
    let mut second: Option<i64> = None;
    let mut milli: Option<i64> = None;
    let mut pm: Option<bool> = None;
    let mut twelve_hour = false;
    let mut any = false;
    // Read a signed integer at the current input position, if any. Lenient
    // parsing skips whitespace before a numeric field.
    fn read_number(t: &[char], ti: &mut usize) -> Option<i64> {
        while *ti < t.len() && t[*ti].is_whitespace() {
            *ti += 1;
        }
        let mut neg = false;
        if *ti < t.len() && (t[*ti] == '-' || t[*ti] == '+') {
            neg = t[*ti] == '-';
            *ti += 1;
        }
        let start = *ti;
        let mut value: i64 = 0;
        while *ti < t.len() && t[*ti].is_ascii_digit() {
            value = value * 10 + i64::from(t[*ti] as u32 - '0' as u32);
            *ti += 1;
        }
        if *ti == start {
            return None;
        }
        Some(if neg { -value } else { value })
    }
    while pi < p.len() {
        let ch = p[pi];
        if ch == '\'' {
            pi += 1;
            if pi < p.len() && p[pi] == '\'' {
                if ti < t.len() && t[ti] == '\'' {
                    ti += 1;
                }
                pi += 1;
                continue;
            }
            while pi < p.len() && p[pi] != '\'' {
                if ti < t.len() && t[ti] == p[pi] {
                    ti += 1;
                }
                pi += 1;
            }
            pi += 1;
            continue;
        }
        let mut run = 1;
        while pi + run < p.len() && p[pi + run] == ch {
            run += 1;
        }
        let numeric = matches!(
            ch,
            'y' | 'd' | 'H' | 'h' | 'k' | 'K' | 'm' | 's' | 'S' | 'D'
        ) || (ch == 'M' && run < 3)
            || (ch == 'Q' && run < 3);
        if numeric {
            if let Some(value) = read_number(&t, &mut ti) {
                any = true;
                match ch {
                    'y' => {
                        year = Some(if run == 2 && (0..100).contains(&value) {
                            let century = two_digit_start - two_digit_start.rem_euclid(100);
                            let mut y = century + value;
                            if y < two_digit_start {
                                y += 100;
                            }
                            y
                        } else {
                            value
                        });
                    }
                    'M' => month = Some(value),
                    'd' => day = Some(value),
                    'H' | 'k' => hour = Some(value),
                    'h' | 'K' => {
                        hour = Some(value);
                        twelve_hour = true;
                    }
                    'm' => minute = Some(value),
                    's' => second = Some(value),
                    'S' => milli = Some(value),
                    _ => {}
                }
            }
        } else if ch == 'a' {
            let rest: String = t[ti..].iter().collect();
            let upper = rest.to_ascii_uppercase();
            if upper.starts_with("PM") {
                pm = Some(true);
                ti += 2;
                any = true;
            } else if upper.starts_with("AM") {
                pm = Some(false);
                ti += 2;
                any = true;
            }
        } else if ch == 'M' && run >= 3 {
            let rest: String = t[ti..].iter().collect();
            for m in 1..=12 {
                let full = month_name(m, true);
                let short = month_name(m, false);
                if rest.starts_with(full) {
                    month = Some(m);
                    ti += full.chars().count();
                    any = true;
                    break;
                }
                if rest.starts_with(short) {
                    month = Some(m);
                    ti += short.chars().count();
                    any = true;
                    break;
                }
            }
        } else if matches!(ch, 'E' | 'z' | 'Z') || (ch == 'Q' && run >= 3) {
            while ti < t.len() && t[ti].is_alphabetic() {
                ti += 1;
            }
        } else {
            for _ in 0..run {
                if ti < t.len() && t[ti] == ch {
                    ti += 1;
                }
            }
        }
        pi += run;
    }
    if !any {
        return crate::format::parse_date_text(text);
    }
    let hour = match (hour, pm) {
        (None, Some(is_pm)) => i64::from(is_pm) * 12,
        (Some(h), Some(is_pm)) if twelve_hour => h % 12 + i64::from(is_pm) * 12,
        (Some(h), _) => h,
        (None, None) => 0,
    };
    crate::format::millis_from_civil(
        year.unwrap_or(1970),
        month.unwrap_or(1),
        day.unwrap_or(1),
        hour,
        minute.unwrap_or(0),
        second.unwrap_or(0),
    ) + milli.unwrap_or(0)
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

const SIMPLE_DATE_FORMAT_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let (declaring, field) =
            vm.find_field(vm.class_of(this), "pattern", "Ljava/lang/String;")?;
        let text = vm.make_string("yyyy-MM-dd HH:mm:ss")?;
        vm.set_instance_ref(this, declaring, field, text);
        let millis = vm.host_time_millis();
        let start = date_of_millis(vm, millis - 2_522_880_000_000)?;
        let (declaring, field) =
            vm.find_field(vm.class_of(this), "twoDigitYearStart", "Ljava/util/Date;")?;
        vm.set_instance_ref(this, declaring, field, start);
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
        "<init>",
        "(Ljava/lang/String;Ljava/text/DateFormatSymbols;)V",
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
    method(
        "parse",
        "(Ljava/lang/String;)Ljava/util/Date;",
        ACC_PUBLIC,
        |vm, _, args| {
            // Actually arg is string ref
            let input = vm.string_or_empty(ref_arg(args, 1));
            if sdf_non_lenient(vm, receiver(args)) {
                let pattern = simple_date_format_pattern(vm, receiver(args));
                if non_lenient_field_error(&pattern, &input).is_some() {
                    return Err(vm.throw_new("java/text/ParseException", Some("Unparsable date")));
                }
            }
            let pattern = simple_date_format_pattern(vm, receiver(args));
            let millis = sdf_parse_millis(
                &pattern,
                &input,
                sdf_two_digit_start_year(vm, receiver(args)),
            );
            let class = vm.resolve_class("java/util/Date")?;
            let instance = vm.new_instance(class)?;
            crate::lang::write_long_field(vm, instance, "time", millis);
            object(instance)
        },
    ),
    method(
        "parse",
        "(Ljava/lang/String;Ljava/text/ParsePosition;)Ljava/util/Date;",
        ACC_PUBLIC,
        |vm, _, args| {
            let position = ref_arg(args, 2);
            let input = vm.string_or_empty(ref_arg(args, 1));
            if input.is_empty() {
                write_int_field(vm, position, "errorIndex", 0);
                return object(ObjectRef::NULL);
            }
            if sdf_non_lenient(vm, receiver(args)) {
                let pattern = simple_date_format_pattern(vm, receiver(args));
                if let Some(error) = non_lenient_field_error(&pattern, &input) {
                    write_int_field(vm, position, "errorIndex", error as i32);
                    return object(ObjectRef::NULL);
                }
            }
            let pattern = simple_date_format_pattern(vm, receiver(args));
            let millis = sdf_parse_millis(
                &pattern,
                &input,
                sdf_two_digit_start_year(vm, receiver(args)),
            );
            let class = vm.resolve_class("java/util/Date")?;
            let instance = vm.new_instance(class)?;
            crate::lang::write_long_field(vm, instance, "time", millis);
            write_int_field(vm, position, "index", input.chars().count() as i32);
            object(instance)
        },
    ),
    method("setLenient", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        if let Some((declaring, field)) = vm.find_field_opt(class, "nonLenient", "Z") {
            vm.set_instance_int(this, declaring, field, i32::from(!crate::bool_arg(args, 1)));
        }
        void()
    }),
    method("isLenient", "()Z", ACC_PUBLIC, |vm, _, args| {
        crate::boolean(!sdf_non_lenient(vm, receiver(args)))
    }),
    method(
        "set2DigitYearStart",
        "(Ljava/util/Date;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let start = ref_arg(args, 1);
            if start.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "twoDigitYearStart", "Ljava/util/Date;")?;
            vm.set_instance_ref(this, declaring, field, start);
            void()
        },
    ),
    method(
        "get2DigitYearStart",
        "()Ljava/util/Date;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let start = vm
                .read_ref_field(
                    this,
                    vm.class_of(this),
                    "twoDigitYearStart",
                    "Ljava/util/Date;",
                )
                .unwrap_or(ObjectRef::NULL);
            object(start)
        },
    ),
    method(
        "toLocalizedPattern",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(vm.make_string(&simple_date_format_pattern(vm, receiver(args)))?),
    ),
];

fn date_of_millis(vm: &mut Vm<'_>, millis: i64) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/Date")?;
    let instance = vm.new_instance(class)?;
    crate::lang::write_long_field(vm, instance, "time", millis);
    Ok(instance)
}

fn simple_date_format_init(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let this = receiver(args);
    if ref_arg(args, 1).is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    if args.len() > 2 && ref_arg(args, 2).is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (declaring, field) = vm.find_field(vm.class_of(this), "pattern", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
    let millis = vm.host_time_millis();
    let start = date_of_millis(vm, millis - 2_522_880_000_000)?;
    let (declaring, field) =
        vm.find_field(vm.class_of(this), "twoDigitYearStart", "Ljava/util/Date;")?;
    vm.set_instance_ref(this, declaring, field, start);
    void()
}

/// `java.text.SimpleDateFormat`: pattern formatting in UTC (time zone offsets are ignored).
pub(crate) const SIMPLE_DATE_FORMAT: oxjvm_vm::NativeClass = class(
    "java/text/SimpleDateFormat",
    Some("java/text/DateFormat"),
    &[],
    ACC_PUBLIC,
    &[
        field("pattern", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("timeZone", "Ljava/util/TimeZone;", ACC_PRIVATE, None),
        field("twoDigitYearStart", "Ljava/util/Date;", ACC_PRIVATE, None),
        field("nonLenient", "Z", ACC_PRIVATE, None),
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

// -------------------------------------------------------------------------------------------
// Normalizer
// -------------------------------------------------------------------------------------------

const NORMALIZER_FORM_NAMES: [&str; 4] = ["NFD", "NFC", "NFKD", "NFKC"];

const NORMALIZER_FORM_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let class = vm.class_of(receiver(args));
        object(
            vm.make_string(
                &vm.read_string_field(receiver(args), class, "name")
                    .unwrap_or_default(),
            )?,
        )
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let class = vm.class_of(receiver(args));
            object(
                vm.make_string(
                    &vm.read_string_field(receiver(args), class, "name")
                        .unwrap_or_default(),
                )?,
            )
        },
    ),
    method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "ordinal"))
    }),
    method(
        "values",
        "()[Ljava/text/Normalizer$Form;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/text/Normalizer$Form")?;
            vm.ensure_initialized(class)?;
            let array = vm.allocate_object_array(class, NORMALIZER_FORM_NAMES.len())?;
            for (index, name) in NORMALIZER_FORM_NAMES.iter().enumerate() {
                if let Some(value) =
                    vm.read_static_ref_if_set(class, name, "Ljava/text/Normalizer$Form;")
                {
                    vm.array_set_ref(array, index, value)?;
                }
            }
            object(array)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/text/Normalizer$Form;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 0));
            let class = vm.resolve_class("java/text/Normalizer$Form")?;
            vm.ensure_initialized(class)?;
            if NORMALIZER_FORM_NAMES.contains(&name.as_str()) {
                if let Some(value) =
                    vm.read_static_ref_if_set(class, &name, "Ljava/text/Normalizer$Form;")
                {
                    return object(value);
                }
            }
            Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some(&alloc::format!(
                    "No enum constant java.text.Normalizer.Form.{name}"
                )),
            ))
        },
    ),
];

fn normalizer_form_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/text/Normalizer$Form")?;
    for (ordinal, name) in NORMALIZER_FORM_NAMES.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        vm.set_string_field(instance, class, "name", name)?;
        write_int_field(vm, instance, "ordinal", ordinal as i32);
        vm.set_static_value(
            class,
            name,
            "Ljava/text/Normalizer$Form;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

/// `java.text.Normalizer$Form`: the four Unicode normalization forms.
pub(crate) const NORMALIZER_FORM: oxjvm_vm::NativeClass = class(
    "java/text/Normalizer$Form",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("ordinal", "I", ACC_PRIVATE, None),
        field(
            "NFD",
            "Ljava/text/Normalizer$Form;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "NFC",
            "Ljava/text/Normalizer$Form;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "NFKD",
            "Ljava/text/Normalizer$Form;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "NFKC",
            "Ljava/text/Normalizer$Form;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
    ],
    &NORMALIZER_FORM_METHODS,
    Some(normalizer_form_clinit),
);

/// The UTF-16 units of a `CharSequence` argument (via `toString` for non-Strings).
fn normalizer_units(vm: &mut Vm<'_>, sequence: ObjectRef) -> Result<Vec<u16>, VmError> {
    if sequence.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    if let Some(units) = vm.string_utf16(sequence) {
        return Ok(units.to_vec());
    }
    let (class, method) =
        vm.resolve_virtual_method(sequence, "toString", "()Ljava/lang/String;")?;
    let text = vm
        .invoke_method(class, method, vec![Value::Ref(sequence)])?
        .as_ref();
    Ok(vm
        .string_utf16(text)
        .map_or_else(Vec::new, |units| units.to_vec()))
}

const NORMALIZER_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "normalize",
        "(Ljava/lang/CharSequence;Ljava/text/Normalizer$Form;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let units = normalizer_units(vm, ref_arg(args, 0))?;
            // ASCII text is already in every normalization form; the full Unicode
            // normalization tables are not modelled, so non-ASCII passes through.
            let text = String::from_utf16_lossy(&units);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "isNormalized",
        "(Ljava/lang/CharSequence;Ljava/text/Normalizer$Form;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let units = normalizer_units(vm, ref_arg(args, 0))?;
            // Without the Unicode tables, treat ASCII as normalized and report
            // non-ASCII conservatively as normalized too (best effort).
            let _ = units;
            crate::boolean(true)
        },
    ),
];

/// `java.text.Normalizer`: Unicode normalization (ASCII passthrough; tables not modelled).
pub(crate) const NORMALIZER: oxjvm_vm::NativeClass = class(
    "java/text/Normalizer",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &NORMALIZER_METHODS,
    None,
);
