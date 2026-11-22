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
use crate::{class, field, int, int_arg, method, object, receiver, ref_arg, void};

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
    /// The per-mille style multiplies the value by a thousand.
    per_mille: bool,
    /// The pattern writes the value as a mantissa and a power of ten (`0.###E0`).
    scientific: bool,
    /// The digit positions the integer part offers; the repeating range in scientific form.
    max_integer: usize,
    /// The minimum digits the exponent is written with (`E00` writes two).
    min_exponent: usize,
    /// An explicit multiplier from `setMultiplier`, overriding the percent/per-mille default.
    multiplier: Option<i32>,
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
        let (mantissa, exponent_part) = match number.split_once('E') {
            Some((mantissa, exponent)) => (mantissa, Some(exponent)),
            None => (number.as_str(), None),
        };
        let (integer, fraction) = match mantissa.split_once('.') {
            Some((integer, fraction)) => (integer, fraction),
            None => (mantissa, ""),
        };
        let grouping = integer
            .rfind(',')
            .map_or(0, |index| integer.len() - index - 1);
        let mut min_integer = integer.chars().filter(|ch| *ch == '0').count();
        let min_fraction = fraction.chars().filter(|ch| *ch == '0').count();
        let max_fraction = fraction
            .chars()
            .filter(|ch| *ch == '0' || *ch == '#')
            .count();
        let max_integer = integer
            .chars()
            .filter(|ch| *ch == '0' || *ch == '#')
            .count();
        let scientific = exponent_part.is_some();
        if scientific {
            // The JDK writes at least one integer digit in scientific form.
            min_integer = min_integer.max(1);
        }
        let min_exponent =
            exponent_part.map_or(0, |part| part.chars().filter(|ch| *ch == '0').count());
        let percent = suffix.contains('%');
        let per_mille = suffix.contains('\u{2030}');
        Self {
            source: pattern.to_string(),
            prefix,
            suffix,
            grouping,
            min_integer,
            min_fraction,
            max_fraction,
            percent,
            per_mille,
            scientific,
            max_integer,
            min_exponent,
            multiplier: None,
        }
    }

    /// Format a number with this pattern.
    fn format(&self, value: f64) -> String {
        self.format_with(value, Rounding::HalfUp, false)
            .unwrap_or_else(|| oxjvm_vm::format::double_to_string(value))
    }

    /// Format with an explicit rounding mode; `None` when `UNNECESSARY` finds a tie.
    /// `compat` selects the legacy `jdk.compat.DecimalFormat` behavior, which rounds the
    /// double's full exact binary expansion instead of its shortest round-trip digits.
    fn format_with(&self, value: f64, mode: Rounding, compat: bool) -> Option<String> {
        let factor = match self.multiplier {
            Some(multiplier) => f64::from(multiplier),
            None if self.percent => 100.0,
            None if self.per_mille => 1000.0,
            None => 1.0,
        };
        let scaled = value * factor;
        if scaled.is_nan() {
            return Some(String::from("NaN"));
        }
        if scaled.is_infinite() {
            let mut text = String::new();
            if scaled.is_sign_negative() {
                text.push('-');
            }
            text.push_str(&self.prefix);
            text.push('\u{221e}');
            text.push_str(&self.suffix);
            return Some(text);
        }
        // The JDK keeps the sign of a negative zero, and of a value that rounds to zero.
        let negative = scaled.is_sign_negative();
        let magnitude = scaled.abs();
        let digits = self.max_fraction;
        // Ceiling and Floor point the other way for a negative value.
        let mode = if negative {
            match mode {
                Rounding::Ceiling => Rounding::Floor,
                Rounding::Floor => Rounding::Ceiling,
                other => other,
            }
        } else {
            mode
        };
        if self.scientific {
            return self.format_scientific(magnitude, negative, mode);
        }
        // The JDK formats a double from its shortest round-trip decimal digits, then rounds
        // at the requested fraction position, deciding ties from the value's exact binary
        // value.  When the rounding position lies beyond the shortest digits (very large or
        // very small magnitudes) no rounding touches them and they are kept as they are.
        let (integer_text, mut fraction_text) = if compat && magnitude != 0.0 {
            let text = exact_double_digits(magnitude);
            let (integer, fraction) = match text.split_once('.') {
                Some((integer, fraction)) => (integer, fraction),
                None => (text.as_str(), ""),
            };
            round_decimal_digits(integer, fraction, digits, mode)?
        } else if magnitude == 0.0 {
            (
                String::from("0"),
                alloc::format!("{:0width$}", 0, width = digits),
            )
        } else {
            let (significant, decimal_at) = shortest_digits(magnitude);
            let count = significant.len() as i32;
            let target = digits as i32 + decimal_at;
            if target < 0 {
                (
                    String::from("0"),
                    alloc::format!("{:0width$}", 0, width = digits),
                )
            } else if target >= count {
                split_fixed(&significant, decimal_at, digits)
            } else {
                match round_exact_scale(magnitude, target - decimal_at, mode) {
                    ExactRound::Digits(value) if value != 0 => {
                        let text = format_u128(value);
                        let len = text.len() as i32;
                        split_fixed(&text, decimal_at + len - target, digits)
                    }
                    ExactRound::Digits(_) => (
                        String::from("0"),
                        alloc::format!("{:0width$}", 0, width = digits),
                    ),
                    ExactRound::Unnecessary => return None,
                    ExactRound::Unsupported => {
                        let text = exact_double_digits(magnitude);
                        let (integer, fraction) = match text.split_once('.') {
                            Some((integer, fraction)) => (integer, fraction),
                            None => (text.as_str(), ""),
                        };
                        round_decimal_digits(integer, fraction, digits, mode)?
                    }
                }
            }
        };
        let grouped = group_digits(&integer_text, self.grouping);
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
        if digits > 0 {
            while fraction_text.len() > self.min_fraction && fraction_text.ends_with('0') {
                fraction_text.pop();
            }
            if !fraction_text.is_empty() {
                out.push('.');
                out.push_str(&fraction_text);
            }
        }
        out.push_str(&self.suffix);
        Some(out)
    }

    /// Scientific form: round to `max_integer + max_fraction` significant digits, then
    /// place the decimal point.  When the integer part offers more than one digit and more
    /// than the minimum, the JDK reads it as a repeating range and snaps the exponent to a
    /// multiple of it (engineering notation); otherwise the minimum integer digits set the
    /// exponent.
    fn format_scientific(&self, magnitude: f64, negative: bool, mode: Rounding) -> Option<String> {
        let max_int = self.max_integer.max(1);
        let significant = max_int + self.max_fraction;
        let (decimal_at, rounded) = if magnitude == 0.0 {
            (0i32, 0u128)
        } else if significant <= 38 {
            let mut decimal_at = decimal_exponent(magnitude) + 1;
            let scale = significant as i32 - decimal_at;
            let mut rounded = match round_exact_scale(magnitude, scale, mode) {
                ExactRound::Digits(value) => value,
                ExactRound::Unnecessary => return None,
                ExactRound::Unsupported => {
                    let mantissa = magnitude / signed_power_of_ten(decimal_at - 1);
                    let factor = power_of_ten(significant - 1);
                    let value = round_scaled(mantissa * factor, mode)?;
                    crate::math::floor(value) as u128
                }
            };
            if rounded >= power_of_ten_u128(significant) {
                rounded /= 10;
                decimal_at += 1;
            }
            (decimal_at, rounded)
        } else {
            let decimal_at = decimal_exponent(magnitude) + 1;
            let mantissa = magnitude / signed_power_of_ten(decimal_at - 1);
            let factor = power_of_ten(37);
            let value = round_scaled(mantissa * factor, mode)?;
            let mut rounded = crate::math::floor(value) as u128;
            let mut decimal_at = decimal_at;
            if rounded >= power_of_ten_u128(38) {
                rounded /= 10;
                decimal_at += 1;
            }
            (decimal_at, rounded)
        };
        let repeat = max_int as i32;
        let min_int = self.min_integer as i32;
        let (exponent, integer_digits) = scientific_exponent(decimal_at, repeat, min_int);
        let mut out = String::new();
        if negative {
            out.push('-');
        }
        out.push_str(&self.prefix);
        if decimal_at == 0 {
            for _ in 0..integer_digits {
                out.push('0');
            }
            if self.min_fraction > 0 {
                out.push('.');
                for _ in 0..self.min_fraction {
                    out.push('0');
                }
            }
        } else {
            let width = if significant <= 38 { significant } else { 38 };
            let text = alloc::format!("{rounded:0width$}");
            let split = (integer_digits as usize).min(text.len());
            out.push_str(&text[..split]);
            let mut fraction_text = text[split..].to_string();
            while fraction_text.len() > self.min_fraction && fraction_text.ends_with('0') {
                fraction_text.pop();
            }
            if !fraction_text.is_empty() {
                out.push('.');
                out.push_str(&fraction_text);
            }
        }
        out.push_str(&self.suffix);
        out.push('E');
        if exponent < 0 {
            out.push('-');
        }
        let exponent_text = alloc::format!("{}", exponent.abs());
        for _ in exponent_text.len()..self.min_exponent {
            out.push('0');
        }
        out.push_str(&exponent_text);
        Some(out)
    }
}

/// The JDK's `java.math.RoundingMode`, in ordinal order.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rounding {
    Up,
    Down,
    Ceiling,
    Floor,
    HalfUp,
    HalfDown,
    HalfEven,
    Unnecessary,
}

impl Rounding {
    /// The mode a `RoundingMode` instance's ordinal names.
    pub(crate) fn from_ordinal(ordinal: i32) -> Self {
        match ordinal {
            0 => Self::Up,
            1 => Self::Down,
            2 => Self::Ceiling,
            3 => Self::Floor,
            4 => Self::HalfUp,
            5 => Self::HalfDown,
            7 => Self::Unnecessary,
            _ => Self::HalfEven,
        }
    }
}

/// The outcome of rounding a double's exact binary value at a decimal scale.
enum ExactRound {
    /// The value times ten to the scale, rounded to a whole number.
    Digits(u128),
    /// `UNNECESSARY` found a nonzero fraction to discard.
    Unnecessary,
    /// The scale or the value is beyond the exact path; use float arithmetic.
    Unsupported,
}

/// Round `magnitude * 10^digits` to a whole number using the double's exact binary value,
/// the way the JDK does: a double is `m * 2^e`, so the scaled value is `m * 5^digits / 2^k`
/// with `k = -(e + digits)`, and the fraction's size follows from the low `k` bits alone.
/// Float multiplication would collapse near-ties (0.0000005 * 10^6 lands exactly on 0.5).
fn round_scaled_exact(magnitude: f64, digits: usize, mode: Rounding) -> ExactRound {
    if !magnitude.is_finite() {
        return ExactRound::Unsupported;
    }
    if magnitude == 0.0 {
        return ExactRound::Digits(0);
    }
    // m < 2^53 and 5^32 < 2^75, so the product stays inside u128 up to 32 digits.
    if digits > 32 {
        return ExactRound::Unsupported;
    }
    let bits = magnitude.to_bits();
    let raw_exponent = ((bits >> 52) & 0x7ff) as i32;
    let mantissa = bits & 0x000f_ffff_ffff_ffff;
    let (m, e) = if raw_exponent == 0 {
        (mantissa, -1074i32)
    } else {
        (mantissa | (1u64 << 52), raw_exponent - 1023 - 52)
    };
    let mut scaled = u128::from(m);
    for _ in 0..digits {
        scaled *= 5;
    }
    let k = -(e + digits as i32);
    if k <= 0 {
        // The scaled value is already a whole number: m * 5^digits * 2^(-k).
        let shift = (-k) as u32;
        return match shl_exact(scaled, shift) {
            Some(value) => ExactRound::Digits(value),
            None => ExactRound::Unsupported,
        };
    }
    let k = k as u32;
    let (quotient, remainder, half) = if k >= 129 {
        // The remainder is the whole product, below 2^128, and half is 2^(k-1) >= 2^128.
        (0u128, scaled, u128::MAX)
    } else {
        (scaled >> k, scaled & ((1u128 << k) - 1), 1u128 << (k - 1))
    };
    let increment = match mode {
        Rounding::Up | Rounding::Ceiling => remainder != 0,
        Rounding::Down | Rounding::Floor => false,
        Rounding::HalfUp => remainder >= half,
        Rounding::HalfDown => remainder > half,
        Rounding::HalfEven => remainder > half || (remainder == half && quotient % 2 == 1),
        Rounding::Unnecessary => {
            if remainder != 0 {
                return ExactRound::Unnecessary;
            }
            false
        }
    };
    ExactRound::Digits(quotient + u128::from(increment))
}

/// A power of ten as an exact integer, for scales the exact path handles.
fn power_of_ten_u128(exponent: usize) -> u128 {
    let mut value = 1u128;
    for _ in 0..exponent {
        value *= 10;
    }
    value
}

/// A power of ten with a signed exponent, as a float.
fn signed_power_of_ten(exponent: i32) -> f64 {
    if exponent >= 0 {
        power_of_ten(exponent as usize)
    } else {
        1.0 / power_of_ten((-exponent) as usize)
    }
}

/// A left shift that reports overflow instead of silently dropping high bits.
fn shl_exact(value: u128, shift: u32) -> Option<u128> {
    if shift >= 128 || value.leading_zeros() < shift {
        None
    } else {
        Some(value << shift)
    }
}

