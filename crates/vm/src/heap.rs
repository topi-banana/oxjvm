//! The object heap and the mark-sweep collector.

use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::string::String;
use alloc::vec::Vec;
use core::mem::size_of;

use crate::class::ClassId;
use crate::value::{ObjectRef, Value};

/// The per-object overhead charged to every allocation, in bytes.
///
/// It stands in for the header every real JVM object carries: the mark word, the class
/// pointer, and the identity hash the collector keeps per object.
pub const OBJECT_HEADER_BYTES: u64 = 16;

/// One object's payload.
#[derive(Debug, Clone, PartialEq)]
pub enum ObjectData {
    /// A plain instance: one [`Value`] per instance field slot.
    Instance(Vec<Value>),
    /// An array.
    Array(ArrayData),
    /// A `java.lang.String` payload, held as UTF-16 code units exactly as Java does.
    String(Vec<u16>),
    /// A `java.lang.Class` instance denoting a runtime class.
    Class(ClassId),
    /// A `java.lang.invoke.MethodType`.
    MethodType(String),
    /// A `java.lang.invoke.MethodHandle`.
    MethodHandle(MethodHandleValue),
    /// A captured stack trace attached to a `Throwable`.
    Backtrace(Vec<BacktraceFrame>),
}

impl ObjectData {
    /// The size of the payload in bytes, excluding the object header.
    #[must_use]
    pub fn payload_bytes(&self) -> u64 {
        match self {
            Self::Instance(fields) => (fields.len() * size_of::<Value>()) as u64,
            Self::Array(array) => array.payload_bytes(),
            Self::String(units) => (units.len() * size_of::<u16>()) as u64,
            Self::Class(_) => 0,
            Self::MethodType(descriptor) => descriptor.len() as u64,
            Self::MethodHandle(handle) => handle.payload_bytes(),
            Self::Backtrace(frames) => (frames.len() * size_of::<BacktraceFrame>()) as u64,
        }
    }

    /// The total number of bytes the object occupies, header included.
    #[must_use]
    pub fn object_size(&self) -> u64 {
        OBJECT_HEADER_BYTES + self.payload_bytes()
    }
}

/// A captured Java stack frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacktraceFrame {
    /// Binary class name.
    pub class_name: String,
    /// Method name.
    pub method_name: String,
    /// Source file, if known.
    pub file_name: Option<String>,
    /// Line number, if known.
    pub line_number: Option<u16>,
}

/// The array payload, unboxed per component type.
#[derive(Debug, Clone, PartialEq)]
pub enum ArrayData {
    /// `boolean[]`.
    Boolean(Vec<u8>),
    /// `byte[]`.
    Byte(Vec<i8>),
    /// `char[]`.
    Char(Vec<u16>),
    /// `short[]`.
    Short(Vec<i16>),
    /// `int[]`.
    Int(Vec<i32>),
    /// `long[]`.
    Long(Vec<i64>),
    /// `float[]`.
    Float(Vec<f32>),
    /// `double[]`.
    Double(Vec<f64>),
    /// An object array.
    Reference(Vec<ObjectRef>),
}

