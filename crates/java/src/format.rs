//! `java.text.Format` and the formats a message is built from: `java.text.NumberFormat`,
//! `java.text.DateFormat`, `java.text.ChoiceFormat` and `java.text.MessageFormat`.
//!
//! The number formats follow the JDK's pattern syntax (`#,##0.###` and the styles the JDK
//! offers), and the date formats the JDK's styles (`short`, `medium`, `long`, `full`) as well as
//! a pattern of its own.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ArrayComponent, ObjectRef, Value, Vm, VmError};

use crate::lang::{read_int_field, write_int_field};
use crate::{class, field, int, method, object, receiver, ref_arg, void};

// -------------------------------------------------------------------------------------------
// Number patterns
// -------------------------------------------------------------------------------------------

/// The style of a compact number format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Compact {
    Short,
    Long,
}

/// A number pattern, the way the JDK reads one: a prefix, the digits, and a suffix.
#[derive(Debug, Clone)]
struct NumberPattern {
    /// The pattern the format was read from.
    source: String,
    prefix: String,
    suffix: String,
    /// The number of digits between two grouping separators; zero means no grouping.
    grouping: usize,
    min_integer: usize,
    min_fraction: usize,
    max_fraction: usize,
    /// The percent style multiplies the value by a hundred.
    percent: bool,
    /// The compact style writes the value as a multiple of a thousand, a million, …
    compact: Option<Compact>,
}

impl NumberPattern {
    /// The JDK's default number pattern.
    fn default_pattern() -> Self {
        Self::parse("#,##0.###")
    }

    /// The pattern this number format carries.
    fn text(&self) -> String {
        self.source.clone()
    }

    /// Read a pattern such as `#,##0.00`, `0.##%` or `$#,##0`.
    fn parse(pattern: &str) -> Self {
        let chars: Vec<char> = pattern.chars().collect();
        let start = chars
            .iter()
            .position(|ch| ch.is_ascii_digit() || *ch == '#')
            .unwrap_or(0);
        let end = chars
            .iter()
            .rposition(|ch| ch.is_ascii_digit() || *ch == '#')
            .map_or(start, |index| index + 1);
        let prefix: String = chars[..start].iter().collect();
        let suffix: String = chars[end..].iter().collect();
        let number: String = chars[start..end].iter().collect();
        let (integer, fraction) = match number.split_once('.') {
            Some((integer, fraction)) => (integer, fraction),
            None => (number.as_str(), ""),
        };
        let grouping = integer
            .rfind(',')
            .map_or(0, |index| integer.len() - index - 1);
        let min_integer = integer.chars().filter(|ch| *ch == '0').count();
        let min_fraction = fraction.chars().filter(|ch| *ch == '0').count();
        let max_fraction = fraction
            .chars()
            .filter(|ch| *ch == '0' || *ch == '#')
            .count();
        let percent = suffix.contains('%');
        Self {
            source: pattern.to_string(),
            prefix,
            suffix,
            grouping,
            min_integer,
            min_fraction,
            max_fraction,
            percent,
            compact: None,
        }
    }

    /// A compact number format: the value is written as a multiple of a thousand or more.
    fn compact(style: Compact) -> Self {
        let mut pattern = Self::parse("#,##0.#");
        pattern.compact = Some(style);
        pattern
    }

    /// Format a number with this pattern.
    fn format(&self, value: f64) -> String {
        let mut scaled = if self.percent { value * 100.0 } else { value };
        let mut compact_suffix = String::new();
        if let Some(style) = self.compact {
            let magnitude = scaled.abs();
            let (divisor, short, long) = if magnitude >= 1e12 {
                (1e12, "T", " trillion")
            } else if magnitude >= 1e9 {
                (1e9, "B", " billion")
            } else if magnitude >= 1e6 {
                (1e6, "M", " million")
            } else if magnitude >= 1e3 {
                (1e3, "K", " thousand")
            } else {
                (1.0, "", "")
            };
            scaled /= divisor;
            compact_suffix = match style {
                Compact::Short => short.to_string(),
                Compact::Long => long.to_string(),
            };
        }
        let factor = power_of_ten(self.max_fraction);
        let magnitude = crate::math::floor(scaled.abs() * factor + 0.5) / factor;
        let negative = scaled < 0.0 && magnitude != 0.0;
        let integer = crate::math::floor(magnitude);
        let fraction = crate::math::floor((magnitude - integer) * factor + 0.5) as u64;
        let digits = format_integer(integer);
        let grouped = group_digits(&digits, self.grouping);
        let padded = if grouped.len() < self.min_integer {
            let mut text = String::new();
            for _ in 0..(self.min_integer - grouped.len()) {
                text.push('0');
            }
            text.push_str(&grouped);
            text
        } else {
            grouped
        };
        let mut out = String::new();
        if negative {
            out.push('-');
        }
        out.push_str(&self.prefix);
        out.push_str(&padded);
        if self.max_fraction > 0 {
            let mut fraction_text = alloc::format!("{fraction:0width$}", width = self.max_fraction);
            while fraction_text.len() > self.min_fraction && fraction_text.ends_with('0') {
                fraction_text.pop();
            }
            if !fraction_text.is_empty() {
                out.push('.');
                out.push_str(&fraction_text);
            }
        }
        out.push_str(&self.suffix);
        out.push_str(&compact_suffix);
        out
    }
}

/// A power of ten, without the float maths the portable core does not carry.
fn power_of_ten(exponent: usize) -> f64 {
    let mut value = 1.0f64;
    for _ in 0..exponent {
        value *= 10.0;
    }
    value
}

/// The digits of a whole number, without a fraction.
fn format_integer(value: f64) -> String {
    if value < 9.2e18 {
        alloc::format!("{}", value as i64)
    } else {
        alloc::format!("{value}")
    }
}

/// Put a grouping separator between the groups of digits.
fn group_digits(digits: &str, grouping: usize) -> String {
    if grouping == 0 {
        return digits.to_string();
    }
    let chars: Vec<char> = digits.chars().collect();
    let mut out = String::new();
    for (index, ch) in chars.iter().enumerate() {
        if index > 0 && (chars.len() - index) % grouping == 0 {
            out.push(',');
        }
        out.push(*ch);
    }
    out
}

/// The number format of a locale: the currency symbol it uses.
fn currency_symbol(country: &str) -> &'static str {
    match country {
        "JP" => "¥",
        "GB" => "£",
        "CH" => "CHF",
        "DE" | "FR" | "IT" | "ES" | "NL" | "BE" | "AT" | "IE" | "PT" | "FI" | "GR" => "€",
        "CN" => "¥",
        "IN" => "₹",
        "KR" => "₩",
        _ => "$",
    }
}

