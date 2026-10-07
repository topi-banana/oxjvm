//! JVM values and object references.

use core::fmt;

/// A reference to a heap object. Zero is the null reference; real objects start at 1.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectRef(pub(crate) u32);

impl ObjectRef {
    /// The null reference.
    pub const NULL: Self = Self(0);

    /// Wrap a raw index (0 means null).
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw index (0 means null).
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Whether this is the null reference.
    #[must_use]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }

    /// Whether this is a real reference.
    #[must_use]
    pub const fn is_null_or(self, other: Self) -> bool {
        self.0 == 0 || self.0 == other.0
    }

    pub(crate) const fn slot(self) -> usize {
        self.0 as usize - 1
    }
}

impl fmt::Debug for ObjectRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_null() {
            f.write_str("null")
        } else {
            write!(f, "obj#{}", self.0)
        }
    }
}

/// A JVM value on the operand stack or in a local variable.
///
/// Category-2 values (`long`/`double`) are held in one `Value` even though they occupy two slots;
/// slot accounting is the interpreter's job.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    /// `int`, `boolean`, `byte`, `char`, `short`, and all reference *pointers* are `Int` only when
    /// the verifier says so; references use [`Value::Ref`].
    Int(i32),
    /// `long`.
    Long(i64),
    /// `float`.
    Float(f32),
    /// `double`.
    Double(f64),
    /// Reference (including `null` as [`ObjectRef::NULL`]).
    Ref(ObjectRef),
}

impl Value {
    /// The value `0` of the given primitive descriptor, or null for references.
    #[must_use]
    pub const fn default_for(descriptor: u8) -> Self {
        match descriptor {
            b'J' => Self::Long(0),
            b'F' => Self::Float(0.0),
            b'D' => Self::Double(0.0),
            b'L' | b'[' => Self::Ref(ObjectRef::NULL),
            _ => Self::Int(0),
        }
    }

    /// Number of operand-stack slots this value occupies.
    #[must_use]
    pub const fn slots(&self) -> u16 {
        match self {
            Self::Long(_) | Self::Double(_) => 2,
            _ => 1,
        }
    }

    /// Whether this is a category-2 value.
    #[must_use]
    pub const fn is_category2(&self) -> bool {
        self.slots() == 2
    }

    /// Extract an `int`-shaped value (`int`, `boolean`, `byte`, `char`, `short`).
    ///
    /// # Panics
    ///
    /// Panics on a value that the verifier should have rejected.
    #[must_use]
    pub fn as_int(self) -> i32 {
        match self {
            Self::Int(value) => value,
            other => panic!("expected int, found {other:?}"),
        }
    }

    /// Extract a `long`.
    ///
    /// # Panics
    ///
    /// Panics on a value that the verifier should have rejected.
    #[must_use]
    pub fn as_long(self) -> i64 {
        match self {
            Self::Long(value) => value,
            other => panic!("expected long, found {other:?}"),
        }
    }

    /// Extract a `float`.
    ///
    /// # Panics
    ///
    /// Panics on a value that the verifier should have rejected.
    #[must_use]
    pub fn as_float(self) -> f32 {
        match self {
            Self::Float(value) => value,
            other => panic!("expected float, found {other:?}"),
        }
    }

    /// Extract a `double`.
    ///
    /// # Panics
    ///
    /// Panics on a value that the verifier should have rejected.
    #[must_use]
    pub fn as_double(self) -> f64 {
        match self {
            Self::Double(value) => value,
            other => panic!("expected double, found {other:?}"),
        }
    }

    /// Extract a reference.
    ///
    /// # Panics
    ///
    /// Panics on a value that the verifier should have rejected.
    #[must_use]
    pub fn as_ref(self) -> ObjectRef {
        match self {
            Self::Ref(value) => value,
            other => panic!("expected reference, found {other:?}"),
        }
    }

    /// Whether this is the null reference.
    #[must_use]
    pub const fn is_null_ref(self) -> bool {
        matches!(self, Self::Ref(r) if r.is_null())
    }

    /// The `boolean` interpretation of an int-shaped value (nonzero is true).
    #[must_use]
    pub fn as_boolean(self) -> bool {
        self.as_int() != 0
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Self::Int(value)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self::Long(value)
    }
}

impl From<f32> for Value {
    fn from(value: f32) -> Self {
        Self::Float(value)
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Self::Double(value)
    }
}

impl From<ObjectRef> for Value {
    fn from(value: ObjectRef) -> Self {
        Self::Ref(value)
    }
}

/// Compare two floats like `fcmpl`/`fcmpg`: `-1`/`0`/`1`, with NaN ordering chosen by `nan_result`.
#[must_use]
pub fn compare_f32(a: f32, b: f32, nan_result: i32) -> i32 {
    if a < b {
        -1
    } else if a > b {
        1
    } else if a == b {
        0
    } else {
        nan_result
    }
}

/// Compare two doubles like `dcmpl`/`dcmpg`.
#[must_use]
pub fn compare_f64(a: f64, b: f64, nan_result: i32) -> i32 {
    if a < b {
        -1
    } else if a > b {
        1
    } else if a == b {
        0
    } else {
        nan_result
    }
}