impl ArrayData {
    /// Number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Boolean(v) => v.len(),
            Self::Byte(v) => v.len(),
            Self::Char(v) => v.len(),
            Self::Short(v) => v.len(),
            Self::Int(v) => v.len(),
            Self::Long(v) => v.len(),
            Self::Float(v) => v.len(),
            Self::Double(v) => v.len(),
            Self::Reference(v) => v.len(),
        }
    }

    /// The size of the elements in bytes.
    #[must_use]
    pub fn payload_bytes(&self) -> u64 {
        match self {
            Self::Boolean(v) => v.len() as u64,
            Self::Byte(v) => v.len() as u64,
            Self::Char(v) => (v.len() * size_of::<u16>()) as u64,
            Self::Short(v) => (v.len() * size_of::<i16>()) as u64,
            Self::Int(v) => (v.len() * size_of::<i32>()) as u64,
            Self::Long(v) => (v.len() * size_of::<i64>()) as u64,
            Self::Float(v) => (v.len() * size_of::<f32>()) as u64,
            Self::Double(v) => (v.len() * size_of::<f64>()) as u64,
            Self::Reference(v) => (v.len() * size_of::<ObjectRef>()) as u64,
        }
    }

    /// The number of bytes one element of `component` occupies.
    #[must_use]
    pub const fn element_size(component: &crate::class::ArrayComponent) -> usize {
        use crate::class::ArrayComponent as C;
        match component {
            C::Boolean | C::Byte => 1,
            C::Char | C::Short => 2,
            C::Int | C::Float => 4,
            C::Long | C::Double => 8,
            C::Reference => size_of::<ObjectRef>(),
        }
    }

    /// The number of bytes an array of `len` elements occupies, header included.
    #[must_use]
    pub fn size_of(component: &crate::class::ArrayComponent, len: usize) -> u64 {
        OBJECT_HEADER_BYTES + (len as u64).saturating_mul(Self::element_size(component) as u64)
    }

    /// Whether the array has no elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The component category of this array.
    #[must_use]
    pub const fn component(&self) -> crate::class::ArrayComponent {
        use crate::class::ArrayComponent as C;
        match self {
            Self::Boolean(_) => C::Boolean,
            Self::Byte(_) => C::Byte,
            Self::Char(_) => C::Char,
            Self::Short(_) => C::Short,
            Self::Int(_) => C::Int,
            Self::Long(_) => C::Long,
            Self::Float(_) => C::Float,
            Self::Double(_) => C::Double,
            Self::Reference(_) => C::Reference,
        }
    }

    /// Read an element as a [`Value`].
    #[must_use]
    pub fn get(&self, index: usize) -> Value {
        match self {
            Self::Boolean(v) => Value::Int(i32::from(v[index])),
            Self::Byte(v) => Value::Int(i32::from(v[index])),
            Self::Char(v) => Value::Int(i32::from(v[index])),
            Self::Short(v) => Value::Int(i32::from(v[index])),
            Self::Int(v) => Value::Int(v[index]),
            Self::Long(v) => Value::Long(v[index]),
            Self::Float(v) => Value::Float(v[index]),
            Self::Double(v) => Value::Double(v[index]),
            Self::Reference(v) => Value::Ref(v[index]),
        }
    }

    /// Write an element from a [`Value`].
    pub fn set(&mut self, index: usize, value: Value) {
        match self {
            Self::Boolean(v) => v[index] = u8::from(value.as_int() != 0),
            Self::Byte(v) => v[index] = value.as_int() as i8,
            Self::Char(v) => v[index] = (value.as_int() as u32 & 0xFFFF) as u16,
            Self::Short(v) => v[index] = value.as_int() as i16,
            Self::Int(v) => v[index] = value.as_int(),
            Self::Long(v) => v[index] = value.as_long(),
            Self::Float(v) => v[index] = value.as_float(),
            Self::Double(v) => v[index] = value.as_double(),
            Self::Reference(v) => v[index] = value.as_ref(),
        }
    }

    /// Allocate a zeroed array of `len` elements for a component type.
    ///
    /// Returns `None` when the host allocator cannot provide the memory, so the VM can raise
    /// `OutOfMemoryError` instead of aborting the process.
    #[must_use]
    pub fn zeroed(component: &crate::class::ArrayComponent, len: usize) -> Option<Self> {
        use crate::class::ArrayComponent as C;
        macro_rules! zeroed {
            ($variant:ident, $value:expr) => {{
                let mut data = Vec::new();
                data.try_reserve_exact(len).ok()?;
                data.resize(len, $value);
                Some(Self::$variant(data))
            }};
        }
        match component {
            C::Boolean => zeroed!(Boolean, 0u8),
            C::Byte => zeroed!(Byte, 0i8),
            C::Char => zeroed!(Char, 0u16),
            C::Short => zeroed!(Short, 0i16),
            C::Int => zeroed!(Int, 0i32),
            C::Long => zeroed!(Long, 0i64),
            C::Float => zeroed!(Float, 0.0f32),
            C::Double => zeroed!(Double, 0.0f64),
            C::Reference => zeroed!(Reference, ObjectRef::NULL),
        }
    }
}

/// The value of a `java.lang.invoke.MethodHandle`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MethodHandleValue {
    /// A direct static method handle.
    Static {
        /// Declaring class.
        class: ClassId,
        /// Method index.
        method: u32,
    },
    /// A direct virtual (including interface) method handle.
    Virtual {
        /// Declaring class.
        class: ClassId,
        /// Method index.
        method: u32,
    },
    /// A direct special (super/private/constructor) method handle.
    Special {
        /// Declaring class.
        class: ClassId,
        /// Method index.
        method: u32,
    },
    /// A constructor handle: allocate `class` then invoke its constructor.
    New {
        /// The class to instantiate.
        class: ClassId,
        /// The class declaring the constructor (a superclass for native classes).
        declaring: ClassId,
        /// The constructor method index on `declaring`.
        method: u32,
    },
    /// A static field getter.
    StaticGetter {
        /// Declaring class.
        class: ClassId,
        /// Field index.
        field: u32,
    },
    /// A static field setter.
    StaticSetter {
        /// Declaring class.
        class: ClassId,
        /// Field index.
        field: u32,
    },
    /// An instance field getter.
    Getter {
        /// Declaring class.
        class: ClassId,
        /// Field index.
        field: u32,
    },
    /// An instance field setter.
    Setter {
        /// Declaring class.
        class: ClassId,
        /// Field index.
        field: u32,
    },
    /// A bound instance method handle (receiver already captured).
    Bound {
        /// The captured receiver.
        receiver: ObjectRef,
        /// The underlying target.
        target: alloc::boxed::Box<MethodHandleValue>,
    },
    /// An identity cast handle.
    Identity {
        /// The class the value is cast to.
        class: ClassId,
    },
}