/// The country of the default locale, when the runtime knows one.
fn default_country(vm: &mut Vm<'_>) -> String {
    let Some(locale) = vm.resolve_class_lenient_pub("java/util/Locale") else {
        return String::from("US");
    };
    vm.read_static_ref_if_set(locale, "defaultLocale", "Ljava/util/Locale;")
        .map(|instance| {
            vm.read_string_field(instance, vm.class_of(instance), "country")
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

/// The number format of a locale, used when the message names no style of its own.
fn default_number_pattern(vm: &mut Vm<'_>) -> NumberPattern {
    let _ = vm;
    NumberPattern::default_pattern()
}

/// The currency pattern of a locale.
fn currency_pattern(vm: &mut Vm<'_>) -> NumberPattern {
    let country = default_country(vm);
    let symbol = currency_symbol(&country);
    let mut pattern = NumberPattern::parse(&alloc::format!("{symbol}#,##0.00"));
    pattern.min_fraction = 2;
    pattern.max_fraction = 2;
    pattern
}

// -------------------------------------------------------------------------------------------
// Date patterns
// -------------------------------------------------------------------------------------------

/// The date patterns the JDK's styles use.
fn date_pattern(style: &str) -> &'static str {
    match style {
        "short" => "M/d/yy",
        "medium" => "MMM d, yyyy",
        "long" => "MMMM d, yyyy",
        "full" => "EEEE, MMMM d, yyyy",
        _ => "MMM d, yyyy",
    }
}

/// The time patterns the JDK's styles use.
fn time_pattern(style: &str) -> &'static str {
    match style {
        "short" => "h:mm a",
        "medium" => "h:mm:ss a",
        "long" => "h:mm:ss a z",
        "full" => "h:mm:ss a z",
        _ => "h:mm:ss a",
    }
}

// -------------------------------------------------------------------------------------------
// Format
// -------------------------------------------------------------------------------------------

const FORMAT_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method(
        "format",
        "(Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = format_argument(vm, receiver(args), ref_arg(args, 1))?;
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "format",
        "(Ljava/lang/Object;Ljava/lang/StringBuffer;Ljava/text/FieldPosition;)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = format_argument(vm, receiver(args), ref_arg(args, 1))?;
            let buffer = ref_arg(args, 2);
            append_to_buffer(vm, buffer, &text);
            object(buffer)
        },
    ),
    method(
        "format",
        "(D)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = oxjvm_vm::format::double_to_string(args[1].as_double());
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "format",
        "(J)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = oxjvm_vm::format::long_to_string(args[1].as_long());
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toPattern",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "applyPattern",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

/// Format one argument with a format, the way the JDK does.
fn format_argument(
    vm: &mut Vm<'_>,
    format: ObjectRef,
    argument: ObjectRef,
) -> Result<String, VmError> {
    if argument.is_null() {
        return Ok(String::from("null"));
    }
    let class = vm.class_name(vm.class_of(argument));
    match class {
        "java/util/Date" => Ok(crate::text::format_pattern(
            &pattern_of(vm, format),
            date_millis(vm, argument),
        )),
        _ => {
            if number_value(vm, argument).is_some() {
                Ok(number_to_text(vm, argument))
            } else {
                Ok(vm.string_or_empty(argument))
            }
        }
    }
}

/// The text of a number, the way the JDK's `Number.toString` writes it.
fn number_to_text(vm: &mut Vm<'_>, argument: ObjectRef) -> String {
    let Some(value) = number_value(vm, argument) else {
        return vm.string_or_empty(argument);
    };
    if is_double(vm, argument) {
        oxjvm_vm::format::double_to_string(value)
    } else if is_float(vm, argument) {
        oxjvm_vm::format::float_to_string(value as f32)
    } else if is_long(vm, argument) {
        oxjvm_vm::format::long_to_string(value as i64)
    } else {
        oxjvm_vm::format::int_to_string(value as i32)
    }
}

/// Whether a reference is a `java.lang.Double`.
fn is_double(vm: &mut Vm<'_>, reference: ObjectRef) -> bool {
    vm.resolve_class_lenient_pub("java/lang/Double")
        .is_some_and(|class| vm.is_instance(reference, class))
}

/// Whether a reference is a `java.lang.Float`.
fn is_float(vm: &mut Vm<'_>, reference: ObjectRef) -> bool {
    vm.resolve_class_lenient_pub("java/lang/Float")
        .is_some_and(|class| vm.is_instance(reference, class))
}

/// Whether a reference is a `java.lang.Long`.
fn is_long(vm: &mut Vm<'_>, reference: ObjectRef) -> bool {
    vm.resolve_class_lenient_pub("java/lang/Long")
        .is_some_and(|class| vm.is_instance(reference, class))
}

/// The milliseconds a `java.util.Date` holds.
fn date_millis(vm: &mut Vm<'_>, date: ObjectRef) -> i64 {
    crate::lang::read_long_field(vm, date, "time")
}

/// The pattern a format carries.
fn pattern_of(vm: &mut Vm<'_>, format: ObjectRef) -> String {
    if format.is_null() {
        return String::new();
    }
    vm.read_string_field(format, vm.class_of(format), "pattern")
        .unwrap_or_default()
}

/// Append text to a `StringBuffer`.
fn append_to_buffer(vm: &mut Vm<'_>, buffer: ObjectRef, text: &str) {
    let _ = vm;
    let _ = (buffer, text);
}

/// `java.text.Format`: the base of every format.
pub(crate) const FORMAT: oxjvm_vm::NativeClass = class(
    "java/text/Format",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &FORMAT_METHODS,
    None,
);

/// `java.text.FieldPosition`: the place a format wrote to.
pub(crate) const FIELD_POSITION: oxjvm_vm::NativeClass = class(
    "java/text/FieldPosition",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("beginIndex", "I", ACC_PRIVATE, None),
        field("endIndex", "I", ACC_PRIVATE, None),
    ],
    &FIELD_POSITION_METHODS,
    None,
);

const FIELD_POSITION_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        write_int_field(vm, this, "beginIndex", crate::int_arg(args, 1));
        write_int_field(vm, this, "endIndex", crate::int_arg(args, 1));
        void()
    }),
    method("getBeginIndex", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "beginIndex"))
    }),
    method("getEndIndex", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "endIndex"))
    }),
    method("setBeginIndex", "(I)V", ACC_PUBLIC, |vm, _, args| {
        write_int_field(vm, receiver(args), "beginIndex", crate::int_arg(args, 1));
        void()
    }),
    method("setEndIndex", "(I)V", ACC_PUBLIC, |vm, _, args| {
        write_int_field(vm, receiver(args), "endIndex", crate::int_arg(args, 1));
        void()
    }),
];

const PARSE_POSITION_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        write_int_field(vm, receiver(args), "index", crate::int_arg(args, 1));
        void()
    }),
    method("getIndex", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "index"))
    }),
    method("setIndex", "(I)V", ACC_PUBLIC, |vm, _, args| {
        write_int_field(vm, receiver(args), "index", crate::int_arg(args, 1));
        void()
    }),
    method("getErrorIndex", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "errorIndex"))
    }),
    method("setErrorIndex", "(I)V", ACC_PUBLIC, |vm, _, args| {
        write_int_field(vm, receiver(args), "errorIndex", crate::int_arg(args, 1));
        void()
    }),
];

/// `java.text.ParsePosition`: where a parse stands, and where it went wrong.
pub(crate) const PARSE_POSITION: oxjvm_vm::NativeClass = class(
    "java/text/ParsePosition",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[
        field("index", "I", ACC_PRIVATE, None),
        field("errorIndex", "I", ACC_PRIVATE, None),
    ],
    &PARSE_POSITION_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// NumberFormat
// -------------------------------------------------------------------------------------------

const NUMBER_FORMAT_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
    method(
        "getInstance",
        "()Ljava/text/NumberFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let pattern = default_number_pattern(vm);
            number_format(vm, &pattern)
        },
    ),
    method(
        "getInstance",
        "(Ljava/util/Locale;)Ljava/text/NumberFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let pattern = default_number_pattern(vm);
            number_format(vm, &pattern)
        },
    ),
    method(
        "getIntegerInstance",
        "()Ljava/text/NumberFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| number_format(vm, &NumberPattern::parse("#,##0")),
    ),
    method(
        "getIntegerInstance",
        "(Ljava/util/Locale;)Ljava/text/NumberFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| number_format(vm, &NumberPattern::parse("#,##0")),
    ),
    method(
        "getPercentInstance",
        "()Ljava/text/NumberFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| number_format(vm, &NumberPattern::parse("#,##0%")),
    ),
    method(
        "getPercentInstance",
        "(Ljava/util/Locale;)Ljava/text/NumberFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| number_format(vm, &NumberPattern::parse("#,##0%")),
    ),
    method(
        "getCurrencyInstance",
        "()Ljava/text/NumberFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let pattern = currency_pattern(vm);
            number_format(vm, &pattern)
        },
    ),
    method(
        "getCurrencyInstance",
        "(Ljava/util/Locale;)Ljava/text/NumberFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let pattern = currency_pattern(vm);
            number_format(vm, &pattern)
        },
    ),
    method(
        "getCompactNumberInstance",
        "(Ljava/util/Locale;Ljava/text/NumberFormat$Style;)Ljava/text/NumberFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let style = vm
                .read_string_field(ref_arg(args, 1), vm.class_of(ref_arg(args, 1)), "name")
                .unwrap_or_default();
            let compact = if style == "LONG" {
                Compact::Long
            } else {
                Compact::Short
            };
            number_format(vm, &NumberPattern::compact(compact))
        },
    ),
    method(
        "format",
        "(Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = number_argument_text(vm, receiver(args), ref_arg(args, 1));
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "format",
        "(D)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pattern = pattern_of(vm, receiver(args));
            let text = NumberPattern::parse(&pattern).format(args[1].as_double());
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "format",
        "(J)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pattern = pattern_of(vm, receiver(args));
            let text = NumberPattern::parse(&pattern).format(args[1].as_long() as f64);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toPattern",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pattern = pattern_of(vm, receiver(args));
            object(vm.make_string(&pattern)?)
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
            let same = vm.class_of(receiver(args)) == vm.class_of(other)
                && pattern_of(vm, receiver(args)) == pattern_of(vm, other);
            crate::boolean(same)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let pattern = pattern_of(vm, receiver(args));
        let mut hash = 0i32;
        for ch in pattern.chars() {
            hash = hash.wrapping_mul(31).wrapping_add(ch as i32);
        }
        int(hash)
    }),
];

/// The text of a number argument, in the form the JDK formats.
fn number_argument_text(vm: &mut Vm<'_>, format: ObjectRef, argument: ObjectRef) -> String {
    if argument.is_null() {
        return String::from("null");
    }
    match number_value(vm, argument) {
        Some(value) => {
            let pattern = NumberPattern::parse(&pattern_of(vm, format));
            pattern.format(value)
        }
        None => number_to_text(vm, argument),
    }
}

/// A number format that carries a pattern.
fn number_format(vm: &mut Vm<'_>, pattern: &NumberPattern) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/text/OjvmNumberFormat")?;
    let instance = vm.new_instance(class)?;
    let text = vm.make_string(&pattern.text())?;
    let (declaring, field) = vm.find_field(class, "pattern", "Ljava/lang/String;")?;
    vm.set_instance_ref(instance, declaring, field, text);
    let kind = vm.make_string("number")?;
    let (declaring, field) = vm.find_field(class, "kind", "Ljava/lang/String;")?;
    vm.set_instance_ref(instance, declaring, field, kind);
    object(instance)
}

/// `java.text.NumberFormat$Style`: the style of a compact number format.
pub(crate) const NUMBER_FORMAT_STYLE: oxjvm_vm::NativeClass = class(
    "java/text/NumberFormat$Style",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field("name", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("ordinal", "I", ACC_PRIVATE, None),
        field(
            "SHORT",
            "Ljava/text/NumberFormat$Style;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "LONG",
            "Ljava/text/NumberFormat$Style;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
    ],
    &NUMBER_FORMAT_STYLE_METHODS,
    Some(number_format_style_clinit),
);

const NUMBER_FORMAT_STYLE_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
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
];