/// Round `num / den` to a whole number under the given mode.
fn round_ratio(num: u128, den: u128, mode: Rounding) -> ExactRound {
    let quotient = num / den;
    let remainder = num % den;
    let half = den / 2;
    let tie = remainder == half && den % 2 == 0;
    let increment = match mode {
        Rounding::Up | Rounding::Ceiling => remainder != 0,
        Rounding::Down | Rounding::Floor => false,
        Rounding::HalfUp => remainder > half || tie,
        Rounding::HalfDown => remainder > half,
        Rounding::HalfEven => remainder > half || (tie && quotient % 2 == 1),
        Rounding::Unnecessary => {
            if remainder != 0 {
                return ExactRound::Unnecessary;
            }
            false
        }
    };
    ExactRound::Digits(quotient + u128::from(increment))
}

/// Round `magnitude * 10^scale` for a scale that may be negative: the value is then
/// `m * 2^(e-j) / 5^j` with `j = -scale`, a ratio of two exact integers.
fn round_exact_scale(magnitude: f64, scale: i32, mode: Rounding) -> ExactRound {
    if scale >= 0 {
        return round_scaled_exact(magnitude, scale as usize, mode);
    }
    if !magnitude.is_finite() {
        return ExactRound::Unsupported;
    }
    if magnitude == 0.0 {
        return ExactRound::Digits(0);
    }
    let j = (-scale) as usize;
    if j > 32 {
        return ExactRound::Unsupported;
    }
    let bits = magnitude.to_bits();
    let raw_exponent = ((bits >> 52) & 0x7ff) as i32;
    let mantissa = bits & 0x000f_ffff_ffff_ffff;
    let (m, e) = if raw_exponent == 0 {
        (mantissa, -1074i32)
    } else {
        (mantissa | (1u64 << 52), raw_exponent - 1023 - 52)
    };
    let mut five = 1u128;
    for _ in 0..j {
        five *= 5;
    }
    let t = e - j as i32;
    let (num, den) = if t >= 0 {
        match shl_exact(u128::from(m), t as u32) {
            Some(num) => (num, five),
            None => return ExactRound::Unsupported,
        }
    } else {
        match shl_exact(five, (-t) as u32) {
            Some(den) => (u128::from(m), den),
            None => return ExactRound::Unsupported,
        }
    };
    round_ratio(num, den, mode)
}

/// Compare a double's exact binary value with `10^power`, exactly where the
/// cross-multiplication fits in u128 and with float arithmetic otherwise.
fn cmp_pow10(magnitude: f64, power: i32) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    let bits = magnitude.to_bits();
    let raw_exponent = ((bits >> 52) & 0x7ff) as i32;
    let mantissa = bits & 0x000f_ffff_ffff_ffff;
    let (m, e) = if raw_exponent == 0 {
        (mantissa, -1074i32)
    } else {
        (mantissa | (1u64 << 52), raw_exponent - 1023 - 52)
    };
    if power >= 0 {
        if power <= 55 {
            let mut five = 1u128;
            for _ in 0..power {
                five *= 5;
            }
            if e >= power {
                if let Some(lhs) = shl_exact(u128::from(m), (e - power) as u32) {
                    return lhs.cmp(&five);
                }
            } else if let Some(rhs) = shl_exact(five, (power - e) as u32) {
                return u128::from(m).cmp(&rhs);
            }
        }
    } else {
        let q = -power;
        if e + q >= 0 {
            // m * 5^q * 2^(e+q) is a whole number, at least one.
            return Ordering::Greater;
        }
        if q <= 32 && -(e + q) < 128 {
            let mut five = 1u128;
            for _ in 0..q {
                five *= 5;
            }
            let lhs = u128::from(m) * five;
            let rhs = 1u128 << (-(e + q)) as u32;
            return lhs.cmp(&rhs);
        }
    }
    let reference = signed_power_of_ten(power);
    magnitude.partial_cmp(&reference).unwrap_or(Ordering::Equal)
}

/// The exponent and integer-digit count of scientific form: a repeating range snaps the
/// exponent to a multiple of the maximum integer digits; otherwise the minimum sets it.
fn scientific_exponent(decimal_at: i32, repeat: i32, min_int: i32) -> (i32, i32) {
    if decimal_at == 0 {
        let min_used = if repeat > 1 && repeat > min_int {
            1
        } else {
            min_int.max(1)
        };
        (0, min_used)
    } else if repeat > 1 && repeat > min_int {
        let exponent = if decimal_at >= 1 {
            ((decimal_at - 1) / repeat) * repeat
        } else {
            ((decimal_at - repeat) / repeat) * repeat
        };
        (exponent, decimal_at - exponent)
    } else {
        (decimal_at - min_int, min_int.max(1))
    }
}

/// Whether the discarded tail of a digit string is above half, exactly half, or below,
/// and whether it is nonzero at all.
fn digit_tail(rest: &str) -> (bool, bool, bool) {
    let bytes = rest.as_bytes();
    let first = bytes[0];
    let nonzero = bytes.iter().any(|byte| *byte != b'0');
    let tail_zero = bytes[1..].iter().all(|byte| *byte == b'0');
    let greater = first > b'5' || (first == b'5' && !tail_zero);
    let tie = first == b'5' && tail_zero;
    (greater, tie, nonzero)
}

/// Whether a rounding mode rounds up given the discarded tail and the last kept digit.
fn rounds_up(mode: Rounding, greater: bool, tie: bool, nonzero: bool, last_odd: bool) -> bool {
    match mode {
        Rounding::Up | Rounding::Ceiling => nonzero,
        Rounding::Down | Rounding::Floor => false,
        Rounding::HalfUp => greater || tie,
        Rounding::HalfDown => greater,
        Rounding::HalfEven => greater || (tie && last_odd),
        Rounding::Unnecessary => false,
    }
}

/// Add one to the number `integer.fraction`, carrying through both parts.
fn increment_digits(integer: &str, fraction: &str) -> (String, String) {
    let mut frac: Vec<u8> = fraction.bytes().collect();
    let mut carry = true;
    for byte in frac.iter_mut().rev() {
        if *byte == b'9' {
            *byte = b'0';
        } else {
            *byte += 1;
            carry = false;
            break;
        }
    }
    let mut int: Vec<u8> = integer.bytes().collect();
    if carry {
        for byte in int.iter_mut().rev() {
            if *byte == b'9' {
                *byte = b'0';
            } else {
                *byte += 1;
                carry = false;
                break;
            }
        }
        if carry {
            int.insert(0, b'1');
        }
    }
    (
        String::from_utf8(int).unwrap_or_default(),
        String::from_utf8(frac).unwrap_or_default(),
    )
}

/// The last digit of the kept part, for half-even rounding.
fn last_digit_odd(integer: &str, fraction: &str) -> bool {
    let digit = fraction
        .bytes()
        .last()
        .or_else(|| integer.bytes().last())
        .unwrap_or(b'0');
    (digit - b'0') % 2 == 1
}

/// Format the exact decimal digits of a `BigInteger` or `BigDecimal` under a format's
/// settings, without going through a double (which would lose digits).
fn format_exact_decimal(vm: &mut Vm<'_>, format: ObjectRef, text: &str) -> Result<String, VmError> {
    let (pattern, mode) = number_format_state(vm, format);
    let negative = text.starts_with('-');
    let digits = text.trim_start_matches(['-', '+']);
    let (integer, fraction) = match digits.split_once('.') {
        Some((integer, fraction)) => (integer, fraction),
        None => (digits, ""),
    };
    let trimmed = integer.trim_start_matches('0');
    let integer = if trimmed.is_empty() { "0" } else { trimmed };
    let mode = if negative {
        match mode {
            Rounding::Ceiling => Rounding::Floor,
            Rounding::Floor => Rounding::Ceiling,
            other => other,
        }
    } else {
        mode
    };
    let unnecessary =
        |vm: &mut Vm<'_>| vm.throw_new("java/lang/ArithmeticException", Some("Rounding necessary"));
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    out.push_str(&pattern.prefix);
    if pattern.scientific {
        let max_int = pattern.max_integer.max(1);
        let significant = max_int + pattern.max_fraction;
        let all = alloc::format!("{integer}{fraction}");
        let all = all.trim_end_matches('0');
        let zero = all.is_empty() || integer == "0" && fraction.bytes().all(|b| b == b'0');
        let (decimal_at, rounded) = if zero {
            (0i32, String::from("0"))
        } else {
            let mut decimal_at = integer.len() as i32;
            let mut rounded = if all.len() <= significant {
                let mut text = all.to_string();
                while text.len() < significant {
                    text.push('0');
                }
                text
            } else {
                let kept = &all[..significant];
                let rest = &all[significant..];
                let (greater, tie, nonzero) = digit_tail(rest);
                if mode == Rounding::Unnecessary && nonzero {
                    return Err(unnecessary(vm));
                }
                let mut text = kept.to_string();
                if rounds_up(mode, greater, tie, nonzero, last_digit_odd(kept, "")) {
                    let (int_part, _) = increment_digits(&text, "");
                    text = int_part;
                }
                if text.len() > significant {
                    text.truncate(significant);
                    decimal_at += 1;
                }
                text
            };
            if rounded.len() < significant {
                let mut text = rounded;
                while text.len() < significant {
                    text.push('0');
                }
                rounded = text;
            }
            (decimal_at, rounded)
        };
        let (exponent, integer_digits) =
            scientific_exponent(decimal_at, max_int as i32, pattern.min_integer as i32);
        if decimal_at == 0 {
            for _ in 0..integer_digits {
                out.push('0');
            }
            if pattern.min_fraction > 0 {
                out.push('.');
                for _ in 0..pattern.min_fraction {
                    out.push('0');
                }
            }
        } else {
            let split = (integer_digits as usize).min(rounded.len());
            out.push_str(&rounded[..split]);
            let mut fraction_text = rounded[split..].to_string();
            while fraction_text.len() > pattern.min_fraction && fraction_text.ends_with('0') {
                fraction_text.pop();
            }
            if !fraction_text.is_empty() {
                out.push('.');
                out.push_str(&fraction_text);
            }
        }
        out.push_str(&pattern.suffix);
        out.push('E');
        if exponent < 0 {
            out.push('-');
        }
        let exponent_text = alloc::format!("{}", exponent.abs());
        for _ in exponent_text.len()..pattern.min_exponent {
            out.push('0');
        }
        out.push_str(&exponent_text);
        return Ok(out);
    }
    // Plain form: round the fraction at the maximum fraction digits, carrying into the
    // integer part, then group and pad.
    let max_fraction = pattern.max_fraction;
    let (mut integer_text, mut fraction_text) = if fraction.len() <= max_fraction {
        let mut fraction_text = fraction.to_string();
        while fraction_text.len() < max_fraction {
            fraction_text.push('0');
        }
        (integer.to_string(), fraction_text)
    } else {
        let kept = &fraction[..max_fraction];
        let rest = &fraction[max_fraction..];
        let (greater, tie, nonzero) = digit_tail(rest);
        if mode == Rounding::Unnecessary && nonzero {
            return Err(unnecessary(vm));
        }
        if rounds_up(mode, greater, tie, nonzero, last_digit_odd(integer, kept)) {
            increment_digits(integer, kept)
        } else {
            (integer.to_string(), kept.to_string())
        }
    };
    let max_integer_digits = read_number_digits(vm, format, "maxIntegerDigits", i32::MAX);
    if max_integer_digits >= 0 && integer_text.len() > max_integer_digits as usize {
        let keep = integer_text.len() - max_integer_digits as usize;
        integer_text = integer_text[keep..].to_string();
    }
    let grouped = group_digits(&integer_text, pattern.grouping);
    if grouped.len() < pattern.min_integer {
        for _ in 0..(pattern.min_integer - grouped.len()) {
            out.push('0');
        }
    }
    out.push_str(&grouped);
    while fraction_text.len() > pattern.min_fraction && fraction_text.ends_with('0') {
        fraction_text.pop();
    }
    if !fraction_text.is_empty() {
        out.push('.');
        out.push_str(&fraction_text);
    }
    out.push_str(&pattern.suffix);
    Ok(out)
}

/// The decimal exponent of a magnitude: the largest `e` with `10^e <= magnitude`.
fn decimal_exponent(magnitude: f64) -> i32 {
    use core::cmp::Ordering;
    let bits = magnitude.to_bits();
    let raw_exponent = ((bits >> 52) & 0x7ff) as i32;
    let exp2 = if raw_exponent == 0 {
        -1074
    } else {
        raw_exponent - 1023
    };
    let mut exponent = crate::math::floor(f64::from(exp2) * core::f64::consts::LOG10_2) as i32;
    while cmp_pow10(magnitude, exponent + 1) != Ordering::Less {
        exponent += 1;
    }
    while cmp_pow10(magnitude, exponent) == Ordering::Less {
        exponent -= 1;
    }
    exponent
}

/// Fixed-point text with exactly `digits` fraction digits, the way `String.format` and
/// `java.util.Formatter` write `%f`: the shortest round-trip digits, rounded half-up at
/// the requested position, with ties decided from the exact binary value.
pub(crate) fn format_fixed_jdk(value: f64, digits: usize) -> String {
    if value.is_nan() {
        return String::from("NaN");
    }
    if value.is_infinite() {
        return String::from(if value > 0.0 { "Infinity" } else { "-Infinity" });
    }
    let mut pattern = NumberPattern::parse("0");
    pattern.grouping = 0;
    pattern.min_fraction = digits;
    pattern.max_fraction = digits;
    pattern
        .format_with(value, Rounding::HalfUp, false)
        .unwrap_or_else(|| oxjvm_vm::format::double_to_string(value))
}