impl MethodHandleValue {
    /// The size of the handle's payload in bytes.
    #[must_use]
    pub fn payload_bytes(&self) -> u64 {
        match self {
            Self::Static { .. }
            | Self::Virtual { .. }
            | Self::Special { .. }
            | Self::New { .. }
            | Self::StaticGetter { .. }
            | Self::StaticSetter { .. }
            | Self::Getter { .. }
            | Self::Setter { .. }
            | Self::Identity { .. } => 0,
            Self::Bound { target, .. } => {
                size_of::<MethodHandleValue>() as u64 + target.payload_bytes()
            }
        }
    }
}

/// A per-object monitor (intrinsic lock).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Monitor {
    /// The thread index currently holding the monitor.
    pub owner: Option<usize>,
    /// Re-entrant hold count.
    pub count: u32,
    /// Threads blocked entering the monitor (`monitorenter`).
    pub entrants: VecDeque<usize>,
    /// Threads waiting on the monitor (`Object.wait`).
    pub waiters: VecDeque<usize>,
}

/// One heap object.
#[derive(Debug, Clone)]
pub struct Object {
    /// The runtime class of this object.
    pub class: ClassId,
    /// The payload.
    pub data: ObjectData,
    /// The identity hash code, stable for the object's lifetime.
    pub hash: i32,
    /// The intrinsic lock.
    pub monitor: Monitor,
    /// GC mark bit.
    pub marked: bool,
    /// Whether this object is a candidate for sweeping (never roots).
    pub pinned: bool,
}

impl Object {
    /// The array payload, if this object is an array.
    #[must_use]
    pub const fn data_array(&self) -> Option<&ArrayData> {
        match &self.data {
            ObjectData::Array(array) => Some(array),
            _ => None,
        }
    }
}

/// The heap could not satisfy an allocation within its byte budget.
///
/// The VM turns this into a `java.lang.OutOfMemoryError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutOfMemory;

/// The object heap, with a mark-sweep collector.
#[derive(Debug, Default)]
pub struct Heap {
    objects: Vec<Option<Object>>,
    free: Vec<u32>,
    next_hash: u32,
    /// Number of live objects (for statistics and thresholds).
    pub live: usize,
    /// Bytes occupied by the live objects.
    used_bytes: u64,
    /// The byte budget the heap must stay within; `None` means unbounded.
    limit_bytes: Option<u64>,
}

impl Heap {
    /// An empty, unbounded heap.
    #[must_use]
    pub fn new() -> Self {
        Self::with_limit(None)
    }

    /// An empty heap with a byte budget; `None` means unbounded.
    #[must_use]
    pub fn with_limit(limit_bytes: Option<u64>) -> Self {
        Self {
            objects: Vec::new(),
            free: Vec::new(),
            next_hash: 1,
            live: 0,
            used_bytes: 0,
            limit_bytes,
        }
    }

    /// The byte budget the heap must stay within, or `None` when it is unbounded.
    #[must_use]
    pub const fn limit_bytes(&self) -> Option<u64> {
        self.limit_bytes
    }

    /// Set the byte budget the heap must stay within; `None` makes the heap unbounded.
    pub fn set_limit_bytes(&mut self, limit_bytes: Option<u64>) {
        self.limit_bytes = limit_bytes;
    }

    /// The bytes occupied by the live objects.
    #[must_use]
    pub const fn used_bytes(&self) -> u64 {
        self.used_bytes
    }

    /// Whether an allocation of `bytes` fits in the budget.
    #[must_use]
    pub fn can_fit(&self, bytes: u64) -> bool {
        self.limit_bytes
            .is_none_or(|limit| self.used_bytes.saturating_add(bytes) <= limit)
    }

    /// Allocate an object slot and return its reference.
    ///
    /// Fails with [`OutOfMemory`] when the object would push the heap past its byte budget.
    /// An unbounded heap never fails.
    pub fn allocate(&mut self, class: ClassId, data: ObjectData) -> Result<ObjectRef, OutOfMemory> {
        if !self.can_fit(data.object_size()) {
            return Err(OutOfMemory);
        }
        Ok(self.allocate_unchecked(class, data))
    }

