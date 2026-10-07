//! Field and method descriptors (JVMS 4.3) and class signatures (JVMS 4.7.9.1).
//!
//! Descriptors appear throughout the runtime — in the constant pool, in `method_info`, and in
//! every `invoke*` instruction — so they are parsed into a structural form once, here, and never
//! re-scanned as raw text by the rest of the runtime.

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

/// A base (primitive) type, including `void` for method returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BaseType {
    /// `B`
    Byte,
    /// `C`
    Char,
    /// `D`
    Double,
    /// `F`
    Float,
    /// `I`
    Int,
    /// `J`
    Long,
    /// `S`
    Short,
    /// `Z`
    Boolean,
    /// `V` (valid only as a method return type).
    Void,
}

impl BaseType {
    /// Parse a single base-type character.
    #[must_use]
    pub const fn from_char(ch: u8) -> Option<Self> {
        Some(match ch {
            b'B' => Self::Byte,
            b'C' => Self::Char,
            b'D' => Self::Double,
            b'F' => Self::Float,
            b'I' => Self::Int,
            b'J' => Self::Long,
            b'S' => Self::Short,
            b'Z' => Self::Boolean,
            b'V' => Self::Void,
            _ => return None,
        })
    }

    /// The descriptor character.
    #[must_use]
    pub const fn as_char(self) -> char {
        match self {
            Self::Byte => 'B',
            Self::Char => 'C',
            Self::Double => 'D',
            Self::Float => 'F',
            Self::Int => 'I',
            Self::Long => 'J',
            Self::Short => 'S',
            Self::Boolean => 'Z',
            Self::Void => 'V',
        }
    }

    /// Number of operand-stack slots a value of this type occupies (two for long/double).
    #[must_use]
    pub const fn slots(self) -> u16 {
        match self {
            Self::Long | Self::Double => 2,
            _ => 1,
        }
    }

    /// The wrapper class internal name for a primitive, if any.
    #[must_use]
    pub const fn wrapper(self) -> Option<&'static str> {
        Some(match self {
            Self::Byte => "java/lang/Byte",
            Self::Char => "java/lang/Character",
            Self::Double => "java/lang/Double",
            Self::Float => "java/lang/Float",
            Self::Int => "java/lang/Integer",
            Self::Long => "java/lang/Long",
            Self::Short => "java/lang/Short",
            Self::Boolean => "java/lang/Boolean",
            Self::Void => "java/lang/Void",
        })
    }
}

/// A field type: base, object, or array.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FieldType {
    /// A primitive type (`void` is not a valid field type).
    Base(BaseType),
    /// An object type, by internal name (`java/lang/String`).
    Object(String),
    /// An array type, of any component depth.
    Array(Box<FieldType>),
}

impl FieldType {
    /// Parse exactly one field type from the front of `text`.
    ///
    /// # Errors
    ///
    /// Returns a message when the descriptor is malformed.
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        let (ty, rest) = Self::parse_prefix(text)?;
        if rest.is_empty() {
            Ok(ty)
        } else {
            Err("trailing characters after field descriptor")
        }
    }

    /// Parse a field type prefix, returning the type and the unconsumed suffix.
    ///
    /// # Errors
    ///
    /// Returns a message when the prefix is not a valid field descriptor.
    pub fn parse_prefix(text: &str) -> Result<(Self, &str), &'static str> {
        let bytes = text.as_bytes();
        let Some(&first) = bytes.first() else {
            return Err("empty field descriptor");
        };
        match first {
            b'[' => {
                let (component, rest) = Self::parse_prefix(&text[1..])?;
                Ok((Self::Array(Box::new(component)), rest))
            }
            b'L' => {
                let Some(end) = text.find(';') else {
                    return Err("unterminated object descriptor");
                };
                let name = &text[1..end];
                if name.is_empty() {
                    return Err("empty object descriptor");
                }
                Ok((Self::Object(name.to_string()), &text[end + 1..]))
            }
            other => {
                let base = BaseType::from_char(other).ok_or("bad base type in descriptor")?;
                if base == BaseType::Void {
                    return Err("void is not a field type");
                }
                Ok((Self::Base(base), &text[1..]))
            }
        }
    }

    /// Number of operand-stack slots.
    #[must_use]
    pub fn slots(&self) -> u16 {
        match self {
            Self::Base(base) => base.slots(),
            Self::Object(_) | Self::Array(_) => 1,
        }
    }

    /// Whether this is `long` or `double`.
    #[must_use]
    pub fn is_category2(&self) -> bool {
        self.slots() == 2
    }

    /// The internal name of the class this type denotes, or `None` for primitives.
    #[must_use]
    pub fn class_name(&self) -> Option<&str> {
        match self {
            Self::Object(name) => Some(name),
            Self::Array(_) => Some(""), // filled by `descriptor` below
            Self::Base(_) => None,
        }
    }

    /// The JVM descriptor text for this type.
    #[must_use]
    pub fn descriptor(&self) -> String {
        let mut out = String::new();
        self.write_descriptor(&mut out);
        out
    }

    fn write_descriptor(&self, out: &mut String) {
        match self {
            Self::Base(base) => out.push(base.as_char()),
            Self::Object(name) => {
                out.push('L');
                out.push_str(name);
                out.push(';');
            }
            Self::Array(component) => {
                out.push('[');
                component.write_descriptor(out);
            }
        }
    }

    /// The internal name of this array/object type (`[I`, `java/lang/String`).
    #[must_use]
    pub fn internal_name(&self) -> String {
        match self {
            Self::Object(name) => name.clone(),
            Self::Array(_) => self.descriptor(),
            Self::Base(base) => base.wrapper().unwrap_or("java/lang/Object").to_string(),
        }
    }
}