/// `java.util.Formatter` `%e`/`%E`: scientific notation with a fixed number of
/// fraction digits (default 6) and a signed, at-least-two-digit exponent.
pub(crate) fn format_scientific_jdk(value: f64, digits: usize, upper: bool) -> String {
    if value.is_nan() {
        return String::from("NaN");
    }
    if value.is_infinite() {
        return String::from(if value > 0.0 { "Infinity" } else { "-Infinity" });
    }
    let e = if upper { 'E' } else { 'e' };
    let sign = if value.is_sign_negative() { "-" } else { "" };
    let magnitude = value.abs();
    if magnitude == 0.0 {
        let frac = alloc::format!("{:0width$}", 0, width = digits);
        return alloc::format!("{sign}0.{frac}{e}+00");
    }
    let (significant, decimal_at) = shortest_digits(magnitude);
    let mut exponent = decimal_at - 1;
    let mantissa_text = if significant.len() == 1 {
        significant.clone()
    } else {
        alloc::format!("{}.{}", &significant[..1], &significant[1..])
    };
    let mantissa: f64 = mantissa_text.parse().unwrap_or(0.0);
    let fixed = format_fixed_jdk(mantissa, digits);
    let (int_part, frac_part) = fixed.split_once('.').unwrap_or((fixed.as_str(), ""));
    let mut int_part = int_part.to_string();
    let mut frac_part = frac_part.to_string();
    if int_part.len() > 1 {
        frac_part = alloc::format!("{}{}", &int_part[1..], frac_part);
        frac_part.truncate(digits);
        int_part.truncate(1);
        exponent += 1;
    }
    while frac_part.len() < digits {
        frac_part.push('0');
    }
    let exp_sign = if exponent < 0 { '-' } else { '+' };
    alloc::format!(
        "{sign}{int_part}.{frac_part}{e}{exp_sign}{:02}",
        exponent.abs()
    )
}

/// The digits of a whole number held as an exact integer.
fn format_u128(value: u128) -> String {
    alloc::format!("{value}")
}

/// The shortest round-trip decimal digits of a finite, non-negative double and the position
/// of the decimal point within them, the way the JDK's digit list stores a double.  Rust's
/// `Display` already produces the shortest round-trip form without an exponent.
fn shortest_digits(magnitude: f64) -> (String, i32) {
    let text = alloc::format!("{magnitude}");
    let (integer, fraction) = match text.split_once('.') {
        Some((integer, fraction)) => (integer, fraction),
        None => (text.as_str(), ""),
    };
    let combined = alloc::format!("{integer}{fraction}");
    let leading = combined.bytes().take_while(|byte| *byte == b'0').count();
    let tail = &combined[leading..];
    let end = tail
        .bytes()
        .rposition(|byte| byte != b'0')
        .map_or(0, |index| index + 1);
    let significant = &tail[..end];
    if significant.is_empty() {
        return (String::from("0"), 0);
    }
    let decimal_at = integer.len() as i32 - leading as i32;
    (significant.to_string(), decimal_at)
}

/// Split a rounded digit string with its decimal-point position into an integer part and a
/// fraction part padded to exactly `digits` places.
fn split_fixed(rounded: &str, decimal_at: i32, digits: usize) -> (String, String) {
    let len = rounded.len() as i32;
    if decimal_at >= len {
        let mut integer = rounded.to_string();
        for _ in 0..(decimal_at - len) {
            integer.push('0');
        }
        (integer, alloc::format!("{:0width$}", 0, width = digits))
    } else if decimal_at > 0 {
        let split = decimal_at as usize;
        let mut fraction = rounded[split..].to_string();
        while fraction.len() < digits {
            fraction.push('0');
        }
        fraction.truncate(digits);
        (rounded[..split].to_string(), fraction)
    } else {
        let mut fraction = String::new();
        for _ in 0..(-decimal_at) {
            fraction.push('0');
        }
        fraction.push_str(rounded);
        while fraction.len() < digits {
            fraction.push('0');
        }
        fraction.truncate(digits);
        (String::from("0"), fraction)
    }
}

/// Round the fraction of an exact decimal digit string at `max_fraction` digits, carrying
/// into the integer part; `None` when `UNNECESSARY` would have to discard a nonzero tail.
/// The returned fraction always holds exactly `max_fraction` digits.
fn round_decimal_digits(
    integer: &str,
    fraction: &str,
    max_fraction: usize,
    mode: Rounding,
) -> Option<(String, String)> {
    if fraction.len() <= max_fraction {
        let mut fraction_text = fraction.to_string();
        while fraction_text.len() < max_fraction {
            fraction_text.push('0');
        }
        return Some((integer.to_string(), fraction_text));
    }
    let kept = &fraction[..max_fraction];
    let rest = &fraction[max_fraction..];
    let (greater, tie, nonzero) = digit_tail(rest);
    if mode == Rounding::Unnecessary && nonzero {
        return None;
    }
    if rounds_up(mode, greater, tie, nonzero, last_digit_odd(integer, kept)) {
        Some(increment_digits(integer, kept))
    } else {
        Some((integer.to_string(), kept.to_string()))
    }
}

/// The exact decimal expansion of a finite, non-negative double, without an exponent.
/// A double is `m * 2^e`; for `e >= 0` that is the whole number `m << e`, and for `e < 0`
/// it is `m * 5^(-e)` with the decimal point placed `-e` digits from the right.
fn exact_double_digits(magnitude: f64) -> String {
    let bits = magnitude.to_bits();
    let raw_exponent = ((bits >> 52) & 0x7ff) as i32;
    let mantissa = bits & 0x000f_ffff_ffff_ffff;
    let (m, e) = if raw_exponent == 0 {
        (mantissa, -1074i32)
    } else {
        (mantissa | (1u64 << 52), raw_exponent - 1023 - 52)
    };
    if m == 0 {
        return String::from("0");
    }
    let mut big = BigUint::from_u64(m);
    if e >= 0 {
        big.shl(e as usize);
        return big.to_decimal();
    }
    let k = (-e) as usize;
    for _ in 0..k {
        big.mul_small(5);
    }
    let digits = big.to_decimal();
    let split = digits.len().saturating_sub(k);
    if split == 0 {
        let mut text = String::new();
        for _ in 0..(k + 1 - digits.len()) {
            text.push('0');
        }
        text.push_str(&digits);
        let point = text.len() - k;
        alloc::format!("{}.{}", &text[..point], &text[point..])
    } else {
        alloc::format!("{}.{}", &digits[..split], &digits[split..])
    }
}

/// A whole number of arbitrary size, held as little-endian 64-bit limbs, for the exact
/// decimal expansion of doubles beyond the u128 range.
struct BigUint {
    limbs: Vec<u64>,
}

impl BigUint {
    fn from_u64(value: u64) -> Self {
        Self { limbs: vec![value] }
    }

    fn is_zero(&self) -> bool {
        self.limbs.iter().all(|limb| *limb == 0)
    }

    fn mul_small(&mut self, factor: u64) {
        let mut carry = 0u128;
        for limb in self.limbs.iter_mut() {
            let product = u128::from(*limb) * u128::from(factor) + carry;
            *limb = product as u64;
            carry = product >> 64;
        }
        while carry != 0 {
            self.limbs.push(carry as u64);
            carry >>= 64;
        }
    }

    fn shl(&mut self, bits: usize) {
        let whole = bits / 64;
        let part = bits % 64;
        if whole != 0 {
            let mut shifted = vec![0u64; whole];
            shifted.extend_from_slice(&self.limbs);
            self.limbs = shifted;
        }
        if part != 0 {
            let mut carry = 0u64;
            for limb in self.limbs.iter_mut() {
                let next = *limb >> (64 - part);
                *limb = (*limb << part) | carry;
                carry = next;
            }
            if carry != 0 {
                self.limbs.push(carry);
            }
        }
    }

    /// Divide by a small divisor in place, returning the remainder.
    fn div_small(&mut self, divisor: u64) -> u64 {
        let mut remainder = 0u128;
        for limb in self.limbs.iter_mut().rev() {
            let current = (remainder << 64) | u128::from(*limb);
            *limb = (current / u128::from(divisor)) as u64;
            remainder = current % u128::from(divisor);
        }
        while self.limbs.len() > 1 && *self.limbs.last().unwrap_or(&0) == 0 {
            self.limbs.pop();
        }
        remainder as u64
    }

    fn to_decimal(&self) -> String {
        let mut copy = Self {
            limbs: self.limbs.clone(),
        };
        if copy.is_zero() {
            return String::from("0");
        }
        let mut chunks = Vec::new();
        while !copy.is_zero() {
            chunks.push(copy.div_small(1_000_000_000));
        }
        let mut text = alloc::format!("{}", chunks.pop().unwrap_or(0));
        while let Some(chunk) = chunks.pop() {
            text.push_str(&alloc::format!("{chunk:09}"));
        }
        text
    }
}

/// Round a scaled value to an integer under the given mode; `None` when `UNNECESSARY`
/// would have to discard a nonzero fraction.
fn round_scaled(value: f64, mode: Rounding) -> Option<f64> {
    let floor = crate::math::floor(value);
    let ceil = -crate::math::floor(-value);
    let magnitude = value.abs();
    let mag_floor = crate::math::floor(magnitude);
    let fraction = magnitude - mag_floor;
    let sign = if value < 0.0 { -1.0 } else { 1.0 };
    let rounded = match mode {
        Rounding::Up => sign * -crate::math::floor(-magnitude),
        Rounding::Down => sign * mag_floor,
        Rounding::Ceiling => ceil,
        Rounding::Floor => floor,
        Rounding::HalfUp => sign * crate::math::floor(magnitude + 0.5),
        Rounding::HalfDown => {
            if fraction == 0.5 {
                sign * mag_floor
            } else {
                sign * crate::math::floor(magnitude + 0.5)
            }
        }
        Rounding::HalfEven => {
            if fraction > 0.5 || (fraction == 0.5 && mag_floor as i64 % 2 != 0) {
                sign * (mag_floor + 1.0)
            } else {
                sign * mag_floor
            }
        }
        Rounding::Unnecessary => {
            if value != floor {
                return None;
            }
            value
        }
    };
    Some(rounded)
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
            let this = receiver(args);
            let value = args[1].as_long() as f64;
            let (class, method) =
                vm.resolve_virtual_method(this, "format", "(D)Ljava/lang/String;")?;
            vm.invoke_method(class, method, vec![Value::Ref(this), Value::Double(value)])
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
pub(crate) fn is_double(vm: &mut Vm<'_>, reference: ObjectRef) -> bool {
    vm.resolve_class_lenient_pub("java/lang/Double")
        .is_some_and(|class| vm.is_instance(reference, class))
}

/// Whether a reference is a `java.lang.Float`.
pub(crate) fn is_float(vm: &mut Vm<'_>, reference: ObjectRef) -> bool {
    vm.resolve_class_lenient_pub("java/lang/Float")
        .is_some_and(|class| vm.is_instance(reference, class))
}

/// Whether a reference is a `java.lang.Long`.
pub(crate) fn is_long(vm: &mut Vm<'_>, reference: ObjectRef) -> bool {
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

/// Append formatted text to a `StringBuffer` and return the buffer, as the
/// `format(value, StringBuffer, FieldPosition)` overloads do.
/// Record the integer/fraction field range of a formatted number in a
/// `FieldPosition`, relative to the buffer length before the append.
fn set_number_field_position(
    vm: &mut Vm<'_>,
    position: ObjectRef,
    text: &str,
    base: usize,
) -> Result<(), VmError> {
    if position.is_null() {
        return Ok(());
    }
    let field = read_int_field(vm, position, "field");
    if field != 0 && field != 1 {
        return Ok(());
    }
    let separator = text.find('.');
    let (start, end) = if field == 0 {
        let region_end = separator.unwrap_or(text.len());
        let region = &text[..region_end];
        let first = region.find(|c: char| c.is_ascii_digit());
        let last = region.rfind(|c: char| c.is_ascii_digit());
        match (first, last) {
            (Some(a), Some(b)) => (a, b + 1),
            _ => return Ok(()),
        }
    } else {
        let Some(sep) = separator else {
            return Ok(());
        };
        let region = &text[sep + 1..];
        let first = region.find(|c: char| c.is_ascii_digit());
        let last = region.rfind(|c: char| c.is_ascii_digit());
        match (first, last) {
            (Some(a), Some(b)) => (sep + 1 + a, sep + 1 + b + 1),
            _ => return Ok(()),
        }
    };
    write_int_field(vm, position, "beginIndex", (base + start) as i32);
    write_int_field(vm, position, "endIndex", (base + end) as i32);
    Ok(())
}

/// The current length of a `StringBuffer`/`StringBuilder`.
fn buffer_length(vm: &mut Vm<'_>, buffer: ObjectRef) -> Result<usize, VmError> {
    if buffer.is_null() {
        return Ok(0);
    }
    let (class, method) = vm.resolve_virtual_method(buffer, "length", "()I")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(buffer)])?
        .as_int()
        .max(0) as usize)
}