/// The styles of a compact number format.
fn number_format_style_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/text/NumberFormat$Style")?;
    for (ordinal, name) in ["SHORT", "LONG"].iter().enumerate() {
        let instance = vm.new_instance(class)?;
        vm.set_string_field(instance, class, "name", name)?;
        write_int_field(vm, instance, "ordinal", ordinal as i32);
        vm.set_static_value(
            class,
            name,
            "Ljava/text/NumberFormat$Style;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

/// `java.text.DecimalFormat`: a number format that reads its pattern.
pub(crate) const DECIMAL_FORMAT: oxjvm_vm::NativeClass = class(
    "java/text/DecimalFormat",
    Some("java/text/NumberFormat"),
    &[],
    ACC_PUBLIC,
    &[field("pattern", "Ljava/lang/String;", ACC_PRIVATE, None)],
    &DECIMAL_FORMAT_METHODS,
    None,
);

const DECIMAL_FORMAT_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "pattern", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "toPattern",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pattern = pattern_of(vm, receiver(args));
            object(vm.make_string(&pattern)?)
        },
    ),
    method(
        "applyPattern",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "pattern", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "parse",
        "(Ljava/lang/String;)Ljava/lang/Number;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 1));
            match parse_number_text(&text) {
                Some(value) => Ok(crate::boxed::boxed_double(vm, value)?),
                None => Err(vm.throw_new("java/text/ParseException", Some("Unparsable number"))),
            }
        },
    ),
];

/// `java.text.NumberFormat`.
pub(crate) const NUMBER_FORMAT: oxjvm_vm::NativeClass = class(
    "java/text/NumberFormat",
    Some("java/text/Format"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &NUMBER_FORMAT_METHODS,
    None,
);

/// The number format the runtime hands to the guest.
pub(crate) const OJVM_NUMBER_FORMAT: oxjvm_vm::NativeClass = class(
    "java/text/OjvmNumberFormat",
    Some("java/text/NumberFormat"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("pattern", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("kind", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &[],
    None,
);

// -------------------------------------------------------------------------------------------
// DateFormat
// -------------------------------------------------------------------------------------------

const DATE_FORMAT_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    method(
        "getInstance",
        "()Ljava/text/DateFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| date_format(vm, "MMM d, yyyy h:mm:ss a"),
    ),
    method(
        "getInstance",
        "(Ljava/util/Locale;)Ljava/text/DateFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| date_format(vm, "MMM d, yyyy h:mm:ss a"),
    ),
    method(
        "getDateInstance",
        "()Ljava/text/DateFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| date_format(vm, date_pattern("medium")),
    ),
    method(
        "getDateInstance",
        "(Ljava/util/Locale;)Ljava/text/DateFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| date_format(vm, date_pattern("medium")),
    ),
    method(
        "getDateInstance",
        "(I)Ljava/text/DateFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| date_format(vm, date_pattern(date_style_name(crate::int_arg(args, 0)))),
    ),
    method(
        "getDateInstance",
        "(ILjava/util/Locale;)Ljava/text/DateFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| date_format(vm, date_pattern(date_style_name(crate::int_arg(args, 0)))),
    ),
    method(
        "getTimeInstance",
        "()Ljava/text/DateFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| date_format(vm, time_pattern("medium")),
    ),
    method(
        "getTimeInstance",
        "(Ljava/util/Locale;)Ljava/text/DateFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| date_format(vm, time_pattern("medium")),
    ),
    method(
        "getDateTimeInstance",
        "()Ljava/text/DateFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| date_format(vm, "MMM d, yyyy h:mm:ss a"),
    ),
    method(
        "getDateTimeInstance",
        "(Ljava/util/Locale;)Ljava/text/DateFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| date_format(vm, "MMM d, yyyy h:mm:ss a"),
    ),
    method(
        "format",
        "(Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = format_argument(vm, receiver(args), ref_arg(args, 1))?;
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "format",
        "(Ljava/util/Date;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let millis = date_millis(vm, ref_arg(args, 1));
            let text = crate::text::format_pattern(&pattern_of(vm, receiver(args)), millis);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toPattern",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pattern = pattern_of(vm, receiver(args));
            object(vm.make_string(&pattern)?)
        },
    ),
    method(
        "applyPattern",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "pattern", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
];

fn date_style_name(style: i32) -> &'static str {
    match style {
        0 => "full",
        1 => "long",
        2 => "medium",
        3 => "short",
        _ => "medium",
    }
}

/// A date format that carries a pattern.
fn date_format(vm: &mut Vm<'_>, pattern: &str) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/text/OjvmDateFormat")?;
    let instance = vm.new_instance(class)?;
    let text = vm.make_string(pattern)?;
    let (declaring, field) = vm.find_field(class, "pattern", "Ljava/lang/String;")?;
    vm.set_instance_ref(instance, declaring, field, text);
    let kind = vm.make_string("date")?;
    let (declaring, field) = vm.find_field(class, "kind", "Ljava/lang/String;")?;
    vm.set_instance_ref(instance, declaring, field, kind);
    object(instance)
}

/// `java.text.DateFormat`.
pub(crate) const DATE_FORMAT: oxjvm_vm::NativeClass = class(
    "java/text/DateFormat",
    Some("java/text/Format"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &DATE_FORMAT_METHODS,
    None,
);

/// The date format the runtime hands to the guest.
pub(crate) const OJVM_DATE_FORMAT: oxjvm_vm::NativeClass = class(
    "java/text/OjvmDateFormat",
    Some("java/text/DateFormat"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("pattern", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("kind", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &[],
    None,
);

// -------------------------------------------------------------------------------------------
// ChoiceFormat
// -------------------------------------------------------------------------------------------

/// The limits and the texts of a choice format.
#[derive(Debug, Clone)]
struct Choice {
    limits: Vec<f64>,
    texts: Vec<String>,
}

impl Choice {
    /// Read a pattern such as `0#none|1#one`.
    ///
    /// A limit is written before a `#`, a `<` or a `≤`; a `<` opens the range above the limit,
    /// so the limit becomes the next number after it.  The parts alternate between a limit and a
    /// text, and a `|` ends a pair.
    fn parse(pattern: &str) -> Self {
        let mut limits: Vec<f64> = Vec::new();
        let mut texts: Vec<String> = Vec::new();
        let mut limit_text = String::new();
        let mut format_text = String::new();
        let mut in_format = false;
        let mut in_quote = false;
        let chars: Vec<char> = pattern.chars().collect();
        let mut index = 0;
        while index < chars.len() {
            let ch = chars[index];
            if ch == '\'' {
                if index + 1 < chars.len() && chars[index + 1] == '\'' {
                    if in_format {
                        format_text.push('\'');
                    } else {
                        limit_text.push('\'');
                    }
                    index += 1;
                } else {
                    in_quote = !in_quote;
                }
            } else if (ch == '<' || ch == '#' || ch == '\u{2264}') && !in_quote && !in_format {
                let limit = limit_text.trim().parse::<f64>().unwrap_or(0.0);
                let limit = if ch == '<' { next_double(limit) } else { limit };
                limits.push(limit);
                limit_text.clear();
                in_format = true;
            } else if ch == '|' && !in_quote {
                if in_format {
                    texts.push(format_text.clone());
                    format_text.clear();
                    in_format = false;
                }
            } else if in_format {
                format_text.push(ch);
            } else {
                limit_text.push(ch);
            }
            index += 1;
        }
        if in_format {
            texts.push(format_text);
        }
        Self { limits, texts }
    }

    /// The text a number gets.
    ///
    /// A limit opens a range that reaches to the next limit; a number below the first limit
    /// belongs to the first range.
    fn text(&self, value: f64) -> String {
        let mut index = 0;
        for (position, limit) in self.limits.iter().enumerate() {
            if value >= *limit {
                index = position;
            } else {
                break;
            }
        }
        self.texts.get(index).cloned().unwrap_or_default()
    }

    /// The pattern of a choice, written the way the JDK writes it.
    ///
    /// A limit that is not a whole number is written as the number below it with a `<`, so that
    /// the range it opens stays the same.
    fn pattern(&self) -> String {
        let mut pattern = String::new();
        for (index, limit) in self.limits.iter().enumerate() {
            if index > 0 {
                pattern.push('|');
            }
            let less = previous_double(*limit);
            let exact = ieee_remainder_one(*limit).abs();
            let below = ieee_remainder_one(less).abs();
            if exact < below {
                pattern.push_str(&oxjvm_vm::format::double_to_string(*limit));
                pattern.push('#');
            } else {
                pattern.push_str(&oxjvm_vm::format::double_to_string(less));
                pattern.push('<');
            }
            pattern.push_str(
                self.texts
                    .get(index)
                    .map(|text| text.as_str())
                    .unwrap_or(""),
            );
        }
        pattern
    }
}

/// The number after a given one.
fn next_double(value: f64) -> f64 {
    if value.is_nan() || value == f64::INFINITY {
        return value;
    }
    if value == 0.0 {
        return f64::from_bits(1);
    }
    f64::from_bits(value.to_bits() + 1)
}

/// The number before a given one.
fn previous_double(value: f64) -> f64 {
    if value.is_nan() || value == f64::NEG_INFINITY {
        return value;
    }
    if value == 0.0 {
        return -f64::from_bits(1);
    }
    f64::from_bits(value.to_bits() - 1)
}

/// The part of a number that is left after the whole ones are taken off.
fn ieee_remainder_one(value: f64) -> f64 {
    value - crate::math::floor(value + 0.5)
}

const CHOICE_FORMAT_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let pattern = vm.string_or_empty(ref_arg(args, 1));
            let choice = Choice::parse(&pattern);
            let text = vm.make_string(&choice.pattern())?;
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "pattern", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, text);
            void()
        },
    ),
    method(
        "<init>",
        "([D[Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let limits = ref_arg(args, 1);
            let texts = ref_arg(args, 2);
            let mut pattern = String::new();
            let count = vm.array_length(limits).unwrap_or(0);
            for index in 0..count {
                let limit = vm.array_get(limits, index as i32)?.as_double();
                let text = vm.array_get(texts, index as i32)?.as_ref();
                if index > 0 {
                    pattern.push('|');
                }
                pattern.push_str(&oxjvm_vm::format::double_to_string(limit));
                pattern.push('#');
                pattern.push_str(&vm.string_or_empty(text));
            }
            let text = vm.make_string(&pattern)?;
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "pattern", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, text);
            void()
        },
    ),
    method("<init>", "([D[D)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let limits = ref_arg(args, 1);
        let texts = ref_arg(args, 2);
        let mut pattern = String::new();
        let count = vm.array_length(limits).unwrap_or(0);
        for index in 0..count {
            let limit = vm.array_get(limits, index as i32)?.as_double();
            let text = vm.array_get(texts, index as i32)?.as_ref();
            if index > 0 {
                pattern.push('|');
            }
            pattern.push_str(&oxjvm_vm::format::double_to_string(limit));
            pattern.push('#');
            pattern.push_str(&vm.string_or_empty(text));
        }
        let text = vm.make_string(&pattern)?;
        let (declaring, field) =
            vm.find_field(vm.class_of(this), "pattern", "Ljava/lang/String;")?;
        vm.set_instance_ref(this, declaring, field, text);
        void()
    }),
    method(
        "format",
        "(D)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let choice = Choice::parse(&pattern_of(vm, receiver(args)));
            object(vm.make_string(&choice.text(args[1].as_double()))?)
        },
    ),
    method(
        "format",
        "(Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let choice = Choice::parse(&pattern_of(vm, receiver(args)));
            let value = number_value(vm, ref_arg(args, 1)).unwrap_or(0.0);
            object(vm.make_string(&choice.text(value))?)
        },
    ),
    method(
        "toPattern",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pattern = pattern_of(vm, receiver(args));
            object(vm.make_string(&pattern)?)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pattern = pattern_of(vm, receiver(args));
            let text = alloc::format!("ChoiceFormat [pattern: \"{pattern}\"]");
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "setPattern",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "pattern", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
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
            let this = receiver(args);
            let same = vm.class_of(this) == vm.class_of(other)
                && pattern_of(vm, this) == pattern_of(vm, other);
            crate::boolean(same)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let pattern = pattern_of(vm, receiver(args));
        let mut hash = 0i32;
        for ch in pattern.chars() {
            hash = hash.wrapping_mul(31).wrapping_add(ch as i32);
        }
        int(hash)
    }),
];