impl fmt::Display for FieldType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.descriptor())
    }
}

/// A parsed method descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MethodDescriptor {
    /// Parameter types, in order.
    pub parameters: Vec<FieldType>,
    /// Return type; `None` means `void`.
    pub returns: Option<FieldType>,
}

impl MethodDescriptor {
    /// Parse a method descriptor such as `(ILjava/lang/String;)J`.
    ///
    /// # Errors
    ///
    /// Returns a message when the descriptor is malformed.
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        let Some(rest) = text.strip_prefix('(') else {
            return Err("method descriptor must start with '('");
        };
        let Some(close) = rest.find(')') else {
            return Err("method descriptor missing ')'");
        };
        let params_text = &rest[..close];
        let ret_text = &rest[close + 1..];
        let mut parameters = Vec::new();
        let mut cursor = params_text;
        while !cursor.is_empty() {
            let (ty, next) = FieldType::parse_prefix(cursor)?;
            parameters.push(ty);
            cursor = next;
        }
        if ret_text.is_empty() {
            return Err("method descriptor missing return type");
        }
        let returns = if ret_text == "V" {
            None
        } else {
            Some(FieldType::parse(ret_text)?)
        };
        Ok(Self {
            parameters,
            returns,
        })
    }

    /// Number of operand-stack slots taken by the parameters.
    #[must_use]
    pub fn parameter_slots(&self) -> u16 {
        self.parameters.iter().map(FieldType::slots).sum()
    }

    /// The descriptor text.
    #[must_use]
    pub fn descriptor(&self) -> String {
        let mut out = String::from("(");
        for parameter in &self.parameters {
            parameter.write_descriptor(&mut out);
        }
        out.push(')');
        match &self.returns {
            Some(ret) => ret.write_descriptor(&mut out),
            None => out.push('V'),
        }
        out
    }
}

impl fmt::Display for MethodDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.descriptor())
    }
}

/// Count the operand-stack slots a descriptor's arguments take, without full parsing.
///
/// This is the hot path in `invoke*` decoding and matches [`MethodDescriptor::parameter_slots`].
#[must_use]
pub fn parameter_slot_count(descriptor: &str) -> u16 {
    let mut slots = 0;
    let mut chars = descriptor.bytes();
    if chars.next() != Some(b'(') {
        return 0;
    }
    while let Some(ch) = chars.next() {
        match ch {
            b')' => break,
            b'J' | b'D' => slots += 2,
            b'[' => {
                // Skip the array prefix and any object component.
                let mut inner = chars.next();
                while inner == Some(b'[') {
                    inner = chars.next();
                }
                if inner == Some(b'L') {
                    for c in chars.by_ref() {
                        if c == b';' {
                            break;
                        }
                    }
                }
                slots += 1;
            }
            b'L' => {
                for c in chars.by_ref() {
                    if c == b';' {
                        break;
                    }
                }
                slots += 1;
            }
            _ => slots += 1,
        }
    }
    slots
}

/// Split an internal class name into `(package-with-slashes, simple-name)`.
#[must_use]
pub fn split_internal_name(internal: &str) -> (&str, &str) {
    match internal.rfind('/') {
        Some(index) => (&internal[..index], &internal[index + 1..]),
        None => ("", internal),
    }
}

/// Convert an internal name to a binary (dotted) name.
#[must_use]
pub fn internal_to_binary(internal: &str) -> String {
    internal.replace('/', ".")
}