fn append_format_text(vm: &mut Vm<'_>, buffer: ObjectRef, text: &str) -> Result<Value, VmError> {
    let string = vm.make_string(text)?;
    let (class, method) = vm.resolve_virtual_method(
        buffer,
        "append",
        "(Ljava/lang/String;)Ljava/lang/StringBuffer;",
    )?;
    vm.invoke_method(class, method, vec![Value::Ref(buffer), Value::Ref(string)])
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
        field("field", "I", ACC_PRIVATE, None),
    ],
    &FIELD_POSITION_METHODS,
    None,
);

const FIELD_POSITION_METHODS: [oxjvm_vm::NativeMethodDef; 6] = [
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        write_int_field(vm, this, "field", int_arg(args, 1));
        write_int_field(vm, this, "beginIndex", 0);
        write_int_field(vm, this, "endIndex", 0);
        void()
    }),
    method("getField", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "field"))
    }),
    method("getBeginIndex", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "beginIndex"))
    }),
    method("getEndIndex", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "endIndex"))
    }),
    method("setBeginIndex", "(I)V", ACC_PUBLIC, |vm, _, args| {
        write_int_field(vm, receiver(args), "beginIndex", int_arg(args, 1));
        void()
    }),
    method("setEndIndex", "(I)V", ACC_PUBLIC, |vm, _, args| {
        write_int_field(vm, receiver(args), "endIndex", int_arg(args, 1));
        void()
    }),
];

const PARSE_POSITION_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("<init>", "(I)V", ACC_PUBLIC, |vm, _, args| {
        write_int_field(vm, receiver(args), "index", int_arg(args, 1));
        void()
    }),
    method("getIndex", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "index"))
    }),
    method("setIndex", "(I)V", ACC_PUBLIC, |vm, _, args| {
        write_int_field(vm, receiver(args), "index", int_arg(args, 1));
        void()
    }),
    method("getErrorIndex", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int_field(vm, receiver(args), "errorIndex"))
    }),
    method("setErrorIndex", "(I)V", ACC_PUBLIC, |vm, _, args| {
        write_int_field(vm, receiver(args), "errorIndex", int_arg(args, 1));
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

const NUMBER_FORMAT_METHODS: [oxjvm_vm::NativeMethodDef; 47] = [
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
        |vm, _, args| {
            let locale = ref_arg(args, 0);
            let country = if locale.is_null() {
                default_country(vm)
            } else {
                vm.read_string_field(locale, vm.class_of(locale), "country")
                    .unwrap_or_default()
            };
            let symbol = if country.is_empty() {
                "\u{00a4}"
            } else {
                currency_symbol(&country)
            };
            let mut pattern = NumberPattern::parse(&alloc::format!("{symbol}#,##0.00"));
            pattern.min_fraction = 2;
            pattern.max_fraction = 2;
            let value = number_format(vm, &pattern)?;
            let instance = value.as_ref();
            let class = vm.class_of(instance);
            if let Some((declaring, field)) =
                vm.find_field_opt(class, "currencySymbol", "Ljava/lang/String;")
            {
                let text = vm.make_string(symbol)?;
                vm.set_instance_ref(instance, declaring, field, text);
            }
            Ok(value)
        },
    ),
    method(
        "getCompactNumberInstance",
        "()Ljava/text/NumberFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| compact_number_format(vm, "#,##0.###", ObjectRef::NULL, "SHORT"),
    ),
    method(
        "getCompactNumberInstance",
        "(Ljava/util/Locale;)Ljava/text/NumberFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| compact_number_format(vm, "#,##0.###", ObjectRef::NULL, "SHORT"),
    ),
    method(
        "getCompactNumberInstance",
        "(Ljava/util/Locale;Ljava/text/NumberFormat$Style;)Ljava/text/NumberFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let style = vm
                .read_string_field(ref_arg(args, 1), vm.class_of(ref_arg(args, 1)), "name")
                .unwrap_or_default();
            let style = if style == "LONG" { "LONG" } else { "SHORT" };
            compact_number_format(vm, "#,##0.###", ObjectRef::NULL, style)
        },
    ),
    method(
        "format",
        "(Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = number_argument_text(vm, receiver(args), ref_arg(args, 1))?;
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "format",
        "(D)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = format_number_required(vm, receiver(args), args[1].as_double())?;
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "format",
        "(J)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let value = args[1].as_long() as f64;
            let (class, method) =
                vm.resolve_virtual_method(this, "format", "(D)Ljava/lang/String;")?;
            vm.invoke_method(class, method, vec![Value::Ref(this), Value::Double(value)])
        },
    ),
    method(
        "format",
        "(DLjava/lang/StringBuffer;Ljava/text/FieldPosition;)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = format_number_required(vm, receiver(args), args[1].as_double())?;
            let buffer = ref_arg(args, 2);
            let base = buffer_length(vm, buffer)?;
            set_number_field_position(vm, ref_arg(args, 3), &text, base)?;
            append_format_text(vm, buffer, &text)
        },
    ),
    method(
        "format",
        "(JLjava/lang/StringBuffer;Ljava/text/FieldPosition;)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let value = args[1].as_long() as f64;
            let (class, method) = vm.resolve_virtual_method(
                this,
                "format",
                "(DLjava/lang/StringBuffer;Ljava/text/FieldPosition;)Ljava/lang/StringBuffer;",
            )?;
            vm.invoke_method(
                class,
                method,
                vec![
                    Value::Ref(this),
                    Value::Double(value),
                    Value::Ref(ref_arg(args, 2)),
                    Value::Ref(ref_arg(args, 3)),
                ],
            )
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
    method(
        "getNumberInstance",
        "()Ljava/text/NumberFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let pattern = default_number_pattern(vm);
            number_format(vm, &pattern)
        },
    ),
    method(
        "getNumberInstance",
        "(Ljava/util/Locale;)Ljava/text/NumberFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let pattern = default_number_pattern(vm);
            number_format(vm, &pattern)
        },
    ),
    method(
        "getAvailableLocales",
        "()[Ljava/util/Locale;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/Locale")?;
            vm.ensure_initialized(class)?;
            let names = [
                "ROOT", "ENGLISH", "US", "UK", "CANADA", "FRANCE", "GERMANY", "ITALY", "JAPAN",
                "KOREA", "CHINA", "PRC", "TAIWAN",
            ];
            let mut locales = Vec::new();
            for name in names {
                if let Some(value) = vm.read_static_ref_if_set(class, name, "Ljava/util/Locale;") {
                    locales.push(value);
                }
            }
            let array = vm.allocate_object_array(class, locales.len())?;
            for (index, locale) in locales.iter().enumerate() {
                vm.array_set_ref(array, index, *locale)?;
            }
            object(array)
        },
    ),
    method(
        "parse",
        "(Ljava/lang/String;)Ljava/lang/Number;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 1));
            number_parse_boxed(vm, &text)
        },
    ),
    method(
        "parse",
        "(Ljava/lang/String;Ljava/text/ParsePosition;)Ljava/lang/Number;",
        ACC_PUBLIC,
        |vm, _, args| {
            let position = ref_arg(args, 2);
            let text = vm.string_or_empty(ref_arg(args, 1));
            let start = read_int_field(vm, position, "index").max(0) as usize;
            let slice: String = text.chars().skip(start).collect();
            match parse_number_text(&slice) {
                Some(_) => {
                    let value = number_parse_boxed(vm, &slice)?;
                    write_int_field(vm, position, "index", text.chars().count() as i32);
                    Ok(value)
                }
                None => {
                    write_int_field(vm, position, "errorIndex", start as i32);
                    object(ObjectRef::NULL)
                }
            }
        },
    ),
    method(
        "parseObject",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 1));
            number_parse_boxed(vm, &text)
        },
    ),
    method(
        "parseObject",
        "(Ljava/lang/String;Ljava/text/ParsePosition;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let position = ref_arg(args, 2);
            let text = vm.string_or_empty(ref_arg(args, 1));
            let start = read_int_field(vm, position, "index").max(0) as usize;
            let slice: String = text.chars().skip(start).collect();
            match parse_number_text(&slice) {
                Some(_) => {
                    let value = number_parse_boxed(vm, &slice)?;
                    write_int_field(vm, position, "index", text.chars().count() as i32);
                    Ok(value)
                }
                None => {
                    write_int_field(vm, position, "errorIndex", start as i32);
                    object(ObjectRef::NULL)
                }
            }
        },
    ),
    method(
        "setRoundingMode",
        "(Ljava/math/RoundingMode;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let mode = ref_arg(args, 1);
            if mode.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let (declaring, field) = vm.find_field(
                vm.class_of(this),
                "roundingMode",
                "Ljava/math/RoundingMode;",
            )?;
            vm.set_instance_ref(this, declaring, field, mode);
            void()
        },
    ),
    method(
        "getRoundingMode",
        "()Ljava/math/RoundingMode;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if let Some(mode) = vm.read_ref_field(
                this,
                vm.class_of(this),
                "roundingMode",
                "Ljava/math/RoundingMode;",
            ) {
                if !mode.is_null() {
                    return object(mode);
                }
            }
            let class = vm.resolve_class("java/math/RoundingMode")?;
            vm.ensure_initialized(class)?;
            let value = vm
                .read_static_ref_if_set(class, "HALF_EVEN", "Ljava/math/RoundingMode;")
                .ok_or_else(|| VmError::internal("RoundingMode constants not initialized"))?;
            object(value)
        },
    ),
    method("setGroupingUsed", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        write_number_flag(vm, receiver(args), "groupingUsed", args[1].as_int());
        void()
    }),
    method("isGroupingUsed", "()Z", ACC_PUBLIC, |vm, _, args| {
        crate::boolean(read_number_flag(vm, receiver(args), "groupingUsed", 1) != 0)
    }),
    method("setParseIntegerOnly", "(Z)V", ACC_PUBLIC, |vm, _, args| {
        write_number_flag(vm, receiver(args), "parseIntegerOnly", args[1].as_int());
        void()
    }),
    method("isParseIntegerOnly", "()Z", ACC_PUBLIC, |vm, _, args| {
        crate::boolean(read_number_flag(vm, receiver(args), "parseIntegerOnly", 0) != 0)
    }),
    method(
        "setMaximumFractionDigits",
        "(I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            write_number_digits(vm, receiver(args), "maxFractionDigits", int_arg(args, 1));
            void()
        },
    ),
    method(
        "getMaximumFractionDigits",
        "()I",
        ACC_PUBLIC,
        |vm, _, args| {
            int(read_number_digits(
                vm,
                receiver(args),
                "maxFractionDigits",
                3,
            ))
        },
    ),
    method(
        "setMinimumFractionDigits",
        "(I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            write_number_digits(vm, receiver(args), "minFractionDigits", int_arg(args, 1));
            void()
        },
    ),
    method(
        "getMinimumFractionDigits",
        "()I",
        ACC_PUBLIC,
        |vm, _, args| {
            int(read_number_digits(
                vm,
                receiver(args),
                "minFractionDigits",
                0,
            ))
        },
    ),
    method(
        "setMaximumIntegerDigits",
        "(I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            write_number_digits(vm, receiver(args), "maxIntegerDigits", int_arg(args, 1));
            void()
        },
    ),
    method(
        "getMaximumIntegerDigits",
        "()I",
        ACC_PUBLIC,
        |vm, _, args| {
            int(read_number_digits(
                vm,
                receiver(args),
                "maxIntegerDigits",
                i32::MAX,
            ))
        },
    ),
    method(
        "setMinimumIntegerDigits",
        "(I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            write_number_digits(vm, receiver(args), "minIntegerDigits", int_arg(args, 1));
            void()
        },
    ),
    method(
        "getMinimumIntegerDigits",
        "()I",
        ACC_PUBLIC,
        |vm, _, args| {
            int(read_number_digits(
                vm,
                receiver(args),
                "minIntegerDigits",
                1,
            ))
        },
    ),
    method(
        "setCurrency",
        "(Ljava/util/Currency;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let currency = ref_arg(args, 1);
            if let Some((declaring, field)) =
                vm.find_field_opt(vm.class_of(this), "currency", "Ljava/util/Currency;")
            {
                vm.set_instance_ref(this, declaring, field, currency);
            }
            void()
        },
    ),
    method(
        "getCurrency",
        "()Ljava/util/Currency;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            if let Some(currency) =
                vm.read_ref_field(this, vm.class_of(this), "currency", "Ljava/util/Currency;")
            {
                if !currency.is_null() {
                    return object(currency);
                }
            }
            object(ObjectRef::NULL)
        },
    ),
    method("setStrict", "(Z)V", ACC_PUBLIC, |_, _, _| void()),
    method("isStrict", "()Z", ACC_PUBLIC, |_, _, _| {
        crate::boolean(false)
    }),
    method("setParseBigDecimal", "(Z)V", ACC_PUBLIC, |_, _, _| void()),
    method("isParseBigDecimal", "()Z", ACC_PUBLIC, |_, _, _| {
        crate::boolean(false)
    }),
    method(
        "format",
        "(Ljava/lang/Object;Ljava/lang/StringBuffer;Ljava/text/FieldPosition;)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = number_argument_text(vm, receiver(args), ref_arg(args, 1))?;
            let buffer = ref_arg(args, 2);
            let base = buffer_length(vm, buffer)?;
            set_number_field_position(vm, ref_arg(args, 3), &text, base)?;
            append_format_text(vm, buffer, &text)
        },
    ),
];