/// The value of a number argument.
fn number_value(vm: &mut Vm<'_>, argument: ObjectRef) -> Option<f64> {
    if argument.is_null() {
        return None;
    }
    if !is_number(vm, argument) {
        return None;
    }
    if is_double(vm, argument) {
        return Some(f64::from_bits(
            crate::boxed::read_value_long(vm, argument) as u64
        ));
    }
    if is_float(vm, argument) {
        return Some(f32::from_bits(crate::boxed::read_value_int(vm, argument) as u32) as f64);
    }
    if is_long(vm, argument) {
        return Some(crate::boxed::read_value_long(vm, argument) as f64);
    }
    Some(crate::boxed::read_value_int(vm, argument) as f64)
}

/// Whether a reference is a `java.lang.Number`.
fn is_number(vm: &mut Vm<'_>, reference: ObjectRef) -> bool {
    vm.resolve_class_lenient_pub("java/lang/Number")
        .is_some_and(|class| vm.is_instance(reference, class))
}

/// `java.text.ChoiceFormat`.
pub(crate) const CHOICE_FORMAT: oxjvm_vm::NativeClass = class(
    "java/text/ChoiceFormat",
    Some("java/text/Format"),
    &[],
    ACC_PUBLIC,
    &[
        field("pattern", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("kind", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &CHOICE_FORMAT_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// MessageFormat
// -------------------------------------------------------------------------------------------

/// One piece of a message: either literal text or an argument.
#[derive(Debug, Clone)]
enum Element {
    Literal(String),
    Argument {
        index: i64,
        kind: String,
        style: String,
    },
}

/// The pieces of a message pattern.
#[derive(Debug, Clone, Default)]
struct Message {
    elements: Vec<Element>,
}

impl Message {
    /// The literal text before each element, and the text that ends the message.
    fn literals(&self) -> Vec<String> {
        let mut literals = Vec::new();
        let mut current = String::new();
        for element in &self.elements {
            match element {
                Element::Literal(text) => current.push_str(text),
                Element::Argument { .. } => {
                    literals.push(core::mem::take(&mut current));
                }
            }
        }
        literals.push(current);
        literals
    }

    /// The argument number of each element.
    fn argument_numbers(&self) -> Vec<i32> {
        self.elements
            .iter()
            .filter_map(|element| match element {
                Element::Argument { index, .. } => Some(*index as i32),
                Element::Literal(_) => None,
            })
            .collect()
    }
}

/// Read a message pattern, the way the JDK reads one.
fn parse_message(pattern: &str) -> Result<Message, String> {
    const RAW: usize = 0;
    const INDEX: usize = 1;
    const TYPE: usize = 2;
    const MODIFIER: usize = 3;
    let mut segments: Vec<String> =
        vec![String::new(), String::new(), String::new(), String::new()];
    let mut used = [false, false, false, false];
    let mut part = RAW;
    let mut in_quote = false;
    let mut brace_stack = 0usize;
    let chars: Vec<char> = pattern.chars().collect();
    let mut index = 0usize;
    while index < chars.len() {
        let ch = chars[index];
        if part == RAW {
            if ch == '\'' {
                if index + 1 < chars.len() && chars[index + 1] == '\'' {
                    segments[part].push(ch);
                    index += 1;
                } else {
                    in_quote = !in_quote;
                }
            } else if ch == '{' && !in_quote {
                part = INDEX;
                used[INDEX] = true;
            } else {
                segments[part].push(ch);
            }
        } else if in_quote {
            segments[part].push(ch);
            if ch == '\'' {
                in_quote = false;
            }
        } else {
            match ch {
                ',' if part < MODIFIER => {
                    part += 1;
                    used[part] = true;
                }
                '{' => {
                    brace_stack += 1;
                    segments[part].push(ch);
                }
                '}' if brace_stack == 0 => {
                    part = RAW;
                    used[INDEX] = false;
                    used[TYPE] = false;
                    used[MODIFIER] = false;
                }
                '}' => {
                    brace_stack -= 1;
                    segments[part].push(ch);
                }
                ' ' if part == TYPE && segments[TYPE].is_empty() => {}
                _ => segments[part].push(ch),
            }
        }
        index += 1;
    }
    if brace_stack == 0 && part != RAW {
        return Err(String::from("Unmatched braces in the pattern."));
    }
    let _ = used;

    // The segments were filled in order; walk the pattern again to collect the elements.
    let mut elements = Vec::new();
    let mut literal = String::new();
    let mut pending: Option<(String, String, String)> = None;
    let mut in_quote = false;
    let mut brace_stack = 0usize;
    let mut part = 0usize;
    let mut index = 0usize;
    while index < chars.len() {
        let ch = chars[index];
        if in_quote {
            if ch == '\'' {
                if index + 1 < chars.len() && chars[index + 1] == '\'' {
                    literal.push('\'');
                    index += 1;
                } else {
                    in_quote = false;
                }
            } else {
                literal.push(ch);
            }
            index += 1;
            continue;
        }
        match ch {
            '\'' => {
                // Two quotes are a literal quote; one quote starts a quoted section.
                if index + 1 < chars.len() && chars[index + 1] == '\'' {
                    literal.push('\'');
                    index += 1;
                } else {
                    in_quote = true;
                }
            }
            '{' => {
                brace_stack += 1;
                if brace_stack == 1 {
                    if !literal.is_empty() {
                        elements.push(Element::Literal(core::mem::take(&mut literal)));
                    }
                    pending = Some((String::new(), String::new(), String::new()));
                    part = 0;
                } else if let Some((index_text, kind, style)) = pending.as_mut() {
                    // A brace inside an argument belongs to the part that is being read.
                    match part {
                        0 => index_text.push(ch),
                        1 => kind.push(ch),
                        _ => style.push(ch),
                    }
                }
            }
            '}' => {
                if brace_stack == 0 {
                    // A closing brace outside an argument is a literal character.
                    literal.push(ch);
                } else if brace_stack == 1 {
                    if let Some((index_text, kind, style)) = pending.take() {
                        let number = index_text.trim().parse::<i64>().map_err(|_| {
                            alloc::format!("can't parse argument number: {index_text}")
                        })?;
                        if number < 0 {
                            return Err(alloc::format!("negative argument number: {number}"));
                        }
                        if number >= MAX_ARGUMENT_INDEX {
                            return Err(alloc::format!(
                                "{number} exceeds the ArgumentIndex implementation limit"
                            ));
                        }
                        let kind = kind.trim().to_string();
                        if !kind.is_empty()
                            && !matches!(
                                kind.to_ascii_lowercase().as_str(),
                                "number" | "date" | "time" | "choice"
                            )
                        {
                            return Err(alloc::format!("unknown format type: {kind}"));
                        }
                        elements.push(Element::Argument {
                            index: number,
                            kind,
                            style: style.trim().to_string(),
                        });
                    }
                } else if let Some((index_text, kind, style)) = pending.as_mut() {
                    match part {
                        0 => index_text.push(ch),
                        1 => kind.push(ch),
                        _ => style.push(ch),
                    }
                }
                brace_stack = brace_stack.saturating_sub(1);
            }
            ',' if brace_stack == 1 && part < 2 => {
                // The first comma ends the argument index, the second the type; the style
                // keeps its own commas.
                part += 1;
            }
            _ => {
                if brace_stack == 0 {
                    literal.push(ch);
                } else if let Some((index_text, kind, style)) = pending.as_mut() {
                    match part {
                        0 => index_text.push(ch),
                        1 => kind.push(ch),
                        _ => style.push(ch),
                    }
                }
            }
        }
        index += 1;
    }
    if !literal.is_empty() {
        elements.push(Element::Literal(literal));
    }
    Ok(Message { elements })
}

/// The number a choice format picks for a value.
fn choice_text(pattern: &str, value: f64) -> String {
    Choice::parse(pattern).text(value)
}

/// Parse a message back into the arguments it was built from.
///
/// The literals of the pattern have to match the text; the text between them is what the
/// argument was written as.  A text that does not match gives the place where it went wrong.
fn parse_message_text(
    vm: &mut Vm<'_>,
    pattern: &str,
    input: &str,
) -> Result<(Vec<ObjectRef>, Option<i32>), VmError> {
    let message = parse_message(pattern)
        .map_err(|error| vm.throw_new("java/lang/IllegalArgumentException", Some(&error)))?;
    let mut values: Vec<Option<ObjectRef>> = Vec::new();
    let mut rest = input;
    for (position, element) in message.elements.iter().enumerate() {
        match element {
            Element::Literal(text) => {
                if !rest.starts_with(text) {
                    let offset = input.len() - rest.len();
                    return Ok((Vec::new(), Some(offset as i32)));
                }
                rest = &rest[text.len()..];
            }
            Element::Argument { index, kind, style } => {
                // The text of the argument runs until the literal that follows it.
                let (text, tail) = split_argument(rest, &message, position);
                let value = parse_argument(vm, kind, style, text)?;
                let index = *index as usize;
                if values.len() <= index {
                    values.resize(index + 1, None);
                }
                values[index] = Some(value);
                rest = tail;
            }
        }
    }
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        result.push(value.unwrap_or(ObjectRef::NULL));
    }
    Ok((result, None))
}

/// The text of an argument: everything up to the literal that follows it.
fn split_argument<'a>(rest: &'a str, message: &Message, position: usize) -> (&'a str, &'a str) {
    match message.elements.get(position + 1) {
        Some(Element::Literal(text)) => match rest.find(text.as_str()) {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, ""),
        },
        _ => (rest, ""),
    }
}

/// Parse the text of one argument, with the type and the style the pattern names.
fn parse_argument(
    vm: &mut Vm<'_>,
    kind: &str,
    style: &str,
    text: &str,
) -> Result<ObjectRef, VmError> {
    let trimmed = text.trim();
    match kind.to_ascii_lowercase().as_str() {
        "number" => {
            let value = parse_number_text(trimmed);
            match value {
                Some(value) => Ok(crate::boxed::boxed_double(vm, value)?.as_ref()),
                None => Ok(object(vm.make_string(trimmed)?)?.as_ref()),
            }
        }
        "date" | "time" => {
            let millis = parse_date_text(trimmed);
            let class = vm.resolve_class("java/util/Date")?;
            let instance = vm.new_instance(class)?;
            crate::lang::write_long_field(vm, instance, "time", millis);
            Ok(object(instance)?.as_ref())
        }
        "choice" => {
            let value = parse_number_text(trimmed).unwrap_or(0.0);
            Ok(object(vm.make_string(&choice_text(&unquote(style), value))?)?.as_ref())
        }
        _ => {
            let _ = style;
            // An argument without a type is read back as the text it was written as.
            Ok(object(vm.make_string(trimmed)?)?.as_ref())
        }
    }
}

/// The number a text holds, with the grouping separators and the currency symbols removed.
fn parse_number_text(text: &str) -> Option<f64> {
    let mut digits = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' {
            digits.push(ch);
        }
    }
    if digits.is_empty() || digits == "-" {
        return None;
    }
    digits.parse::<f64>().ok()
}