    /// Allocate an object even when the byte budget is exhausted.
    ///
    /// The bytes are still charged to the heap, but the budget is not enforced. This is how the
    /// `OutOfMemoryError` itself is created once the heap is full.
    pub fn allocate_unchecked(&mut self, class: ClassId, data: ObjectData) -> ObjectRef {
        let bytes = data.object_size();
        let hash = self.mix_hash();
        let object = Object {
            class,
            data,
            hash,
            monitor: Monitor::default(),
            marked: false,
            pinned: false,
        };
        let reference = if let Some(index) = self.free.pop() {
            self.objects[index as usize] = Some(object);
            ObjectRef(index + 1)
        } else {
            self.objects.push(Some(object));
            ObjectRef(self.objects.len() as u32)
        };
        self.live += 1;
        self.used_bytes += bytes;
        reference
    }

    fn mix_hash(&mut self) -> i32 {
        // Marsaglia-style mixing of a counter: stable, well-distributed identity hashes.
        let mut x = self.next_hash;
        self.next_hash = self.next_hash.wrapping_add(1);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        x as i32
    }

    /// Borrow an object, or `None` for null or a dead slot.
    #[must_use]
    pub fn get(&self, reference: ObjectRef) -> Option<&Object> {
        if reference.is_null() {
            return None;
        }
        self.objects.get(reference.slot())?.as_ref()
    }

    /// Replace an object's payload, keeping the byte accounting in step.
    ///
    /// Returns the new object size, or `None` when the reference is null or a dead slot.
    pub fn replace_payload(&mut self, reference: ObjectRef, data: ObjectData) -> Option<u64> {
        if reference.is_null() {
            return None;
        }
        let bytes = data.object_size();
        let previous = {
            let object = self.objects.get_mut(reference.slot())?.as_mut()?;
            let previous = object.data.object_size();
            object.data = data;
            previous
        };
        self.used_bytes = self.used_bytes.saturating_sub(previous) + bytes;
        Some(bytes)
    }

    /// Mutably borrow an object, or `None` for null or a dead slot.
    pub fn get_mut(&mut self, reference: ObjectRef) -> Option<&mut Object> {
        if reference.is_null() {
            return None;
        }
        self.objects.get_mut(reference.slot())?.as_mut()
    }

    /// Total heap slots (live plus free).
    #[must_use]
    pub fn capacity_slots(&self) -> usize {
        self.objects.len()
    }

    /// Run a mark-sweep collection. `roots` yields every object that must be kept; children are
    /// traced through the object graph automatically.
    pub fn collect(&mut self, roots: impl Iterator<Item = ObjectRef>) {
        let mut work: Vec<ObjectRef> = roots.filter(|reference| !reference.is_null()).collect();
        while let Some(reference) = work.pop() {
            if reference.is_null() {
                continue;
            }
            let Some(slot) = self.objects.get_mut(reference.slot()) else {
                continue;
            };
            let Some(object) = slot.as_mut() else {
                continue;
            };
            if object.marked {
                continue;
            }
            object.marked = true;
            Self::for_each_child(object, |child| work.push(child));
        }
        for slot in &mut self.objects {
            if let Some(object) = slot {
                if !object.marked {
                    self.used_bytes = self.used_bytes.saturating_sub(object.data.object_size());
                    *slot = None;
                    self.live -= 1;
                } else {
                    object.marked = false;
                }
            }
        }
        // Rebuild the free list from the swept holes.
        self.free.clear();
        for (index, slot) in self.objects.iter().enumerate() {
            if slot.is_none() {
                self.free.push(index as u32);
            }
        }
        self.free.reverse();
    }

    /// Iterate the direct references held by an object.
    pub fn for_each_child(object: &Object, mut visit: impl FnMut(ObjectRef)) {
        match &object.data {
            ObjectData::Instance(fields) => {
                for value in fields {
                    if let Value::Ref(reference) = value {
                        visit(*reference);
                    }
                }
            }
            ObjectData::Array(ArrayData::Reference(elements)) => {
                for reference in elements {
                    visit(*reference);
                }
            }
            ObjectData::MethodHandle(handle) => {
                let mut current = handle;
                while let MethodHandleValue::Bound { receiver, target } = current {
                    visit(*receiver);
                    current = target;
                }
            }
            ObjectData::String(_)
            | ObjectData::Array(_)
            | ObjectData::Class(_)
            | ObjectData::MethodType(_)
            | ObjectData::Backtrace(_) => {}
        }
    }
}

/// Monitors live per object; the VM collects objects directly rather than through this alias.
pub type Monitors = BTreeMap<ObjectRef, Monitor>;

/// Root sets used by the collector.
pub type RootSet = BTreeSet<ObjectRef>;