/// Mark a fresh number format's digit/grouping settings as unset (`i32::MIN`), so the
/// getters fall back to the JDK defaults instead of reading the zero-filled fields.
fn init_number_format_state(vm: &mut Vm<'_>, this: ObjectRef) {
    let class = vm.class_of(this);
    for name in [
        "maxFractionDigits",
        "minFractionDigits",
        "maxIntegerDigits",
        "minIntegerDigits",
        "groupingUsed",
        "parseIntegerOnly",
        "multiplier",
    ] {
        if let Some((declaring, field)) = vm.find_field_opt(class, name, "I") {
            vm.set_instance_int(this, declaring, field, i32::MIN);
        }
    }
    if let Some((declaring, field)) =
        vm.find_field_opt(class, "currencySymbol", "Ljava/lang/String;")
    {
        let symbol = currency_symbol(&default_country(vm));
        if let Ok(text) = vm.make_string(symbol) {
            vm.set_instance_ref(this, declaring, field, text);
        }
    }
}

/// Store a digit-count setting; negative values mean "unset", matching the JDK's defaults.
fn write_number_digits(vm: &mut Vm<'_>, this: ObjectRef, name: &str, value: i32) {
    let value = if value < 0 { i32::MIN } else { value };
    if let Some((declaring, field)) = vm.find_field_opt(vm.class_of(this), name, "I") {
        vm.set_instance_int(this, declaring, field, value);
    }
}

/// Read a digit-count setting, falling back to the JDK default when unset.
fn read_number_digits(vm: &Vm<'_>, this: ObjectRef, name: &str, default: i32) -> i32 {
    match vm.read_instance_int_named(this, vm.class_of(this), name, "I") {
        Some(value) if value != i32::MIN => value,
        _ => default,
    }
}

/// Store a boolean setting as 0/1, with `i32::MIN` meaning "unset".
fn write_number_flag(vm: &mut Vm<'_>, this: ObjectRef, name: &str, value: i32) {
    if let Some((declaring, field)) = vm.find_field_opt(vm.class_of(this), name, "I") {
        vm.set_instance_int(this, declaring, field, i32::from(value != 0));
    }
}

/// Read a boolean setting, falling back to the JDK default when unset.
fn read_number_flag(vm: &Vm<'_>, this: ObjectRef, name: &str, default: i32) -> i32 {
    match vm.read_instance_int_named(this, vm.class_of(this), name, "I") {
        Some(value) if value == 0 || value == 1 => value,
        _ => default,
    }
}

/// Store an explicit multiplier; `i32::MIN` means "use the pattern default".
fn write_number_multiplier(vm: &mut Vm<'_>, this: ObjectRef, value: i32) {
    if let Some((declaring, field)) = vm.find_field_opt(vm.class_of(this), "multiplier", "I") {
        vm.set_instance_int(this, declaring, field, value);
    }
}

/// The effective multiplier: the stored value, or the percent/per-mille/number default.
fn read_number_multiplier(vm: &mut Vm<'_>, this: ObjectRef) -> i32 {
    match vm.read_instance_int_named(this, vm.class_of(this), "multiplier", "I") {
        Some(value) if value != i32::MIN => value,
        _ => {
            let pattern = NumberPattern::parse(&pattern_of(vm, this));
            if pattern.percent {
                100
            } else if pattern.per_mille {
                1000
            } else {
                1
            }
        }
    }
}

/// The pattern a number format writes with, after its digit/grouping settings are applied,
/// and the rounding mode it rounds under.
/// The currency symbol a number format replaces the `\u{00a4}` placeholder with.
fn format_currency_symbol(vm: &mut Vm<'_>, format: ObjectRef) -> String {
    let class = vm.class_of(format);
    if let Some(text) = vm.read_string_field(format, class, "currencySymbol") {
        if !text.is_empty() {
            return text;
        }
    }
    currency_symbol(&default_country(vm)).to_string()
}

fn number_format_state(vm: &mut Vm<'_>, format: ObjectRef) -> (NumberPattern, Rounding) {
    let mut pattern = NumberPattern::parse(&pattern_of(vm, format));
    if pattern.prefix.contains('\u{00a4}') || pattern.suffix.contains('\u{00a4}') {
        let symbol = format_currency_symbol(vm, format);
        pattern.prefix = pattern.prefix.replace('\u{00a4}', &symbol);
        pattern.suffix = pattern.suffix.replace('\u{00a4}', &symbol);
    }
    let class = vm.class_of(format);
    let max_fraction = read_number_digits(vm, format, "maxFractionDigits", -1);
    if max_fraction >= 0 {
        pattern.max_fraction = max_fraction as usize;
    }
    let min_fraction = read_number_digits(vm, format, "minFractionDigits", -1);
    if min_fraction >= 0 {
        pattern.min_fraction = min_fraction as usize;
        if pattern.max_fraction < pattern.min_fraction {
            pattern.max_fraction = pattern.min_fraction;
        }
    }
    let min_integer = read_number_digits(vm, format, "minIntegerDigits", -1);
    if min_integer >= 0 {
        pattern.min_integer = min_integer as usize;
    }
    if read_number_flag(vm, format, "groupingUsed", 1) == 0 {
        pattern.grouping = 0;
    }
    if let Some(value) = vm.read_instance_int_named(format, class, "multiplier", "I") {
        if value != i32::MIN {
            pattern.multiplier = Some(value);
        }
    }
    let mode = vm
        .read_ref_field(format, class, "roundingMode", "Ljava/math/RoundingMode;")
        .filter(|mode| !mode.is_null())
        .map(|mode| Rounding::from_ordinal(read_int_field(vm, mode, "ordinal")))
        .unwrap_or(Rounding::HalfEven);
    (pattern, mode)
}

/// Format a number under a format's settings; `None` when `UNNECESSARY` finds a fraction.
fn format_number_stateful(vm: &mut Vm<'_>, format: ObjectRef, value: f64) -> Option<String> {
    let (pattern, mode) = number_format_state(vm, format);
    let compat = crate::lang::system_property(vm, "jdk.compat.DecimalFormat")
        .is_some_and(|text| text.eq_ignore_ascii_case("true"));
    pattern.format_with(value, mode, compat)
}

/// The text of a number argument, in the form the JDK formats.
fn number_argument_text(
    vm: &mut Vm<'_>,
    format: ObjectRef,
    argument: ObjectRef,
) -> Result<String, VmError> {
    if argument.is_null() {
        return Ok(String::from("null"));
    }
    // BigInteger and BigDecimal are formatted from their exact digits, not through a double.
    for name in ["java/math/BigInteger", "java/math/BigDecimal"] {
        if let Ok(class) = vm.resolve_class(name) {
            if vm.is_instance(argument, class) {
                let text = vm
                    .read_string_field(argument, vm.class_of(argument), "value")
                    .unwrap_or_default();
                return format_exact_decimal(vm, format, &text);
            }
        }
    }
    match number_value(vm, argument) {
        Some(value) => format_number_required(vm, format, value),
        None => Ok(number_to_text(vm, argument)),
    }
}

/// Format under the format's settings, throwing `ArithmeticException` when the rounding
/// mode is `UNNECESSARY` and the value needs rounding.
fn format_number_required(
    vm: &mut Vm<'_>,
    format: ObjectRef,
    value: f64,
) -> Result<String, VmError> {
    match format_number_stateful(vm, format, value) {
        Some(text) => Ok(text),
        None => Err(vm.throw_new("java/lang/ArithmeticException", Some("Rounding necessary"))),
    }
}

/// A number format that carries a pattern.
fn number_format(vm: &mut Vm<'_>, pattern: &NumberPattern) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/text/OjvmNumberFormat")?;
    let instance = vm.new_instance(class)?;
    init_number_format_state(vm, instance);
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
    &[
        field("pattern", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("currencySymbol", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("positivePrefix", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("positiveSuffix", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("negativePrefix", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("negativeSuffix", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("multiplier", "I", ACC_PRIVATE, None),
    ],
    &DECIMAL_FORMAT_METHODS,
    None,
);

const DECIMAL_FORMAT_METHODS: [oxjvm_vm::NativeMethodDef; 28] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        init_number_format_state(vm, this);
        let text = vm.make_string("#,##0.###")?;
        let (declaring, field) =
            vm.find_field(vm.class_of(this), "pattern", "Ljava/lang/String;")?;
        vm.set_instance_ref(this, declaring, field, text);
        void()
    }),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            init_number_format_state(vm, this);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "pattern", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/text/DecimalFormatSymbols;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            init_number_format_state(vm, this);
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
        "toLocalizedPattern",
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
        "applyLocalizedPattern",
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
            number_parse_boxed(vm, &text)
        },
    ),
    method(
        "getDecimalFormatSymbols",
        "()Ljava/text/DecimalFormatSymbols;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/text/DecimalFormatSymbols")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "setDecimalFormatSymbols",
        "(Ljava/text/DecimalFormatSymbols;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method("setStrict", "(Z)V", ACC_PUBLIC, |_, _, _| void()),
    method("isStrict", "()Z", ACC_PUBLIC, |_, _, _| {
        crate::boolean(false)
    }),
    method("setParseBigDecimal", "(Z)V", ACC_PUBLIC, |_, _, _| void()),
    method("isParseBigDecimal", "()Z", ACC_PUBLIC, |_, _, _| {
        crate::boolean(false)
    }),
    method(
        "setDecimalSeparatorAlwaysShown",
        "(Z)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "isDecimalSeparatorAlwaysShown",
        "()Z",
        ACC_PUBLIC,
        |_, _, _| crate::boolean(false),
    ),
    method("setGroupingSize", "(I)V", ACC_PUBLIC, |_, _, _| void()),
    method("getGroupingSize", "()I", ACC_PUBLIC, |_, _, _| int(3)),
    method("setMultiplier", "(I)V", ACC_PUBLIC, |vm, _, args| {
        write_number_multiplier(vm, receiver(args), int_arg(args, 1));
        void()
    }),
    method("getMultiplier", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_number_multiplier(vm, receiver(args)))
    }),
    method(
        "getPositivePrefix",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm
                .read_string_field(this, vm.class_of(this), "positivePrefix")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "setPositivePrefix",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| set_decimal_string(vm, receiver(args), "positivePrefix", ref_arg(args, 1)),
    ),
    method(
        "getPositiveSuffix",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm
                .read_string_field(this, vm.class_of(this), "positiveSuffix")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "setPositiveSuffix",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| set_decimal_string(vm, receiver(args), "positiveSuffix", ref_arg(args, 1)),
    ),
    method(
        "getNegativePrefix",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm
                .read_string_field(this, vm.class_of(this), "negativePrefix")
                .unwrap_or_else(|| String::from("-"));
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "setNegativePrefix",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| set_decimal_string(vm, receiver(args), "negativePrefix", ref_arg(args, 1)),
    ),
    method(
        "getNegativeSuffix",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = vm
                .read_string_field(this, vm.class_of(this), "negativeSuffix")
                .unwrap_or_default();
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "setNegativeSuffix",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| set_decimal_string(vm, receiver(args), "negativeSuffix", ref_arg(args, 1)),
    ),
];