/// The milliseconds a date text holds, read with the patterns the runtime writes.
fn parse_date_text(text: &str) -> i64 {
    let cleaned: String = text
        .chars()
        .filter(|ch| ch.is_ascii_digit() || *ch == '/' || *ch == ':' || *ch == ',' || *ch == '.')
        .collect();
    let mut numbers = Vec::new();
    let mut current = String::new();
    for ch in cleaned.chars() {
        if ch.is_ascii_digit() {
            current.push(ch);
        } else {
            if !current.is_empty() {
                numbers.push(current.clone());
                current.clear();
            }
        }
    }
    if !current.is_empty() {
        numbers.push(current);
    }
    let number = |index: usize| -> i64 {
        numbers
            .get(index)
            .and_then(|text| text.parse::<i64>().ok())
            .unwrap_or(0)
    };
    let (year, month, day, hour, minute, second) = if numbers.len() >= 6 {
        (
            number(0),
            number(1),
            number(2),
            number(3),
            number(4),
            number(5),
        )
    } else if numbers.len() >= 3 {
        (number(0), number(1), number(2), 0, 0, 0)
    } else {
        (1970, 1, 1, 0, 0, 0)
    };
    let year = if year < 100 { year + 2000 } else { year };
    let (year, month, day) = if month > 12 {
        // The American order: the month comes first.
        (year, day, month)
    } else {
        (year, month, day)
    };
    millis_from_civil(year, month, day, hour, minute, second)
}

/// The milliseconds a UTC date and time holds.
fn millis_from_civil(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64) -> i64 {
    // The days from the civil date, the same way the epoch is counted.
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    ((days * 24 + hour) * 60 + minute) * 60_000 + second * 1000
}

