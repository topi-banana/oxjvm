//! Java's primitive-to-string and string-to-primitive conversions.
//!
//! These mirror `Integer.toString`, `Long.toString`, `Float.toString`, `Double.toString`, and the
//! `parseX` families. Rust's shortest-round-trip float formatting matches Java's shortest-decimal
//! contract; the only Java-specific work is the plain/scientific notation split and the exponent
//! spelling (`E`, always at least one fractional digit).

use alloc::string::{String, ToString};

/// Format an `int` exactly like `Integer.toString`.
#[must_use]
pub fn int_to_string(value: i32) -> String {
    value.to_string()
}

/// Format a `long` exactly like `Long.toString`.
#[must_use]
pub fn long_to_string(value: i64) -> String {
    value.to_string()
}

/// Format a `float` like `Float.toString`.
#[must_use]
pub fn float_to_string(value: f32) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value == f32::INFINITY {
        return "Infinity".into();
    }
    if value == f32::NEG_INFINITY {
        return "-Infinity".into();
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0".into()
        } else {
            "0.0".into()
        };
    }
    let magnitude = value.abs();
    if (1e-3..1e7).contains(&f64::from(magnitude)) {
        let mut text = alloc::format!("{value}");
        if !text.contains('.') && !text.contains('e') && !text.contains('E') {
            text.push_str(".0");
        }
        text
    } else {
        scientific(&alloc::format!("{value:e}"))
    }
}

/// Format a `double` like `Double.toString`.
#[must_use]
pub fn double_to_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value == f64::INFINITY {
        return "Infinity".into();
    }
    if value == f64::NEG_INFINITY {
        return "-Infinity".into();
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0".into()
        } else {
            "0.0".into()
        };
    }
    let magnitude = value.abs();
    if (1e-3..1e7).contains(&magnitude) {
        let mut text = alloc::format!("{value}");
        if !text.contains('.') && !text.contains('e') && !text.contains('E') {
            text.push_str(".0");
        }
        text
    } else {
        scientific(&alloc::format!("{value:e}"))
    }
}

fn scientific(rust_exponent: &str) -> String {
    let Some((mantissa, exponent)) = rust_exponent.split_once('e') else {
        return rust_exponent.into();
    };
    let mut mantissa = mantissa.to_string();
    if !mantissa.contains('.') {
        mantissa.push_str(".0");
    }
    alloc::format!("{mantissa}E{exponent}")
}

/// Parse an `int` exactly like `Integer.parseInt`: optional sign, base 10, overflow is rejected.
///
/// # Errors
///
/// Returns `Err(())` for any malformed or out-of-range input.
pub fn parse_int(text: &str) -> Result<i32, ()> {
    parse_long(text)?.try_into().map_err(|_| ())
}

/// Parse a `long` exactly like `Long.parseLong`.
///
/// # Errors
///
/// Returns `Err(())` for any malformed or out-of-range input.
pub fn parse_long(text: &str) -> Result<i64, ()> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return Err(());
    }
    let (negative, digits) = match bytes[0] {
        b'-' => (true, &bytes[1..]),
        b'+' => (false, &bytes[1..]),
        _ => (false, bytes),
    };
    if digits.is_empty() {
        return Err(());
    }
    let mut value: i64 = 0;
    for &byte in digits {
        if !byte.is_ascii_digit() {
            return Err(());
        }
        let digit = i64::from(byte - b'0');
        value = value.checked_mul(10).ok_or(())?;
        value = if negative {
            value.checked_sub(digit).ok_or(())?
        } else {
            value.checked_add(digit).ok_or(())?
        };
    }
    Ok(value)
}

/// Parse with a radix, like `Integer.parseInt(s, radix)`.
///
/// # Errors
///
/// Returns `Err(())` for malformed input, bad radix, or overflow.
pub fn parse_int_radix(text: &str, radix: u32) -> Result<i32, ()> {
    i64::from(parse_long_radix(text, radix)?)
        .try_into()
        .map_err(|_| ())
}

/// Parse with a radix, like `Long.parseLong(s, radix)`.
///
/// # Errors
///
/// Returns `Err(())` for malformed input, bad radix, or overflow.
pub fn parse_long_radix(text: &str, radix: u32) -> Result<i64, ()> {
    if !(2..=36).contains(&radix) {
        return Err(());
    }
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return Err(());
    }
    let (negative, digits) = match bytes[0] {
        b'-' => (true, &bytes[1..]),
        b'+' => (false, &bytes[1..]),
        _ => (false, bytes),
    };
    if digits.is_empty() {
        return Err(());
    }
    let mut value: i64 = 0;
    for &byte in digits {
        let digit = match byte {
            b'0'..=b'9' => u32::from(byte - b'0'),
            b'a'..=b'z' => u32::from(byte - b'a') + 10,
            b'A'..=b'Z' => u32::from(byte - b'A') + 10,
            _ => return Err(()),
        };
        if digit >= radix {
            return Err(());
        }
        let digit = i64::from(digit);
        value = value.checked_mul(i64::from(radix)).ok_or(())?;
        value = if negative {
            value.checked_sub(digit).ok_or(())?
        } else {
            value.checked_add(digit).ok_or(())?
        };
    }
    Ok(value)
}