/// Store a `DecimalFormat` string property.
fn set_decimal_string(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    value: ObjectRef,
) -> Result<Value, VmError> {
    let (declaring, field) = vm.find_field(vm.class_of(this), name, "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, value);
    void()
}

const DECIMAL_FORMAT_SYMBOLS_METHODS: [oxjvm_vm::NativeMethodDef; 35] = [
    method("<init>", "()V", ACC_PUBLIC, |_, _, _| void()),
    method("<init>", "(Ljava/util/Locale;)V", ACC_PUBLIC, |_, _, _| {
        void()
    }),
    method(
        "getInstance",
        "()Ljava/text/DecimalFormatSymbols;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/text/DecimalFormatSymbols")?;
            object(vm.new_instance(class)?)
        },
    ),
    method(
        "getInstance",
        "(Ljava/util/Locale;)Ljava/text/DecimalFormatSymbols;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/text/DecimalFormatSymbols")?;
            object(vm.new_instance(class)?)
        },
    ),
    method("getDecimalSeparator", "()C", ACC_PUBLIC, |_, _, _| {
        int('.' as i32)
    }),
    method("getGroupingSeparator", "()C", ACC_PUBLIC, |_, _, _| {
        int(',' as i32)
    }),
    method("getMinusSign", "()C", ACC_PUBLIC, |_, _, _| int('-' as i32)),
    method("getZeroDigit", "()C", ACC_PUBLIC, |_, _, _| int('0' as i32)),
    method(
        "getInfinity",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("∞")?),
    ),
    method("getNaN", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, _| {
        object(vm.make_string("NaN")?)
    }),
    method("setDecimalSeparator", "(C)V", ACC_PUBLIC, |_, _, _| void()),
    method("setGroupingSeparator", "(C)V", ACC_PUBLIC, |_, _, _| void()),
    method("setMinusSign", "(C)V", ACC_PUBLIC, |_, _, _| void()),
    method("setZeroDigit", "(C)V", ACC_PUBLIC, |_, _, _| void()),
    method("setPatternSeparator", "(C)V", ACC_PUBLIC, |_, _, _| void()),
    method("getPatternSeparator", "()C", ACC_PUBLIC, |_, _, _| {
        int(';' as i32)
    }),
    method(
        "setMonetaryDecimalSeparator",
        "(C)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "getMonetaryDecimalSeparator",
        "()C",
        ACC_PUBLIC,
        |_, _, _| int('.' as i32),
    ),
    method(
        "setInfinity",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method("setNaN", "(Ljava/lang/String;)V", ACC_PUBLIC, |_, _, _| {
        void()
    }),
    method(
        "getCurrency",
        "()Ljava/util/Currency;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getCurrencySymbol",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("$")?),
    ),
    method(
        "setCurrencySymbol",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "getInternationalCurrencySymbol",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("USD")?),
    ),
    method(
        "setInternationalCurrencySymbol",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method("getPercent", "()C", ACC_PUBLIC, |_, _, _| int('%' as i32)),
    method("setPercent", "(C)V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "getLocale",
        "()Ljava/util/Locale;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/Locale")?;
            vm.ensure_initialized(class)?;
            let locale = vm
                .read_static_ref_if_set(class, "US", "Ljava/util/Locale;")
                .ok_or_else(|| VmError::internal("Locale.US not initialized"))?;
            object(locale)
        },
    ),
    method(
        "getExponentSeparator",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("E")?),
    ),
    method(
        "setExponentSeparator",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method("setDigit", "(C)V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "getMonetaryGroupingSeparator",
        "()C",
        ACC_PUBLIC,
        |_, _, _| int(',' as i32),
    ),
    method(
        "setMonetaryGroupingSeparator",
        "(C)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method("getPerMill", "()C", ACC_PUBLIC, |_, _, _| int('‰' as i32)),
    method("setPerMill", "(C)V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const DECIMAL_FORMAT_SYMBOLS: oxjvm_vm::NativeClass = class(
    "java/text/DecimalFormatSymbols",
    Some("java/lang/Object"),
    &["java/io/Serializable", "java/lang/Cloneable"],
    ACC_PUBLIC,
    &[],
    &DECIMAL_FORMAT_SYMBOLS_METHODS,
    None,
);

/// `java.text.NumberFormat`.
pub(crate) const NUMBER_FORMAT: oxjvm_vm::NativeClass = class(
    "java/text/NumberFormat",
    Some("java/text/Format"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[
        field("maxFractionDigits", "I", ACC_PRIVATE, None),
        field("minFractionDigits", "I", ACC_PRIVATE, None),
        field("maxIntegerDigits", "I", ACC_PRIVATE, None),
        field("minIntegerDigits", "I", ACC_PRIVATE, None),
        field(
            "roundingMode",
            "Ljava/math/RoundingMode;",
            ACC_PRIVATE,
            None,
        ),
        field("groupingUsed", "I", ACC_PRIVATE, None),
        field("parseIntegerOnly", "Z", ACC_PRIVATE, None),
        field("currency", "Ljava/util/Currency;", ACC_PRIVATE, None),
    ],
    &NUMBER_FORMAT_METHODS,
    None,
);

/// The number format the runtime hands to the guest.
pub(crate) const OJVM_NUMBER_FORMAT: oxjvm_vm::NativeClass = class(
    "java/text/OjvmNumberFormat",
    Some("java/text/DecimalFormat"),
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

const DATE_FORMAT_METHODS: [oxjvm_vm::NativeMethodDef; 30] = [
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
        |vm, _, args| {
            require_locale_arg(vm, args)?;
            date_format(vm, "MMM d, yyyy h:mm:ss a")
        },
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
        |vm, _, args| {
            require_locale_arg(vm, args)?;
            date_format(vm, date_pattern("medium"))
        },
    ),
    method(
        "getDateInstance",
        "(I)Ljava/text/DateFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| date_format(vm, date_pattern(date_style_name(int_arg(args, 0)))),
    ),
    method(
        "getDateInstance",
        "(ILjava/util/Locale;)Ljava/text/DateFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            require_locale_arg(vm, args)?;
            date_format(vm, date_pattern(date_style_name(int_arg(args, 0))))
        },
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
        |vm, _, args| {
            require_locale_arg(vm, args)?;
            date_format(vm, time_pattern("medium"))
        },
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
        |vm, _, args| {
            require_locale_arg(vm, args)?;
            date_format(vm, "MMM d, yyyy h:mm:ss a")
        },
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
        "format",
        "(Ljava/util/Date;Ljava/lang/StringBuffer;Ljava/text/FieldPosition;)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let buffer = ref_arg(args, 2);
            let millis = date_millis(vm, ref_arg(args, 1));
            let text = crate::text::format_pattern(&pattern_of(vm, receiver(args)), millis);
            let string = vm.make_string(&text)?;
            let (class, method) = vm.resolve_virtual_method(
                buffer,
                "append",
                "(Ljava/lang/String;)Ljava/lang/StringBuffer;",
            )?;
            vm.invoke_method(class, method, vec![Value::Ref(buffer), Value::Ref(string)])
        },
    ),
    method(
        "format",
        "(Ljava/lang/Object;Ljava/lang/StringBuffer;Ljava/text/FieldPosition;)Ljava/lang/StringBuffer;",
        ACC_PUBLIC,
        |vm, _, args| {
            let buffer = ref_arg(args, 2);
            let text = format_argument(vm, receiver(args), ref_arg(args, 1))?;
            let string = vm.make_string(&text)?;
            let (class, method) = vm.resolve_virtual_method(
                buffer,
                "append",
                "(Ljava/lang/String;)Ljava/lang/StringBuffer;",
            )?;
            vm.invoke_method(class, method, vec![Value::Ref(buffer), Value::Ref(string)])
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
        "getTimeInstance",
        "(I)Ljava/text/DateFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| date_format(vm, time_pattern(date_style_name(int_arg(args, 0)))),
    ),
    method(
        "getTimeInstance",
        "(ILjava/util/Locale;)Ljava/text/DateFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            require_locale_arg(vm, args)?;
            date_format(vm, time_pattern(date_style_name(int_arg(args, 0))))
        },
    ),
    method(
        "getDateTimeInstance",
        "(II)Ljava/text/DateFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let date = date_pattern(date_style_name(int_arg(args, 0)));
            let time = time_pattern(date_style_name(int_arg(args, 1)));
            date_format(vm, &alloc::format!("{date} {time}"))
        },
    ),
    method(
        "getDateTimeInstance",
        "(IILjava/util/Locale;)Ljava/text/DateFormat;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            require_locale_arg(vm, args)?;
            let date = date_pattern(date_style_name(int_arg(args, 0)));
            let time = time_pattern(date_style_name(int_arg(args, 1)));
            date_format(vm, &alloc::format!("{date} {time}"))
        },
    ),
    method(
        "parse",
        "(Ljava/lang/String;)Ljava/util/Date;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 1));
            date_from_millis(vm, parse_date_text(&text))
        },
    ),
    method(
        "parse",
        "(Ljava/lang/String;Ljava/text/ParsePosition;)Ljava/util/Date;",
        ACC_PUBLIC,
        |vm, _, args| {
            let position = ref_arg(args, 2);
            let text = vm.string_or_empty(ref_arg(args, 1));
            if text.is_empty() {
                write_int_field(vm, position, "errorIndex", 0);
                return object(ObjectRef::NULL);
            }
            let value = date_from_millis(vm, parse_date_text(&text))?;
            write_int_field(vm, position, "index", text.chars().count() as i32);
            Ok(value)
        },
    ),
    method(
        "parseObject",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 1));
            date_from_millis(vm, parse_date_text(&text))
        },
    ),
    method(
        "getAvailableLocales",
        "()[Ljava/util/Locale;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/Locale")?;
            vm.ensure_initialized(class)?;
            let names = [
                "ROOT", "ENGLISH", "US", "UK", "CANADA", "FRANCE", "GERMANY", "ITALY", "JAPAN",
                "KOREA", "CHINA", "PRC", "TAIWAN",
            ];
            let mut locales = Vec::new();
            for name in names {
                if let Some(value) = vm.read_static_ref_if_set(class, name, "Ljava/util/Locale;") {
                    locales.push(value);
                }
            }
            let array = vm.allocate_object_array(class, locales.len())?;
            for (index, locale) in locales.iter().enumerate() {
                vm.array_set_ref(array, index, *locale)?;
            }
            object(array)
        },
    ),
    method(
        "setCalendar",
        "(Ljava/util/Calendar;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "getCalendar",
        "()Ljava/util/Calendar;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/GregorianCalendar")?;
            let instance = vm.new_instance(class)?;
            let (declaring, ctor) = vm
                .find_method(class, "<init>", "()V")
                .ok_or_else(|| VmError::internal("GregorianCalendar lacks its constructor"))?;
            vm.invoke_method(declaring, ctor, vec![Value::Ref(instance)])?;
            object(instance)
        },
    ),
    method("setLenient", "(Z)V", ACC_PUBLIC, |_, _, _| void()),
    method("isLenient", "()Z", ACC_PUBLIC, |_, _, _| {
        crate::boolean(true)
    }),
    method(
        "setTimeZone",
        "(Ljava/util/TimeZone;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "getTimeZone",
        "()Ljava/util/TimeZone;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/TimeZone")?;
            let (declaring, method) = vm
                .find_method(class, "getDefault", "()Ljava/util/TimeZone;")
                .ok_or_else(|| VmError::internal("TimeZone.getDefault missing"))?;
            vm.invoke_method(declaring, method, Vec::new())
        },
    ),
];

/// A `java.util.Date` holding the given epoch milliseconds.
fn date_from_millis(vm: &mut Vm<'_>, millis: i64) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/Date")?;
    let instance = vm.new_instance(class)?;
    crate::lang::write_long_field(vm, instance, "time", millis);
    object(instance)
}

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
/// The JDK rejects a null Locale passed to the DateFormat factory methods;
/// the Locale is always the last argument of those static overloads.
fn require_locale_arg(vm: &mut Vm<'_>, args: &[Value]) -> Result<(), VmError> {
    if ref_arg(args, args.len() - 1).is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    Ok(())
}

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
    Some("java/text/SimpleDateFormat"),
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

    /// The limit whose range text matches `text`, for `ChoiceFormat.parse`.
    fn limit_for_text(&self, text: &str) -> Option<f64> {
        for (index, candidate) in self.texts.iter().enumerate() {
            if candidate == text {
                return self.limits.get(index).copied();
            }
        }
        None
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

const CHOICE_FORMAT_METHODS: [oxjvm_vm::NativeMethodDef; 15] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let pattern = vm.string_or_empty(ref_arg(args, 1));
            let choice = Choice::parse(&pattern);
            choice_set_pattern(vm, this, &choice.pattern())?;
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
            choice_set_pattern(vm, this, &pattern)?;
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
        choice_set_pattern(vm, this, &pattern)?;
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
    method("getLimits", "()[D", ACC_PUBLIC, |vm, _, args| {
        let choice = Choice::parse(&pattern_of(vm, receiver(args)));
        let array = vm.allocate_array_of(ArrayComponent::Double, choice.limits.len())?;
        for (index, value) in choice.limits.iter().enumerate() {
            vm.array_set(array, index as i32, Value::Double(*value), false)?;
        }
        object(array)
    }),
    method(
        "getFormats",
        "()[Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let choice = Choice::parse(&pattern_of(vm, receiver(args)));
            let class = vm.resolve_class("java/lang/Object")?;
            let array = vm.allocate_object_array(class, choice.texts.len())?;
            for (index, text) in choice.texts.iter().enumerate() {
                let string = vm.make_string(text)?;
                vm.array_set_ref(array, index, string)?;
            }
            object(array)
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
    method(
        "parse",
        "(Ljava/lang/String;)Ljava/lang/Number;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 1));
            let choice = Choice::parse(&pattern_of(vm, receiver(args)));
            match choice.limit_for_text(&text) {
                Some(value) => crate::boxed::boxed_double(vm, value),
                None => Err(vm.throw_new("java/text/ParseException", Some("Unparsable choice"))),
            }
        },
    ),
    method(
        "parse",
        "(Ljava/lang/String;Ljava/text/ParsePosition;)Ljava/lang/Number;",
        ACC_PUBLIC,
        |vm, _, args| {
            let position = ref_arg(args, 2);
            let text = vm.string_or_empty(ref_arg(args, 1));
            let start = read_int_field(vm, position, "index").max(0) as usize;
            let slice: String = text.chars().skip(start).collect();
            let choice = Choice::parse(&pattern_of(vm, receiver(args)));
            match choice.limit_for_text(&slice) {
                Some(value) => {
                    write_int_field(vm, position, "index", text.chars().count() as i32);
                    crate::boxed::boxed_double(vm, value)
                }
                None => {
                    write_int_field(vm, position, "errorIndex", start as i32);
                    object(ObjectRef::NULL)
                }
            }
        },
    ),
    method(
        "applyPattern",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let pattern = vm.string_or_empty(ref_arg(args, 1));
            let choice = Choice::parse(&pattern);
            choice_set_pattern(vm, this, &choice.pattern())?;
            void()
        },
    ),
];