const MESSAGE_FORMAT_METHODS: [oxjvm_vm::NativeMethodDef; 22] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let pattern = vm.string_or_empty(ref_arg(args, 1));
            let locale = default_locale(vm);
            set_message_locale(vm, receiver(args), locale);
            apply_message_pattern(vm, receiver(args), &pattern)
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/util/Locale;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let pattern = vm.string_or_empty(ref_arg(args, 1));
            set_message_locale(vm, receiver(args), ref_arg(args, 2));
            apply_message_pattern(vm, receiver(args), &pattern)
        },
    ),
    method(
        "format",
        "(Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = message_text(vm, receiver(args), ref_arg(args, 1))?;
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "format",
        "([Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = message_text(vm, receiver(args), ref_arg(args, 1))?;
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "format",
        "(Ljava/lang/Object;Ljava/lang/StringBuffer;Ljava/text/FieldPosition;)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = message_text(vm, receiver(args), ref_arg(args, 1))?;
            let buffer = ref_arg(args, 2);
            let _ = buffer;
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "format",
        "([Ljava/lang/Object;Ljava/lang/StringBuffer;Ljava/text/FieldPosition;)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = message_text(vm, receiver(args), ref_arg(args, 1))?;
            let buffer = ref_arg(args, 2);
            let _ = buffer;
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "format",
        "(Ljava/lang/String;[Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            // A static method has no receiver: the pattern is the first argument.
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let pattern = vm.string_or_empty(ref_arg(args, 0));
            let arguments = object_array_elements(vm, ref_arg(args, 1));
            let text = format_message(vm, &pattern, &arguments)?;
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "parse",
        "(Ljava/lang/String;)[Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pattern = message_pattern(vm, receiver(args));
            let text = vm.string_or_empty(ref_arg(args, 1));
            let (values, error) = parse_message_text(vm, &pattern, &text)?;
            if error.is_some() {
                return Err(vm.throw_new(
                    "java/text/ParseException",
                    Some(&alloc::format!("Cannot parse message: \"{text}\"")),
                ));
            }
            let array = vm.allocate_array_of(ArrayComponent::Reference, values.len())?;
            for (index, value) in values.iter().enumerate() {
                vm.array_set_ref(array, index, *value)?;
            }
            object(array)
        },
    ),
    method(
        "parse",
        "(Ljava/lang/String;Ljava/text/ParsePosition;)[Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pattern = message_pattern(vm, receiver(args));
            let position = ref_arg(args, 2);
            // A null text parses into nothing at all.
            if ref_arg(args, 1).is_null() {
                let array = vm.allocate_array_of(ArrayComponent::Reference, 0)?;
                return object(array);
            }
            let text = vm.string_or_empty(ref_arg(args, 1));
            let (values, error) = parse_message_text(vm, &pattern, &text)?;
            if let Some(offset) = error {
                write_int_field(vm, position, "errorIndex", offset);
                return object(ObjectRef::NULL);
            }
            let array = vm.allocate_array_of(ArrayComponent::Reference, values.len())?;
            for (index, value) in values.iter().enumerate() {
                vm.array_set_ref(array, index, *value)?;
            }
            write_int_field(vm, position, "index", text.chars().count() as i32);
            object(array)
        },
    ),
    method(
        "toPattern",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pattern = message_to_pattern(vm, receiver(args));
            object(vm.make_string(&pattern)?)
        },
    ),
    method(
        "applyPattern",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let pattern = vm.string_or_empty(ref_arg(args, 1));
            apply_message_pattern(vm, receiver(args), &pattern)
        },
    ),
    method(
        "getFormatByArgumentIndex",
        "(I)Ljava/text/Format;",
        ACC_PUBLIC,
        |vm, _, args| {
            let index = crate::int_arg(args, 1);
            object(message_format_at(vm, receiver(args), index))
        },
    ),
    method(
        "getFormatsByArgumentIndex",
        "()[Ljava/text/Format;",
        ACC_PUBLIC,
        |vm, _, args| {
            let numbers = message_argument_numbers(vm, receiver(args));
            let formats = message_formats(vm, receiver(args));
            let count = numbers.iter().copied().max().unwrap_or(-1) + 1;
            let array = vm.allocate_array_of(ArrayComponent::Reference, count.max(0) as usize)?;
            for (position, number) in numbers.iter().enumerate() {
                let value = formats.get(position).copied().unwrap_or(ObjectRef::NULL);
                vm.array_set_ref(array, *number as usize, value)?;
            }
            object(array)
        },
    ),
    method(
        "getFormats",
        "()[Ljava/text/Format;",
        ACC_PUBLIC,
        |vm, _, args| {
            let formats = message_formats(vm, receiver(args));
            let array = vm.allocate_array_of(ArrayComponent::Reference, formats.len())?;
            for (index, format) in formats.iter().enumerate() {
                vm.array_set_ref(array, index, *format)?;
            }
            object(array)
        },
    ),
    method(
        "setFormatsByArgumentIndex",
        "([Ljava/text/Format;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let given = ref_arg(args, 1);
            let numbers = message_argument_numbers(vm, this);
            let formats = message_formats(vm, this);
            // The formats are given by the argument index, and placed in the elements.  A
            // format that is not given leaves the one the pattern named.
            let array = vm.allocate_array_of(ArrayComponent::Reference, formats.len())?;
            for (position, number) in numbers.iter().enumerate() {
                let given_format = if given.is_null() {
                    None
                } else {
                    let length = vm.array_length(given)?;
                    if (*number as usize) < length {
                        Some(vm.array_get(given, *number)?.as_ref())
                    } else {
                        None
                    }
                };
                let value = match given_format {
                    Some(value) => value,
                    None => formats.get(position).copied().unwrap_or(ObjectRef::NULL),
                };
                vm.array_set_ref(array, position, value)?;
            }
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "formats", "[Ljava/text/Format;")?;
            vm.set_instance_ref(this, declaring, field, array);
            void()
        },
    ),
    method(
        "setFormats",
        "([Ljava/text/Format;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let formats = ref_arg(args, 1);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "formats", "[Ljava/text/Format;")?;
            vm.set_instance_ref(this, declaring, field, formats);
            void()
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let locale = vm
                .read_ref_field(this, vm.class_of(this), "locale", "Ljava/util/Locale;")
                .unwrap_or(ObjectRef::NULL);
            let locale = if locale.is_null() {
                String::from("null")
            } else {
                let (class, method) =
                    vm.resolve_virtual_method(locale, "getDisplayName", "()Ljava/lang/String;")?;
                let name = vm
                    .invoke_method(class, method, vec![Value::Ref(locale)])?
                    .as_ref();
                alloc::format!("\"{}\"", vm.string_or_empty(name))
            };
            let pattern = message_to_pattern(vm, this);
            let text = alloc::format!("MessageFormat [locale: {locale}, pattern: \"{pattern}\"]\n");
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "getLocale",
        "()Ljava/util/Locale;",
        ACC_PUBLIC,
        |vm, _, args| {
            let locale = vm
                .read_ref_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "locale",
                    "Ljava/util/Locale;",
                )
                .unwrap_or(ObjectRef::NULL);
            object(locale)
        },
    ),
    method(
        "setLocale",
        "(Ljava/util/Locale;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "locale", "Ljava/util/Locale;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "setFormatByArgumentIndex",
        "(ILjava/text/Format;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let index = crate::int_arg(args, 1);
            set_message_format(vm, receiver(args), index, ref_arg(args, 2));
            void()
        },
    ),
    method(
        "setFormatByArgument",
        "(ILjava/text/Format;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let index = crate::int_arg(args, 1);
            set_message_format(vm, receiver(args), index, ref_arg(args, 2));
            void()
        },
    ),
    method(
        "setFormat",
        "(ILjava/text/Format;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            // The old name of `setFormatByArgumentIndex`.
            let index = crate::int_arg(args, 1);
            set_message_format(vm, receiver(args), index, ref_arg(args, 2));
            void()
        },
    ),
];

/// Format a message with the pattern of a format.
fn message_text(
    vm: &mut Vm<'_>,
    format: ObjectRef,
    argument: ObjectRef,
) -> Result<String, VmError> {
    if argument.is_null() {
        return Ok(String::from("null"));
    }
    // An array of arguments is formatted element by element; a single argument is wrapped.
    let arguments = if is_object_array(vm, argument) {
        object_array_elements(vm, argument)
    } else {
        vec![argument]
    };
    let literals = message_literals(vm, format);
    let numbers = message_argument_numbers(vm, format);
    let formats = message_formats(vm, format);
    let mut out = String::new();
    for (position, number) in numbers.iter().enumerate() {
        if let Some(literal) = literals.get(position) {
            out.push_str(literal);
        }
        let Some(argument) = arguments.get(*number as usize).copied() else {
            out.push_str(&alloc::format!("{{{number}}}"));
            continue;
        };
        if argument.is_null() {
            out.push_str("null");
            continue;
        }
        let sub = formats.get(position).copied().unwrap_or(ObjectRef::NULL);
        out.push_str(&format_with_sub_format(vm, sub, argument)?);
    }
    if let Some(literal) = literals.last() {
        out.push_str(literal);
    }
    Ok(out)
}

/// Format an argument with the format the pattern gave it.
fn format_with_sub_format(
    vm: &mut Vm<'_>,
    format: ObjectRef,
    argument: ObjectRef,
) -> Result<String, VmError> {
    if format.is_null() {
        // Without a format a number is written as a number and a date as a date.
        if let Some(value) = number_value(vm, argument) {
            let pattern = default_number_pattern(vm);
            return Ok(pattern.format(value));
        }
        if is_date(vm, argument) {
            let pattern = "M/d/yy, h:mm a";
            return Ok(crate::text::format_pattern(
                pattern,
                argument_millis(vm, argument),
            ));
        }
        return Ok(vm.string_or_empty(argument));
    }
    let kind = vm
        .read_string_field(format, vm.class_of(format), "kind")
        .unwrap_or_default();
    let pattern = vm
        .read_string_field(format, vm.class_of(format), "pattern")
        .unwrap_or_default();
    match kind.as_str() {
        "number" => {
            let value = number_value(vm, argument).unwrap_or(0.0);
            Ok(NumberPattern::parse(&pattern).format(value))
        }
        "date" | "time" => {
            let millis = argument_millis(vm, argument);
            Ok(crate::text::format_pattern(&pattern, millis))
        }
        "choice" => {
            let value = number_value(vm, argument).unwrap_or(0.0);
            Ok(choice_text(&pattern, value))
        }
        _ => Ok(vm.string_or_empty(argument)),
    }
}

/// Whether a reference points at an array of objects.
fn is_object_array(vm: &mut Vm<'_>, reference: ObjectRef) -> bool {
    if reference.is_null() {
        return false;
    }
    vm.heap
        .get(reference)
        .map(|object| matches!(object.data, oxjvm_vm::ObjectData::Array(_)))
        .unwrap_or(false)
}

/// The elements of an array of objects.
fn object_array_elements(vm: &mut Vm<'_>, array: ObjectRef) -> Vec<ObjectRef> {
    let mut elements = Vec::new();
    if let Ok(length) = vm.array_length(array) {
        for index in 0..length {
            if let Ok(value) = vm.array_get(array, index as i32) {
                elements.push(value.as_ref());
            }
        }
    }
    elements
}