/// Format an `i64` in an arbitrary radix, like `Long.toBinaryString`/`toHexString`/`toString`.
#[must_use]
pub fn long_to_radix_string(mut value: i64, radix: u32) -> String {
    if radix == 10 {
        return value.to_string();
    }
    if value == 0 {
        return "0".into();
    }
    let negative = value < 0;
    let mut digits = [0u8; 65];
    let mut count = 0;
    // Operate on the two's-complement magnitude so `i64::MIN` formats correctly.
    while value != 0 {
        let digit = (value % i64::from(radix)).unsigned_abs() as u32;
        digits[count] = core::char::from_digit(digit, radix).unwrap_or('?') as u8;
        count += 1;
        value /= i64::from(radix);
    }
    if negative {
        digits[count] = b'-';
        count += 1;
    }
    let mut out = String::with_capacity(count);
    for index in (0..count).rev() {
        out.push(digits[index] as char);
    }
    out
}

/// Format an `i32` in an arbitrary radix.
#[must_use]
pub fn int_to_radix_string(value: i32, radix: u32) -> String {
    if radix == 10 {
        return value.to_string();
    }
    if value == 0 {
        return "0".into();
    }
    let negative = value < 0;
    let mut digits = [0u8; 34];
    let mut count = 0;
    let mut value = value;
    while value != 0 {
        let digit = (value % radix as i32).unsigned_abs();
        digits[count] = core::char::from_digit(digit, radix).unwrap_or('?') as u8;
        count += 1;
        value /= radix as i32;
    }
    if negative {
        digits[count] = b'-';
        count += 1;
    }
    let mut out = String::with_capacity(count);
    for index in (0..count).rev() {
        out.push(digits[index] as char);
    }
    out
}

/// Parse a `float` like `Float.parseFloat`, including `NaN`, `Infinity`, and hex literals.
///
/// # Errors
///
/// Returns `Err(())` for malformed input.
pub fn parse_float(text: &str) -> Result<f32, ()> {
    let trimmed = text.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return parse_hex_float32(hex);
    }
    let special = match trimmed {
        "NaN" => f32::NAN,
        "Infinity" | "+Infinity" => f32::INFINITY,
        "-Infinity" => f32::NEG_INFINITY,
        _ => {
            let lower = trimmed.to_ascii_lowercase();
            return lower.parse::<f32>().map_err(|_| ());
        }
    };
    Ok(special)
}

/// Parse a `double` like `Double.parseDouble`.
///
/// # Errors
///
/// Returns `Err(())` for malformed input.
pub fn parse_double(text: &str) -> Result<f64, ()> {
    let trimmed = text.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return parse_hex_float64(hex);
    }
    match trimmed {
        "NaN" => Ok(f64::NAN),
        "Infinity" | "+Infinity" => Ok(f64::INFINITY),
        "-Infinity" => Ok(f64::NEG_INFINITY),
        _ => {
            let lower = trimmed.to_ascii_lowercase();
            lower.parse::<f64>().map_err(|_| ())
        }
    }
}

fn parse_hex_float32(text: &str) -> Result<f32, ()> {
    let (mantissa, exponent) = split_hex_exponent(text);
    let (negative, mantissa) = match mantissa.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, mantissa.strip_prefix('+').unwrap_or(mantissa)),
    };
    let (integer, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if integer.is_empty() && fraction.is_empty() {
        return Err(());
    }
    let mut value = 0.0f32;
    for ch in integer.chars() {
        value = value * 16.0 + f32::from(ch.to_digit(16).ok_or(())? as u8);
    }
    let mut scale = 1.0f32 / 16.0;
    for ch in fraction.chars() {
        value += f32::from(ch.to_digit(16).ok_or(())? as u8) * scale;
        scale /= 16.0;
    }
    if let Some(exponent) = exponent {
        let exponent: i32 = exponent.parse().map_err(|_| ())?;
        value *= pow2_f32(exponent);
    }
    Ok(if negative { -value } else { value })
}

fn parse_hex_float64(text: &str) -> Result<f64, ()> {
    let (mantissa, exponent) = split_hex_exponent(text);
    let (negative, mantissa) = match mantissa.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, mantissa.strip_prefix('+').unwrap_or(mantissa)),
    };
    let (integer, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if integer.is_empty() && fraction.is_empty() {
        return Err(());
    }
    let mut value = 0.0f64;
    for ch in integer.chars() {
        value = value * 16.0 + f64::from(ch.to_digit(16).ok_or(())? as u8);
    }
    let mut scale = 1.0f64 / 16.0;
    for ch in fraction.chars() {
        value += f64::from(ch.to_digit(16).ok_or(())? as u8) * scale;
        scale /= 16.0;
    }
    if let Some(exponent) = exponent {
        let exponent: i32 = exponent.parse().map_err(|_| ())?;
        value *= pow2_f64(exponent);
    }
    Ok(if negative { -value } else { value })
}

fn split_hex_exponent(text: &str) -> (&str, Option<&str>) {
    if let Some(index) = text.rfind(['p', 'P']) {
        (&text[..index], Some(&text[index + 1..]))
    } else {
        (text, None)
    }
}

fn pow2_f32(exponent: i32) -> f32 {
    let mut value = 1.0f32;
    if exponent >= 0 {
        for _ in 0..exponent {
            value *= 2.0;
        }
    } else {
        for _ in 0..-exponent {
            value *= 0.5;
        }
    }
    value
}

fn pow2_f64(exponent: i32) -> f64 {
    let mut value = 1.0f64;
    if exponent >= 0 {
        for _ in 0..exponent {
            value *= 2.0;
        }
    } else {
        for _ in 0..-exponent {
            value *= 0.5;
        }
    }
    value
}