/// The value of a number argument.
fn number_value(vm: &mut Vm<'_>, argument: ObjectRef) -> Option<f64> {
    if argument.is_null() {
        return None;
    }
    if !is_number(vm, argument) {
        return None;
    }
    // BigDecimal and BigInteger keep their value as a decimal string, not a boxed primitive.
    for name in ["java/math/BigDecimal", "java/math/BigInteger"] {
        if let Ok(class) = vm.resolve_class(name) {
            if vm.is_instance(argument, class) {
                let text = vm
                    .read_string_field(argument, vm.class_of(argument), "value")
                    .unwrap_or_default();
                return text.parse::<f64>().ok();
            }
        }
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
// CompactNumberFormat
// -------------------------------------------------------------------------------------------

/// The default SHORT compact patterns: three magnitudes each for K, M, B and T.
const DEFAULT_COMPACT_PATTERNS: [&str; 15] = [
    "0", "0", "0", "0K", "00K", "000K", "0M", "00M", "000M", "0B", "00B", "000B", "0T", "00T",
    "000T",
];

/// Build a `CompactNumberFormat` carrying the decimal pattern, compact patterns and style.
fn compact_number_format(
    vm: &mut Vm<'_>,
    decimal_pattern: &str,
    patterns: ObjectRef,
    style: &str,
) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/text/CompactNumberFormat")?;
    let instance = vm.new_instance(class)?;
    init_number_format_state(vm, instance);
    let text = vm.make_string(decimal_pattern)?;
    let (declaring, field) = vm.find_field(class, "pattern", "Ljava/lang/String;")?;
    vm.set_instance_ref(instance, declaring, field, text);
    let patterns = if patterns.is_null() {
        let string_class = vm.resolve_class("java/lang/String")?;
        let array = vm.allocate_object_array(string_class, DEFAULT_COMPACT_PATTERNS.len())?;
        for (index, pattern) in DEFAULT_COMPACT_PATTERNS.iter().enumerate() {
            let text = vm.make_string(pattern)?;
            vm.array_set_ref(array, index, text)?;
        }
        array
    } else {
        patterns
    };
    let (declaring, field) = vm.find_field(class, "compactPatterns", "[Ljava/lang/String;")?;
    vm.set_instance_ref(instance, declaring, field, patterns);
    let style_text = vm.make_string(style)?;
    let (declaring, field) = vm.find_field(class, "compactStyle", "Ljava/lang/String;")?;
    vm.set_instance_ref(instance, declaring, field, style_text);
    let kind = vm.make_string("number")?;
    let (declaring, field) = vm.find_field(class, "kind", "Ljava/lang/String;")?;
    vm.set_instance_ref(instance, declaring, field, kind);
    object(instance)
}

/// The compact patterns a format carries, as text.
fn compact_patterns_of(vm: &mut Vm<'_>, format: ObjectRef) -> Vec<String> {
    let array = vm
        .read_ref_field(
            format,
            vm.class_of(format),
            "compactPatterns",
            "[Ljava/lang/String;",
        )
        .unwrap_or(ObjectRef::NULL);
    object_array_elements(vm, array)
        .into_iter()
        .map(|element| vm.string_or_empty(element))
        .collect()
}

/// Format a value in compact form: pick the magnitude's pattern, scale the value, and write
/// the pattern's digit count with its suffix.
fn compact_format_value(vm: &mut Vm<'_>, format: ObjectRef, value: f64) -> String {
    let decimal = pattern_of(vm, format);
    let style = vm
        .read_string_field(format, vm.class_of(format), "compactStyle")
        .unwrap_or_default();
    let compact = if style == "LONG" {
        Compact::Long
    } else {
        Compact::Short
    };
    let patterns = compact_patterns_of(vm, format);
    let magnitude = value.abs();
    if patterns.is_empty() || magnitude < 1000.0 {
        return NumberPattern::parse(&decimal).format(value);
    }
    let group = if magnitude >= 1e12 {
        4
    } else if magnitude >= 1e9 {
        3
    } else if magnitude >= 1e6 {
        2
    } else {
        1
    };
    let divisor = power_of_ten(group * 3);
    let scaled = magnitude / divisor;
    // Within a magnitude the pattern index follows the number of integer digits (1..3).
    let digits = if scaled >= 100.0 {
        3
    } else if scaled >= 10.0 {
        2
    } else {
        1
    };
    let index = (group * 3 + digits - 1).min(patterns.len() - 1);
    let pattern = &patterns[index];
    let digit_count = pattern.chars().filter(|ch| *ch == '0').count().max(1);
    let suffix: String = pattern
        .chars()
        .skip_while(|ch| ch.is_ascii_digit() || *ch == '#' || *ch == '.' || *ch == ',')
        .collect();
    let factor = power_of_ten(digit_count.saturating_sub(1));
    let rounded = crate::math::floor(scaled * factor + 0.5) / factor;
    let mut text = format_integer(rounded);
    if text.len() > digit_count {
        text.truncate(digit_count);
    }
    let mut out = String::new();
    if value < 0.0 {
        out.push('-');
    }
    out.push_str(&text);
    if compact == Compact::Long {
        out.push(' ');
        out.push_str(match group {
            1 => "thousand",
            2 => "million",
            3 => "billion",
            _ => "trillion",
        });
    } else {
        out.push_str(&suffix);
    }
    out
}

/// Parse compact text: a trailing K/M/B/T (or the long word) scales the number back up.
fn compact_parse_value(vm: &mut Vm<'_>, text: &str) -> Result<Value, VmError> {
    let trimmed = text.trim();
    let (number, multiplier) = if let Some(rest) = trimmed.strip_suffix('K') {
        (rest, 1e3)
    } else if let Some(rest) = trimmed.strip_suffix('M') {
        (rest, 1e6)
    } else if let Some(rest) = trimmed.strip_suffix('B') {
        (rest, 1e9)
    } else if let Some(rest) = trimmed.strip_suffix('T') {
        (rest, 1e12)
    } else if let Some(rest) = trimmed.strip_suffix("thousand") {
        (rest.trim_end(), 1e3)
    } else if let Some(rest) = trimmed.strip_suffix("million") {
        (rest.trim_end(), 1e6)
    } else if let Some(rest) = trimmed.strip_suffix("billion") {
        (rest.trim_end(), 1e9)
    } else if let Some(rest) = trimmed.strip_suffix("trillion") {
        (rest.trim_end(), 1e12)
    } else {
        (trimmed, 1.0)
    };
    let value = parse_number_text(number)
        .ok_or_else(|| vm.throw_new("java/text/ParseException", Some("Unparsable number")))?;
    let scaled = value * multiplier;
    if scaled >= i64::MIN as f64 && scaled <= i64::MAX as f64 && scaled == (scaled as i64) as f64 {
        crate::boxed::boxed_long(vm, scaled as i64)
    } else {
        crate::boxed::boxed_double(vm, scaled)
    }
}

const COMPACT_NUMBER_FORMAT_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/text/DecimalFormatSymbols;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let pattern = vm.string_or_empty(ref_arg(args, 1));
            let value = compact_number_format(vm, &pattern, ObjectRef::NULL, "SHORT")?;
            copy_compact_fields(vm, this, value.as_ref());
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/text/DecimalFormatSymbols;[Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let pattern = vm.string_or_empty(ref_arg(args, 1));
            let value = compact_number_format(vm, &pattern, ref_arg(args, 3), "SHORT")?;
            copy_compact_fields(vm, this, value.as_ref());
            void()
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/text/DecimalFormatSymbols;[Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let pattern = vm.string_or_empty(ref_arg(args, 1));
            let value = compact_number_format(vm, &pattern, ref_arg(args, 3), "SHORT")?;
            copy_compact_fields(vm, this, value.as_ref());
            void()
        },
    ),
    method(
        "format",
        "(D)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = compact_format_value(vm, receiver(args), args[1].as_double());
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "format",
        "(Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let value = number_value(vm, ref_arg(args, 1)).unwrap_or(0.0);
            let text = compact_format_value(vm, receiver(args), value);
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "parse",
        "(Ljava/lang/String;)Ljava/lang/Number;",
        ACC_PUBLIC,
        |vm, _, args| {
            let text = vm.string_or_empty(ref_arg(args, 1));
            compact_parse_value(vm, &text)
        },
    ),
    method(
        "parse",
        "(Ljava/lang/String;Ljava/text/ParsePosition;)Ljava/lang/Number;",
        ACC_PUBLIC,
        |vm, _, args| {
            let position = ref_arg(args, 2);
            let text = vm.string_or_empty(ref_arg(args, 1));
            let start = read_int_field(vm, position, "index").max(0) as usize;
            let slice: String = text.chars().skip(start).collect();
            match compact_parse_value(vm, &slice) {
                Ok(value) => {
                    write_int_field(vm, position, "index", text.chars().count() as i32);
                    Ok(value)
                }
                Err(_) => {
                    write_int_field(vm, position, "errorIndex", start as i32);
                    object(ObjectRef::NULL)
                }
            }
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
            let patterns = compact_patterns_of(vm, receiver(args));
            let text = alloc::format!(
                "CompactNumberFormat [decimal pattern: \"{pattern}\", compact patterns: \"[{}]\"]",
                patterns.join(", ")
            );
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
            let this = receiver(args);
            let same = vm.class_of(this) == vm.class_of(other)
                && pattern_of(vm, this) == pattern_of(vm, other)
                && compact_patterns_of(vm, this) == compact_patterns_of(vm, other);
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
    method(
        "getDecimalFormatSymbols",
        "()Ljava/text/DecimalFormatSymbols;",
        ACC_PUBLIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/text/DecimalFormatSymbols")?;
            object(vm.new_instance(class)?)
        },
    ),
    method("setGroupingSize", "(I)V", ACC_PUBLIC, |_, _, _| void()),
    method("getGroupingSize", "()I", ACC_PUBLIC, |_, _, _| int(0)),
];

/// Copy the compact format's fields onto a freshly constructed instance.
fn copy_compact_fields(vm: &mut Vm<'_>, target: ObjectRef, source: ObjectRef) {
    let class = vm.class_of(source);
    for (name, descriptor) in [
        ("pattern", "Ljava/lang/String;"),
        ("compactPatterns", "[Ljava/lang/String;"),
        ("compactStyle", "Ljava/lang/String;"),
        ("kind", "Ljava/lang/String;"),
    ] {
        if let Some(value) = vm.read_ref_field(source, class, name, descriptor) {
            if let Some((declaring, field)) =
                vm.find_field_opt(vm.class_of(target), name, descriptor)
            {
                vm.set_instance_ref(target, declaring, field, value);
            }
        }
    }
}

/// `java.text.CompactNumberFormat`: numbers written as multiples of a thousand or more.
pub(crate) const COMPACT_NUMBER_FORMAT: oxjvm_vm::NativeClass = class(
    "java/text/CompactNumberFormat",
    Some("java/text/NumberFormat"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("pattern", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("compactPatterns", "[Ljava/lang/String;", ACC_PRIVATE, None),
        field("compactStyle", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("kind", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &COMPACT_NUMBER_FORMAT_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// DateFormatSymbols
// -------------------------------------------------------------------------------------------

/// A fresh `String[]` holding the given texts.
fn make_string_array(vm: &mut Vm<'_>, texts: &[&str]) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/lang/String")?;
    let array = vm.allocate_object_array(class, texts.len())?;
    for (index, text) in texts.iter().enumerate() {
        let value = vm.make_string(text)?;
        vm.array_set_ref(array, index, value)?;
    }
    Ok(array)
}

/// The texts a `String[]` holds.
fn read_string_array(vm: &mut Vm<'_>, array: ObjectRef) -> Vec<String> {
    object_array_elements(vm, array)
        .into_iter()
        .map(|element| vm.string_or_empty(element))
        .collect()
}

/// A copy of a `String[]`, the way the JDK's getters and setters hand out arrays.
fn clone_string_array(vm: &mut Vm<'_>, array: ObjectRef) -> Result<ObjectRef, VmError> {
    let texts = read_string_array(vm, array);
    let refs: Vec<&str> = texts.iter().map(|text| text.as_str()).collect();
    make_string_array(vm, &refs)
}

/// Fill a fresh `DateFormatSymbols` with the English defaults.
fn date_format_symbols_init(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let months = make_string_array(
        vm,
        &[
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
        ],
    )?;
    let short_months = make_string_array(
        vm,
        &[
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ],
    )?;
    let weekdays = make_string_array(
        vm,
        &[
            "",
            "Sunday",
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
        ],
    )?;
    let short_weekdays =
        make_string_array(vm, &["", "Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"])?;
    let eras = make_string_array(vm, &["BC", "AD"])?;
    let ampms = make_string_array(vm, &["AM", "PM"])?;
    for (name, array) in [
        ("months", months),
        ("shortMonths", short_months),
        ("weekdays", weekdays),
        ("shortWeekdays", short_weekdays),
        ("eras", eras),
        ("ampms", ampms),
    ] {
        let (declaring, field) = vm.find_field(class, name, "[Ljava/lang/String;")?;
        vm.set_instance_ref(this, declaring, field, array);
    }
    let gmt = make_string_array(
        vm,
        &[
            "GMT",
            "Greenwich Mean Time",
            "GMT",
            "Greenwich Mean Time",
            "GMT",
        ],
    )?;
    let zone_class = vm.resolve_class("[Ljava/lang/String;")?;
    let zones = vm.allocate_object_array(zone_class, 1)?;
    vm.array_set_ref(zones, 0, gmt)?;
    let (declaring, field) = vm.find_field(class, "zoneStrings", "[[Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, zones);
    let chars = vm.make_string("GyMdkHmsSEDFwWahKzZ")?;
    let (declaring, field) = vm.find_field(class, "localPatternChars", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, chars);
    Ok(())
}

/// A getter over one of the symbol `String[]` fields; the JDK hands out a copy.
macro_rules! symbols_getter {
    ($getter:literal, $field:literal) => {
        method(
            $getter,
            "()[Ljava/lang/String;",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                let array = vm
                    .read_ref_field(this, vm.class_of(this), $field, "[Ljava/lang/String;")
                    .unwrap_or(ObjectRef::NULL);
                object(clone_string_array(vm, array)?)
            },
        )
    };
}

/// A setter over one of the symbol `String[]` fields; the JDK stores a copy.
macro_rules! symbols_setter {
    ($setter:literal, $field:literal) => {
        method(
            $setter,
            "([Ljava/lang/String;)V",
            ACC_PUBLIC,
            |vm, _, args| {
                let this = receiver(args);
                if ref_arg(args, 1).is_null() {
                    return Err(vm.throw_new("java/lang/NullPointerException", None));
                }
                let copy = clone_string_array(vm, ref_arg(args, 1))?;
                let (declaring, field) =
                    vm.find_field(vm.class_of(this), $field, "[Ljava/lang/String;")?;
                vm.set_instance_ref(this, declaring, field, copy);
                void()
            },
        )
    };
}

const DATE_FORMAT_SYMBOLS_METHODS: [oxjvm_vm::NativeMethodDef; 25] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        date_format_symbols_init(vm, receiver(args))?;
        void()
    }),
    method(
        "<init>",
        "(Ljava/util/Locale;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            if ref_arg(args, 1).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            date_format_symbols_init(vm, receiver(args))?;
            void()
        },
    ),
    method(
        "getInstance",
        "()Ljava/text/DateFormatSymbols;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/text/DateFormatSymbols")?;
            let instance = vm.new_instance(class)?;
            date_format_symbols_init(vm, instance)?;
            object(instance)
        },
    ),
    method(
        "getInstance",
        "(Ljava/util/Locale;)Ljava/text/DateFormatSymbols;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            if ref_arg(args, 0).is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let class = vm.resolve_class("java/text/DateFormatSymbols")?;
            let instance = vm.new_instance(class)?;
            date_format_symbols_init(vm, instance)?;
            object(instance)
        },
    ),
    method(
        "getAvailableLocales",
        "()[Ljava/util/Locale;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/Locale")?;
            vm.ensure_initialized(class)?;
            let names = [
                "ROOT", "ENGLISH", "US", "UK", "CANADA", "FRANCE", "GERMANY", "ITALY", "JAPAN",
                "KOREA", "CHINA", "PRC", "TAIWAN",
            ];
            let mut locales = Vec::new();
            for name in names {
                if let Some(value) = vm.read_static_ref_if_set(class, name, "Ljava/util/Locale;") {
                    locales.push(value);
                }
            }
            let array = vm.allocate_object_array(class, locales.len())?;
            for (index, locale) in locales.iter().enumerate() {
                vm.array_set_ref(array, index, *locale)?;
            }
            object(array)
        },
    ),
    symbols_getter!("getMonths", "months"),
    symbols_setter!("setMonths", "months"),
    symbols_getter!("getShortMonths", "shortMonths"),
    symbols_setter!("setShortMonths", "shortMonths"),
    symbols_getter!("getWeekdays", "weekdays"),
    symbols_setter!("setWeekdays", "weekdays"),
    symbols_getter!("getShortWeekdays", "shortWeekdays"),
    symbols_setter!("setShortWeekdays", "shortWeekdays"),
    symbols_getter!("getEras", "eras"),
    symbols_setter!("setEras", "eras"),
    symbols_getter!("getAmPmStrings", "ampms"),
    symbols_setter!("setAmPmStrings", "ampms"),
    method(
        "getZoneStrings",
        "()[[Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let zones = vm
                .read_ref_field(
                    this,
                    vm.class_of(this),
                    "zoneStrings",
                    "[[Ljava/lang/String;",
                )
                .unwrap_or(ObjectRef::NULL);
            let rows = object_array_elements(vm, zones);
            let string_class = vm.resolve_class("[Ljava/lang/String;")?;
            let copy = vm.allocate_object_array(string_class, rows.len())?;
            for (index, row) in rows.iter().enumerate() {
                let cloned = clone_string_array(vm, *row)?;
                vm.array_set_ref(copy, index, cloned)?;
            }
            object(copy)
        },
    ),
    method(
        "setZoneStrings",
        "([[Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let zones = ref_arg(args, 1);
            if zones.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let rows = object_array_elements(vm, zones);
            if rows.is_empty() {
                return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
            }
            for row in &rows {
                if row.is_null() || vm.array_length(*row).unwrap_or(0) != 5 {
                    return Err(vm.throw_new("java/lang/IllegalArgumentException", None));
                }
            }
            let string_class = vm.resolve_class("[Ljava/lang/String;")?;
            let copy = vm.allocate_object_array(string_class, rows.len())?;
            for (index, row) in rows.iter().enumerate() {
                let cloned = clone_string_array(vm, *row)?;
                vm.array_set_ref(copy, index, cloned)?;
            }
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "zoneStrings", "[[Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, copy);
            void()
        },
    ),
    method(
        "getLocalPatternChars",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let chars = vm
                .read_string_field(
                    receiver(args),
                    vm.class_of(receiver(args)),
                    "localPatternChars",
                )
                .unwrap_or_default();
            object(vm.make_string(&chars)?)
        },
    ),
    method(
        "setLocalPatternChars",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (declaring, field) =
                vm.find_field(vm.class_of(this), "localPatternChars", "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, ref_arg(args, 1));
            void()
        },
    ),
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let copy = vm.new_instance(class)?;
            for name in [
                "months",
                "shortMonths",
                "weekdays",
                "shortWeekdays",
                "eras",
                "ampms",
            ] {
                let array = vm
                    .read_ref_field(this, class, name, "[Ljava/lang/String;")
                    .unwrap_or(ObjectRef::NULL);
                let cloned = clone_string_array(vm, array)?;
                let (declaring, field) = vm.find_field(class, name, "[Ljava/lang/String;")?;
                vm.set_instance_ref(copy, declaring, field, cloned);
            }
            let zones = vm
                .read_ref_field(this, class, "zoneStrings", "[[Ljava/lang/String;")
                .unwrap_or(ObjectRef::NULL);
            let rows = object_array_elements(vm, zones);
            let string_class = vm.resolve_class("[Ljava/lang/String;")?;
            let zones_copy = vm.allocate_object_array(string_class, rows.len())?;
            for (index, row) in rows.iter().enumerate() {
                let cloned = clone_string_array(vm, *row)?;
                vm.array_set_ref(zones_copy, index, cloned)?;
            }
            let (declaring, field) = vm.find_field(class, "zoneStrings", "[[Ljava/lang/String;")?;
            vm.set_instance_ref(copy, declaring, field, zones_copy);
            let chars = vm
                .read_ref_field(this, class, "localPatternChars", "Ljava/lang/String;")
                .unwrap_or(ObjectRef::NULL);
            let (declaring, field) =
                vm.find_field(class, "localPatternChars", "Ljava/lang/String;")?;
            vm.set_instance_ref(copy, declaring, field, chars);
            object(copy)
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let array = vm
            .read_ref_field(this, vm.class_of(this), "months", "[Ljava/lang/String;")
            .unwrap_or(ObjectRef::NULL);
        let mut hash = 0i32;
        for text in read_string_array(vm, array) {
            for ch in text.chars() {
                hash = hash.wrapping_mul(31).wrapping_add(ch as i32);
            }
        }
        int(hash)
    }),
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
            if vm.class_of(this) != vm.class_of(other) {
                return crate::boolean(false);
            }
            let class = vm.class_of(this);
            for name in [
                "months",
                "shortMonths",
                "weekdays",
                "shortWeekdays",
                "eras",
                "ampms",
            ] {
                let left = vm
                    .read_ref_field(this, class, name, "[Ljava/lang/String;")
                    .unwrap_or(ObjectRef::NULL);
                let right = vm
                    .read_ref_field(other, class, name, "[Ljava/lang/String;")
                    .unwrap_or(ObjectRef::NULL);
                if read_string_array(vm, left) != read_string_array(vm, right) {
                    return crate::boolean(false);
                }
            }
            crate::boolean(true)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let array = vm
                .read_ref_field(this, vm.class_of(this), "months", "[Ljava/lang/String;")
                .unwrap_or(ObjectRef::NULL);
            let texts = read_string_array(vm, array);
            let text = alloc::format!("DateFormatSymbols [months=[{}]]", texts.join(","));
            object(vm.make_string(&text)?)
        },
    ),
];