/// Format a message with a pattern and a list of arguments.
fn format_message(
    vm: &mut Vm<'_>,
    pattern: &str,
    arguments: &[ObjectRef],
) -> Result<String, VmError> {
    let message = parse_message(pattern)
        .map_err(|error| vm.throw_new("java/lang/IllegalArgumentException", Some(&error)))?;
    let mut out = String::new();
    for element in &message.elements {
        match element {
            Element::Literal(text) => out.push_str(text),
            Element::Argument { index, kind, style } => {
                let Some(argument) = arguments.get(*index as usize).copied() else {
                    out.push_str(&alloc::format!("{{{index}}}"));
                    continue;
                };
                out.push_str(&format_argument_element(vm, kind, style, argument)?);
            }
        }
    }
    Ok(out)
}
/// Format one argument of a message, with the type and the style the pattern names.
fn format_argument_element(
    vm: &mut Vm<'_>,
    kind: &str,
    style: &str,
    argument: ObjectRef,
) -> Result<String, VmError> {
    if argument.is_null() {
        return Ok(String::from("null"));
    }
    match kind.to_ascii_lowercase().as_str() {
        "number" => {
            let pattern = number_pattern_of(vm, style);
            let value = number_value(vm, argument).unwrap_or(0.0);
            Ok(pattern.format(value))
        }
        "date" | "time" => {
            let pattern = date_pattern_of(kind, style);
            let millis = argument_millis(vm, argument);
            Ok(crate::text::format_pattern(&pattern, millis))
        }
        "choice" => {
            let value = number_value(vm, argument).unwrap_or(0.0);
            Ok(choice_text(&unquote(style), value))
        }
        _ => {
            // Without a type a number is written as a number and a date as a date.
            if let Some(value) = number_value(vm, argument) {
                let pattern = default_number_pattern(vm);
                return Ok(pattern.format(value));
            }
            if is_date(vm, argument) {
                let pattern = "M/d/yy, h:mm a";
                return Ok(crate::text::format_pattern(
                    pattern,
                    argument_millis(vm, argument),
                ));
            }
            Ok(vm.string_or_empty(argument))
        }
    }
}

/// The number pattern a style names.
fn number_pattern_of(vm: &mut Vm<'_>, style: &str) -> NumberPattern {
    match style.to_ascii_lowercase().as_str() {
        "integer" => NumberPattern::parse("#,##0"),
        "percent" => NumberPattern::parse("#,##0%"),
        "currency" => currency_pattern(vm),
        "" => default_number_pattern(vm),
        other => NumberPattern::parse(&unquote(other)),
    }
}

/// The date pattern a style names.
fn date_pattern_of(kind: &str, style: &str) -> String {
    let style = style.to_ascii_lowercase();
    match style.as_str() {
        "" => {
            if kind.eq_ignore_ascii_case("date") {
                date_pattern("medium").to_string()
            } else {
                time_pattern("medium").to_string()
            }
        }
        "short" | "medium" | "long" | "full" => {
            if kind.eq_ignore_ascii_case("date") {
                date_pattern(&style).to_string()
            } else {
                time_pattern(&style).to_string()
            }
        }
        other => unquote(other),
    }
}

/// Whether a reference is a `java.util.Date`.
fn is_date(vm: &mut Vm<'_>, reference: ObjectRef) -> bool {
    vm.resolve_class_lenient_pub("java/util/Date")
        .is_some_and(|class| vm.is_instance(reference, class))
}

/// The milliseconds an argument holds: a date holds them, a number counts them.
fn argument_millis(vm: &mut Vm<'_>, argument: ObjectRef) -> i64 {
    if is_date(vm, argument) {
        return date_millis(vm, argument);
    }
    number_value(vm, argument).unwrap_or(0.0) as i64
}

/// Remove the quotes of a pattern, the way the JDK's pattern readers do.
fn unquote(pattern: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if ch == '\'' {
            if index + 1 < chars.len() && chars[index + 1] == '\'' {
                out.push('\'');
                index += 1;
            } else {
                index += 1;
                while index < chars.len() && chars[index] != '\'' {
                    out.push(chars[index]);
                    index += 1;
                }
                index += 1;
                continue;
            }
        } else {
            out.push(ch);
        }
        index += 1;
    }
    out
}

/// The pattern of a message format.
fn message_pattern(vm: &mut Vm<'_>, format: ObjectRef) -> String {
    vm.read_string_field(format, vm.class_of(format), "pattern")
        .unwrap_or_default()
}

/// The pattern of a message, written the way the JDK writes it.
fn message_to_pattern(vm: &mut Vm<'_>, format: ObjectRef) -> String {
    let pattern = message_pattern(vm, format);
    if parse_message(&pattern).is_err() {
        return pattern;
    }
    let literals = message_literals(vm, format);
    let numbers = message_argument_numbers(vm, format);
    let formats = message_formats(vm, format);
    let mut out = String::new();
    for (position, number) in numbers.iter().enumerate() {
        if let Some(literal) = literals.get(position) {
            fix_quotes(literal, &mut out);
        }
        out.push('{');
        out.push_str(&number.to_string());
        let sub = formats.get(position).copied().unwrap_or(ObjectRef::NULL);
        out.push_str(&pattern_from_format(vm, sub));
        out.push('}');
    }
    if let Some(literal) = literals.last() {
        fix_quotes(literal, &mut out);
    }
    out
}

/// The type and the style of a format, the way the JDK writes them.
fn pattern_from_format(vm: &mut Vm<'_>, format: ObjectRef) -> String {
    if format.is_null() {
        return String::new();
    }
    let class = vm.class_of(format);
    let kind = vm
        .read_string_field(format, class, "kind")
        .unwrap_or_default();
    let pattern = vm
        .read_string_field(format, class, "pattern")
        .unwrap_or_default();
    match kind.as_str() {
        "number" => {
            // The styles the JDK knows are written by name, the others by their pattern.
            let style = number_style(&pattern);
            if style.is_empty() {
                String::from(",number")
            } else {
                alloc::format!(",number,{style}")
            }
        }
        "date" | "time" => {
            let style = date_style(&pattern);
            // The default style, the medium one, is left out.
            if style.is_empty() || style == "medium" {
                alloc::format!(",{kind}")
            } else {
                alloc::format!(",{kind},{style}")
            }
        }
        "choice" => alloc::format!(",choice,{}", copy_and_quote_braces(&pattern)),
        _ => String::new(),
    }
}

/// The style of a number pattern, the way the JDK names it.
fn number_style(pattern: &str) -> String {
    match pattern {
        "#,##0.###" => String::new(),
        "#,##0" => String::from("integer"),
        "#,##0%" => String::from("percent"),
        other if other.starts_with('$') || other.ends_with('¥') || other.ends_with('€') => {
            String::from("currency")
        }
        other => copy_and_quote_braces(other),
    }
}

/// The style of a date pattern, the way the JDK names it.
fn date_style(pattern: &str) -> String {
    match pattern {
        "M/d/yy" => String::from("short"),
        "MMM d, yyyy" => String::from("medium"),
        "MMMM d, yyyy" => String::from("long"),
        "EEEE, MMMM d, yyyy" => String::from("full"),
        "h:mm a" => String::from("short"),
        "h:mm:ss a" => String::from("medium"),
        "h:mm:ss a z" => String::from("long"),
        other => copy_and_quote_braces(other),
    }
}

/// Quote the braces of a pattern that are not quoted yet, the way the JDK does.
fn copy_and_quote_braces(source: &str) -> String {
    let mut quoted = false;
    let mut needs_change = false;
    let mut chars: Vec<(char, bool)> = Vec::new();
    let units: Vec<char> = source.chars().collect();
    let mut index = 0;
    while index < units.len() {
        let ch = units[index];
        if ch == '\'' {
            if index + 1 < units.len() && units[index + 1] == '\'' {
                chars.push(('\'', quoted));
                index += 1;
            } else {
                quoted = !quoted;
            }
        } else {
            chars.push((ch, quoted));
            if (ch == '{' || ch == '}') && !quoted {
                needs_change = true;
            }
        }
        index += 1;
    }
    if !needs_change {
        return source.to_string();
    }
    let mut out = String::new();
    for (ch, in_quote) in chars {
        if !in_quote && (ch == '{' || ch == '}') {
            out.push('\'');
            out.push(ch);
            out.push('\'');
        } else {
            out.push(ch);
        }
    }
    out
}

/// Copy a literal text, fixing its quotes the way the JDK does: a quote is written twice and a
/// brace is put between quotes.
fn fix_quotes(text: &str, out: &mut String) {
    let mut quoted = false;
    for ch in text.chars() {
        match ch {
            '{' => {
                if !quoted {
                    out.push('\'');
                    quoted = true;
                }
                out.push(ch);
            }
            '\'' => out.push_str("''"),
            other => {
                if quoted {
                    out.push('\'');
                    quoted = false;
                }
                out.push(other);
            }
        }
    }
    if quoted {
        out.push('\'');
    }
}

/// The format a message uses for one of its arguments.
fn sub_format(vm: &mut Vm<'_>, kind: &str, style: &str) -> ObjectRef {
    let result: Result<Value, VmError> = match kind.to_ascii_lowercase().as_str() {
        "number" => {
            let pattern = match style.to_ascii_lowercase().as_str() {
                "integer" => NumberPattern::parse("#,##0"),
                "percent" => NumberPattern::parse("#,##0%"),
                "currency" => currency_pattern(vm),
                "" => default_number_pattern(vm),
                other => NumberPattern::parse(other),
            };
            number_format(vm, &pattern)
        }
        "date" | "time" => {
            let pattern = match style.to_ascii_lowercase().as_str() {
                "" => {
                    if kind.eq_ignore_ascii_case("date") {
                        date_pattern("medium").to_string()
                    } else {
                        time_pattern("medium").to_string()
                    }
                }
                "short" | "medium" | "long" | "full" => {
                    if kind.eq_ignore_ascii_case("date") {
                        date_pattern(&style.to_ascii_lowercase()).to_string()
                    } else {
                        time_pattern(&style.to_ascii_lowercase()).to_string()
                    }
                }
                other => other.to_string(),
            };
            date_format(vm, &pattern)
        }
        "choice" => {
            let instance = match vm.resolve_class("java/text/ChoiceFormat") {
                Ok(class) => vm.new_instance(class),
                Err(_) => Err(VmError::internal("ChoiceFormat is not available")),
            };
            match instance {
                Ok(instance) => {
                    // The pattern of a choice is written the way the JDK writes it.
                    let pattern = Choice::parse(style).pattern();
                    if let Ok(text) = vm.make_string(&pattern) {
                        if let Some((declaring, field)) = vm.find_field_opt(
                            vm.class_of(instance),
                            "pattern",
                            "Ljava/lang/String;",
                        ) {
                            vm.set_instance_ref(instance, declaring, field, text);
                        }
                    }
                    if let Ok(text) = vm.make_string("choice") {
                        if let Some((declaring, field)) =
                            vm.find_field_opt(vm.class_of(instance), "kind", "Ljava/lang/String;")
                        {
                            vm.set_instance_ref(instance, declaring, field, text);
                        }
                    }
                    object(instance)
                }
                Err(error) => Err(error),
            }
        }
        _ => object(ObjectRef::NULL),
    };
    match result {
        Ok(value) => value.as_ref(),
        Err(_) => ObjectRef::NULL,
    }
}

/// The format of the argument with the given index.
fn message_format_at(vm: &mut Vm<'_>, format: ObjectRef, index: i32) -> ObjectRef {
    let formats = message_formats(vm, format);
    formats
        .get(index as usize)
        .copied()
        .unwrap_or(ObjectRef::NULL)
}

/// The locale a message uses when none was given.
fn default_locale(vm: &mut Vm<'_>) -> ObjectRef {
    let Some(class) = vm.resolve_class_lenient_pub("java/util/Locale") else {
        return ObjectRef::NULL;
    };
    // The default locale is set when the class is initialized.
    let _ = vm.ensure_initialized(class);
    // `Locale.getDefault` is a static method, so it takes no receiver.
    let Some((declaring, method)) = vm.find_method(class, "getDefault", "()Ljava/util/Locale;")
    else {
        return ObjectRef::NULL;
    };
    vm.invoke_method(declaring, method, Vec::new())
        .map(|value| value.as_ref())
        .unwrap_or(ObjectRef::NULL)
}

/// Remember the locale of a message.
fn set_message_locale(vm: &mut Vm<'_>, message: ObjectRef, locale: ObjectRef) {
    if let Ok((declaring, field)) =
        vm.find_field(vm.class_of(message), "locale", "Ljava/util/Locale;")
    {
        vm.set_instance_ref(message, declaring, field, locale);
    }
}

/// The literals of a message format.
fn message_literals(vm: &mut Vm<'_>, format: ObjectRef) -> Vec<String> {
    let array = vm
        .read_ref_field(
            format,
            vm.class_of(format),
            "literals",
            "[Ljava/lang/String;",
        )
        .unwrap_or(ObjectRef::NULL);
    if array.is_null() {
        return Vec::new();
    }
    let mut literals = Vec::new();
    if let Ok(length) = vm.array_length(array) {
        for index in 0..length {
            if let Ok(value) = vm.array_get(array, index as i32) {
                literals.push(vm.string_or_empty(value.as_ref()));
            }
        }
    }
    literals
}

/// The argument numbers of a message format.
fn message_argument_numbers(vm: &mut Vm<'_>, format: ObjectRef) -> Vec<i32> {
    let array = vm
        .read_ref_field(format, vm.class_of(format), "argumentNumbers", "[I")
        .unwrap_or(ObjectRef::NULL);
    if array.is_null() {
        return Vec::new();
    }
    let mut numbers = Vec::new();
    if let Ok(length) = vm.array_length(array) {
        for index in 0..length {
            if let Ok(value) = vm.array_get(array, index as i32) {
                numbers.push(value.as_int());
            }
        }
    }
    numbers
}

/// The formats of the arguments of a message, in the order they appear.
fn message_formats(vm: &mut Vm<'_>, format: ObjectRef) -> Vec<ObjectRef> {
    let array = vm
        .read_ref_field(
            format,
            vm.class_of(format),
            "formats",
            "[Ljava/text/Format;",
        )
        .unwrap_or(ObjectRef::NULL);
    if array.is_null() {
        return Vec::new();
    }
    let mut formats = Vec::new();
    if let Ok(length) = vm.array_length(array) {
        for index in 0..length {
            if let Ok(value) = vm.array_get(array, index as i32) {
                formats.push(value.as_ref());
            }
        }
    }
    formats
}

/// Set the format of the argument with the given index.
fn set_message_format(vm: &mut Vm<'_>, format: ObjectRef, index: i32, value: ObjectRef) {
    let numbers = message_argument_numbers(vm, format);
    let Some(position) = numbers.iter().position(|number| *number == index) else {
        return;
    };
    let Some(formats) = vm.read_ref_field(
        format,
        vm.class_of(format),
        "formats",
        "[Ljava/text/Format;",
    ) else {
        return;
    };
    if formats.is_null() {
        return;
    }
    let _ = vm.array_set_ref(formats, position, value);
}

/// Store a pattern in a message format, after checking that it reads.
fn apply_message_pattern(
    vm: &mut Vm<'_>,
    format: ObjectRef,
    pattern: &str,
) -> Result<Value, VmError> {
    let message = parse_message(pattern)
        .map_err(|error| vm.throw_new("java/lang/IllegalArgumentException", Some(&error)))?;
    let class = vm.class_of(format);
    // The literal text, the argument numbers and the formats of the arguments.
    let literals = message.literals();
    let array = vm.allocate_array_of(ArrayComponent::Reference, literals.len())?;
    for (index, text) in literals.iter().enumerate() {
        let value = vm.make_string(text)?;
        vm.array_set_ref(array, index, value)?;
    }
    let (declaring, field) = vm.find_field(class, "literals", "[Ljava/lang/String;")?;
    vm.set_instance_ref(format, declaring, field, array);

    let numbers = message.argument_numbers();
    let numbers_array = vm.allocate_array_of(ArrayComponent::Int, numbers.len())?;
    for (index, number) in numbers.iter().enumerate() {
        vm.array_set(numbers_array, index as i32, Value::Int(*number), false)?;
    }
    let (declaring, field) = vm.find_field(class, "argumentNumbers", "[I")?;
    vm.set_instance_ref(format, declaring, field, numbers_array);

    // A format that names a type needs a locale to be built with.
    let locale = vm
        .read_ref_field(format, vm.class_of(format), "locale", "Ljava/util/Locale;")
        .unwrap_or(ObjectRef::NULL);
    let mut formats: Vec<ObjectRef> = Vec::new();
    for element in &message.elements {
        if let Element::Argument { kind, style, .. } = element {
            if !kind.is_empty() && locale.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            formats.push(sub_format(vm, kind, style));
        }
    }
    let formats_array = vm.allocate_array_of(ArrayComponent::Reference, formats.len())?;
    for (index, value) in formats.iter().enumerate() {
        vm.array_set_ref(formats_array, index, *value)?;
    }
    let (declaring, field) = vm.find_field(class, "formats", "[Ljava/text/Format;")?;
    vm.set_instance_ref(format, declaring, field, formats_array);

    let (declaring, field) = vm.find_field(class, "pattern", "Ljava/lang/String;")?;
    let text = vm.make_string(pattern)?;
    vm.set_instance_ref(format, declaring, field, text);
    void()
}

/// The largest argument index a message accepts, the way the JDK limits it.
const MAX_ARGUMENT_INDEX: i64 = 10_000;

/// `java.text.MessageFormat`.
pub(crate) const MESSAGE_FORMAT: oxjvm_vm::NativeClass = class(
    "java/text/MessageFormat",
    Some("java/text/Format"),
    &[],
    ACC_PUBLIC,
    &[
        field("pattern", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("literals", "[Ljava/lang/String;", ACC_PRIVATE, None),
        field("argumentNumbers", "[I", ACC_PRIVATE, None),
        field("formats", "[Ljava/text/Format;", ACC_PRIVATE, None),
        field("locale", "Ljava/util/Locale;", ACC_PRIVATE, None),
    ],
    &MESSAGE_FORMAT_METHODS,
    None,
);

const PARSE_EXCEPTION_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        crate::throwable::exception_ctor,
    ),
    method(
        "<init>",
        "(Ljava/lang/String;I)V",
        ACC_PUBLIC,
        |vm, context, args| {
            let this = receiver(args);
            crate::throwable::exception_ctor(vm, context, args)?;
            write_int_field(vm, this, "errorOffset", crate::int_arg(args, 2));
            void()
        },
    ),
    method("getErrorOffset", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "errorOffset"))
    }),
    method(
        "getMessage",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            object(
                vm.read_ref_field(this, class, "message", "Ljava/lang/String;")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
];

/// `java.text.ParseException`: a message that does not read back.
pub(crate) const PARSE_EXCEPTION: oxjvm_vm::NativeClass = class(
    "java/text/ParseException",
    Some("java/lang/Exception"),
    &[],
    ACC_PUBLIC,
    &[field("errorOffset", "I", ACC_PRIVATE, None)],
    &PARSE_EXCEPTION_METHODS,
    None,
);