/// `java.text.DateFormatSymbols`: the month, weekday, era and zone names a date format uses.
pub(crate) const DATE_FORMAT_SYMBOLS: oxjvm_vm::NativeClass = class(
    "java/text/DateFormatSymbols",
    Some("java/lang/Object"),
    &["java/io/Serializable", "java/lang/Cloneable"],
    ACC_PUBLIC,
    &[
        field("months", "[Ljava/lang/String;", ACC_PRIVATE, None),
        field("shortMonths", "[Ljava/lang/String;", ACC_PRIVATE, None),
        field("weekdays", "[Ljava/lang/String;", ACC_PRIVATE, None),
        field("shortWeekdays", "[Ljava/lang/String;", ACC_PRIVATE, None),
        field("eras", "[Ljava/lang/String;", ACC_PRIVATE, None),
        field("ampms", "[Ljava/lang/String;", ACC_PRIVATE, None),
        field("zoneStrings", "[[Ljava/lang/String;", ACC_PRIVATE, None),
        field("localPatternChars", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &DATE_FORMAT_SYMBOLS_METHODS,
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
/// Store a choice pattern and mark the format's kind so a message format can use it.
fn choice_set_pattern(vm: &mut Vm<'_>, this: ObjectRef, pattern: &str) -> Result<(), VmError> {
    let text = vm.make_string(pattern)?;
    let (declaring, field) = vm.find_field(vm.class_of(this), "pattern", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, text);
    let kind = vm.make_string("choice")?;
    let (declaring, field) = vm.find_field(vm.class_of(this), "kind", "Ljava/lang/String;")?;
    vm.set_instance_ref(this, declaring, field, kind);
    Ok(())
}

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

/// `NumberFormat.parse`: box the parsed value as a `Long` when it is integral (no fractional
/// part written) and fits, otherwise as a `Double`, matching the JDK's return-type contract.
fn number_parse_boxed(vm: &mut Vm<'_>, text: &str) -> Result<Value, VmError> {
    let mut digits = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' {
            digits.push(ch);
        }
    }
    let value = parse_number_text(text)
        .ok_or_else(|| vm.throw_new("java/text/ParseException", Some("Unparsable number")))?;
    // The JDK returns a Long when the parsed value is a whole number that fits in a long,
    // even when a zero fraction such as ".00" is written; negative zero stays a Double.
    // The range test uses the exact integer text, since an f64 cannot tell Long.MIN_VALUE-1
    // from Long.MIN_VALUE.
    let (integer, fraction) = match digits.split_once('.') {
        Some((integer, fraction)) => (integer, fraction),
        None => (digits.as_str(), ""),
    };
    if fraction.bytes().all(|byte| byte == b'0') {
        if let Ok(exact) = integer.parse::<i128>() {
            let negative_zero = exact == 0 && integer.starts_with('-');
            if !negative_zero && (i64::MIN as i128..=i64::MAX as i128).contains(&exact) {
                return crate::boxed::boxed_long(vm, exact as i64);
            }
        }
    }
    crate::boxed::boxed_double(vm, value)
}

/// The milliseconds a date text holds, read with the patterns the runtime writes.
pub(crate) fn parse_date_text(text: &str) -> i64 {
    let cleaned: String = text
        .chars()
        .filter(|ch| {
            ch.is_ascii_digit()
                || *ch == '/'
                || *ch == ':'
                || *ch == ','
                || *ch == '.'
                || *ch == '-'
                || *ch == ' '
        })
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
pub(crate) fn millis_from_civil(
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
) -> i64 {
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
            let index = int_arg(args, 1);
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
            let index = int_arg(args, 1);
            set_message_format(vm, receiver(args), index, ref_arg(args, 2));
            void()
        },
    ),
    method(
        "setFormatByArgument",
        "(ILjava/text/Format;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let index = int_arg(args, 1);
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
            let index = int_arg(args, 1);
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
pub(crate) fn default_locale(vm: &mut Vm<'_>) -> ObjectRef {
    let Some(class) = vm.resolve_class_lenient_pub("java/util/Locale") else {
        return ObjectRef::NULL;
    };
    // The default locale is set when the class is initialized.
    let _ = vm.ensure_initialized(class);
    // Formatting follows the FORMAT category default locale, not the user default.
    if let Some(category_class) = vm.resolve_class_lenient_pub("java/util/Locale$Category") {
        let _ = vm.ensure_initialized(category_class);
        if let Some(format) =
            vm.read_static_ref_if_set(category_class, "FORMAT", "Ljava/util/Locale$Category;")
        {
            if let Some((declaring, method)) = vm.find_method(
                class,
                "getDefault",
                "(Ljava/util/Locale$Category;)Ljava/util/Locale;",
            ) {
                if let Ok(value) = vm.invoke_method(declaring, method, vec![Value::Ref(format)]) {
                    let locale = value.as_ref();
                    if !locale.is_null() {
                        return locale;
                    }
                }
            }
        }
    }
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
            write_int_field(vm, this, "errorOffset", int_arg(args, 2));
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
