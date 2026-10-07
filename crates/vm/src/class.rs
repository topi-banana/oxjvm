//! Runtime classes, fields, methods, and the native-class registry.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use oxjvm_classfile::ConstantPool;
use oxjvm_classfile::FieldType;
use oxjvm_platform::Host;

use crate::Vm;
use crate::error::VmError;
use crate::value::{ObjectRef, Value};

/// A runtime class index.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct ClassId(pub(crate) u32);

impl ClassId {
    /// The synthetic `java/lang/Object` slot, allocated first by [`ClassTable::new`].
    pub const OBJECT: Self = Self(0);

    /// Raw index.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// What kind of class this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassKind {
    /// An ordinary class (or interface/annotation/enum, per `access_flags`).
    Class,
    /// A primitive type class (`int.class`).
    Primitive,
    /// An array class (`[I`, `[Ljava/lang/String;`).
    Array,
}

/// The initialization state of a class (JVMS 5.5, 12.4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassState {
    /// Loaded and linked but not initialized.
    Prepared,
    /// Initialization is in progress on some thread.
    Initializing,
    /// Fully initialized.
    Initialized,
    /// Initialization failed; the error is remembered.
    Errored,
}

/// A field of a runtime class.
#[derive(Debug, Clone)]
pub struct Field {
    /// Field name.
    pub name: String,
    /// Field descriptor.
    pub descriptor: String,
    /// Access flags.
    pub access_flags: u16,
    /// Declaring class.
    pub owner: ClassId,
    /// Whether the field is static.
    pub is_static: bool,
    /// Slot index: instance layout slot, or index into `Class::static_values`.
    pub slot: u16,
    /// The `ConstantValue` attribute value, resolved at preparation time.
    pub constant: Option<Value>,
}

/// A method of a runtime class.
#[derive(Debug, Clone)]
pub struct Method {
    /// Method name (`<init>`, `<clinit>`, or a normal name).
    pub name: String,
    /// Method descriptor.
    pub descriptor: String,
    /// Access flags.
    pub access_flags: u16,
    /// Declaring class.
    pub owner: ClassId,
    /// The body, for non-native methods.
    pub code: Option<Arc<Code>>,
    /// The implementation, for native methods.
    pub native: Option<NativeFn>,
    /// The `Exceptions` attribute, resolved.
    pub exceptions: Vec<ClassId>,
    /// Parameter names from `MethodParameters`/`LocalVariableTable`, when present.
    pub parameter_names: Vec<Option<String>>,
}

impl Method {
    /// Whether this method has no Java body and no native implementation.
    #[must_use]
    pub fn is_abstract(&self) -> bool {
        self.code.is_none() && self.native.is_none()
    }

    /// Whether this method is `static`.
    #[must_use]
    pub const fn is_static(&self) -> bool {
        self.access_flags & oxjvm_classfile::flags::ACC_STATIC != 0
    }
}

/// A linked method body.
#[derive(Debug, Clone)]
pub struct Code {
    /// Bytecode.
    pub bytes: Vec<u8>,
    /// Operand-stack limit in slots.
    pub max_stack: u16,
    /// Local-variable limit in slots.
    pub max_locals: u16,
    /// Exception handlers.
    pub exception_table: Vec<Handler>,
    /// `(start_pc, line)` pairs, sorted by `start_pc`.
    pub line_numbers: Vec<(u16, u16)>,
}

impl Code {
    /// The source line for a bytecode offset.
    #[must_use]
    pub fn line_for(&self, pc: u16) -> Option<u16> {
        let mut line = None;
        for &(start, candidate) in &self.line_numbers {
            if start <= pc {
                line = Some(candidate);
            } else {
                break;
            }
        }
        line
    }
}

/// A linked exception handler.
#[derive(Debug, Clone, Copy)]
pub struct Handler {
    /// Protected range start (inclusive).
    pub start_pc: u16,
    /// Protected range end (exclusive).
    pub end_pc: u16,
    /// Handler entry.
    pub handler_pc: u16,
    /// Constant-pool index of the caught class, or 0 for catch-all. Resolved lazily at dispatch.
    pub catch_type_index: u16,
}

/// The context a native method is called in: which class and method was invoked. Native
/// implementations use it to distinguish overloads and to delegate to superclass constructors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeContext {
    /// The class of the method being invoked.
    pub class: ClassId,
    /// The method index within that class.
    pub method: u32,
}

/// The native function signature: it receives the VM, the invocation context, and the arguments
/// (the receiver, for instance methods, is `args[0]`).
pub type NativeFn = for<'a> fn(&mut Vm<'a>, NativeContext, &[Value]) -> Result<Value, VmError>;

/// A hook run when a native class is initialized (`System.out` wiring, the main thread, ...).
pub type NativeClinit = for<'a> fn(&mut Vm<'a>) -> Result<(), VmError>;

/// A constant declared by a native class's static final field.
#[derive(Debug, Clone, Copy)]
pub enum NativeConstant {
    /// `int`/`boolean`/`char`/`short`/`byte`.
    Int(i32),
    /// `long`.
    Long(i64),
    /// `float`.
    Float(f32),
    /// `double`.
    Double(f64),
    /// `java.lang.String`.
    Str(&'static str),
}

/// A static or instance field contributed by a native class.
#[derive(Debug, Clone)]
pub struct NativeFieldDef {
    /// Name.
    pub name: &'static str,
    /// Descriptor.
    pub descriptor: &'static str,
    /// Access flags.
    pub access_flags: u16,
    /// Constant value, if any.
    pub constant: Option<NativeConstant>,
}

/// A method contributed by a native class.
#[derive(Debug, Clone, Copy)]
pub struct NativeMethodDef {
    /// Name.
    pub name: &'static str,
    /// Descriptor.
    pub descriptor: &'static str,
    /// Access flags.
    pub access_flags: u16,
    /// Implementation.
    pub native: NativeFn,
}

/// A complete native class definition.
#[derive(Debug)]
pub struct NativeClass {
    /// Internal name.
    pub name: &'static str,
    /// Superclass internal name (`None` only for `java/lang/Object`).
    pub super_name: Option<&'static str>,
    /// Superinterface internal names.
    pub interfaces: &'static [&'static str],
    /// Class access flags.
    pub access_flags: u16,
    /// Fields.
    pub fields: &'static [NativeFieldDef],
    /// Methods.
    pub methods: &'static [NativeMethodDef],
    /// Initialization hook.
    pub clinit: Option<NativeClinit>,
}

/// The component type of an array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrayComponent {
    /// `boolean`.
    Boolean,
    /// `byte`.
    Byte,
    /// `char`.
    Char,
    /// `short`.
    Short,
    /// `int`.
    Int,
    /// `long`.
    Long,
    /// `float`.
    Float,
    /// `double`.
    Double,
    /// A reference type.
    Reference,
}

impl ArrayComponent {
    /// The component for a descriptor's element type.
    #[must_use]
    pub fn from_field_type(ty: &FieldType) -> Option<Self> {
        Some(match ty {
            FieldType::Base(base) => match base {
                oxjvm_classfile::BaseType::Boolean => Self::Boolean,
                oxjvm_classfile::BaseType::Byte => Self::Byte,
                oxjvm_classfile::BaseType::Char => Self::Char,
                oxjvm_classfile::BaseType::Short => Self::Short,
                oxjvm_classfile::BaseType::Int => Self::Int,
                oxjvm_classfile::BaseType::Long => Self::Long,
                oxjvm_classfile::BaseType::Float => Self::Float,
                oxjvm_classfile::BaseType::Double => Self::Double,
                oxjvm_classfile::BaseType::Void => return None,
            },
            FieldType::Object(_) | FieldType::Array(_) => Self::Reference,
        })
    }

    /// The descriptor character of the component.
    #[must_use]
    pub const fn descriptor(&self) -> &'static str {
        match self {
            Self::Boolean => "Z",
            Self::Byte => "B",
            Self::Char => "C",
            Self::Short => "S",
            Self::Int => "I",
            Self::Long => "J",
            Self::Float => "F",
            Self::Double => "D",
            Self::Reference => "Ljava/lang/Object;",
        }
    }

    /// The `newarray` atype code, or `None` for references.
    #[must_use]
    pub const fn atype(&self) -> Option<u8> {
        Some(match self {
            Self::Boolean => 4,
            Self::Byte => 8,
            Self::Char => 5,
            Self::Short => 9,
            Self::Int => 10,
            Self::Long => 11,
            Self::Float => 6,
            Self::Double => 7,
            Self::Reference => return None,
        })
    }

    /// The component for a `newarray` atype code.
    #[must_use]
    pub const fn from_atype(atype: u8) -> Option<Self> {
        Some(match atype {
            4 => Self::Boolean,
            5 => Self::Char,
            6 => Self::Float,
            7 => Self::Double,
            8 => Self::Byte,
            9 => Self::Short,
            10 => Self::Int,
            11 => Self::Long,
            _ => return None,
        })
    }
}

/// A fully linked runtime class.
#[derive(Debug)]
pub struct Class {
    /// Class id.
    pub id: ClassId,
    /// Internal name (`java/lang/String`), or an array descriptor (`[I`).
    pub name: String,
    /// Superclass.
    pub super_class: Option<ClassId>,
    /// Direct superinterfaces.
    pub interfaces: Vec<ClassId>,
    /// Access flags.
    pub access_flags: u16,
    /// Initialization state.
    pub state: ClassState,
    /// The class file's constant pool (empty for native/synthetic classes).
    pub constant_pool: ConstantPool,
    /// Declared fields (static and instance).
    pub fields: Vec<Field>,
    /// Declared methods.
    pub methods: Vec<Method>,
    /// Storage for static fields, indexed by [`Field::slot`].
    pub static_values: Vec<Value>,
    /// Default values for instance slots, indexed by [`Field::slot`].
    pub instance_defaults: Vec<Value>,
    /// Number of instance field slots.
    pub instance_slots: u16,
    /// Source file name, when known.
    pub source_file: Option<String>,
    /// Bootstrap methods from the class file.
    pub bootstrap_methods: Vec<BootstrapMethod>,
    /// The runtime `Class` object, created lazily.
    pub class_object: Option<ObjectRef>,
    /// What kind of class this is.
    pub kind: ClassKind,
    /// For array classes, the component type.
    pub component: Option<ArrayComponent>,
    /// The component class id for reference arrays.
    pub component_class: Option<ClassId>,
    /// For primitives, the base type.
    pub primitive: Option<oxjvm_classfile::BaseType>,
    /// The native definition, if this class came from the registry.
    pub native_definition: Option<&'static NativeClass>,
    /// The error thrown while initializing, if initialization failed.
    pub initialization_error: Option<ObjectRef>,
    /// Nest host, from the `NestHost` attribute.
    pub nest_host: Option<ClassId>,
    /// Permitted subclasses, from `PermittedSubclasses`.
    pub permitted_subclasses: Vec<ClassId>,
    /// Whether the class has an `ACC_ENUM` flag.
    pub is_enum: bool,
    /// Whether verification has run.
    pub verified: bool,
}

impl Class {
    /// Whether the class is an interface (or annotation).
    #[must_use]
    pub const fn is_interface(&self) -> bool {
        self.access_flags & oxjvm_classfile::flags::ACC_INTERFACE != 0
    }

    /// Whether the class is abstract.
    #[must_use]
    pub const fn is_abstract(&self) -> bool {
        self.access_flags & oxjvm_classfile::flags::ACC_ABSTRACT != 0
    }

    /// Whether this class is a subtype of (or equal to) `other`.
    #[must_use]
    pub fn is_subtype_of(&self, table: &ClassTable, other: ClassId) -> bool {
        if self.id == other {
            return true;
        }
        let target = table.get(other);
        if target.kind == ClassKind::Array {
            // Arrays are assignable only to Object, Cloneable, and Serializable.
            return target.name == "java/lang/Object"
                || target.name == "java/lang/Cloneable"
                || target.name == "java/io/Serializable";
        }
        if self.kind == ClassKind::Primitive {
            return false;
        }
        if let Some(parent) = self.super_class {
            if table.get(parent).is_subtype_of(table, other) {
                return true;
            }
        }
        for interface in &self.interfaces {
            if table.get(*interface).is_subtype_of(table, other) {
                return true;
            }
        }
        false
    }
}

/// The VM's class table.
#[derive(Debug)]
pub struct ClassTable {
    classes: Vec<Class>,
    index: BTreeMap<String, ClassId>,
}

impl ClassTable {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            classes: Vec::new(),
            index: BTreeMap::new(),
        }
    }

    /// Add a class, returning its id. The name must not already be registered.
    pub fn insert(&mut self, class: Class) -> ClassId {
        let id = ClassId(self.classes.len() as u32);
        self.index.insert(class.name.clone(), id);
        self.classes.push(class);
        id
    }

    /// Resolve an internal name to an id.
    #[must_use]
    pub fn by_name(&self, name: &str) -> Option<ClassId> {
        self.index.get(name).copied()
    }

    /// Borrow a class.
    ///
    /// # Panics
    ///
    /// Panics on an id that was not issued by this table.
    #[must_use]
    pub fn get(&self, id: ClassId) -> &Class {
        &self.classes[id.0 as usize]
    }

    /// Mutably borrow a class.
    ///
    /// # Panics
    ///
    /// Panics on an id that was not issued by this table.
    pub fn get_mut(&mut self, id: ClassId) -> &mut Class {
        &mut self.classes[id.0 as usize]
    }

    /// All classes.
    #[must_use]
    pub fn all(&self) -> &[Class] {
        &self.classes
    }

    /// Number of loaded classes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.classes.len()
    }

    /// Whether no classes are loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.classes.is_empty()
    }

    /// Find a declared method by name and descriptor, without walking the hierarchy.
    #[must_use]
    pub fn find_declared_method(
        &self,
        class: ClassId,
        name: &str,
        descriptor: &str,
    ) -> Option<u32> {
        self.get(class)
            .methods
            .iter()
            .position(|m| m.name == name && m.descriptor == descriptor)
            .map(|index| index as u32)
    }

    /// Find a declared field by name and descriptor.
    #[must_use]
    pub fn find_declared_field(&self, class: ClassId, name: &str, descriptor: &str) -> Option<u32> {
        self.get(class)
            .fields
            .iter()
            .position(|f| f.name == name && f.descriptor == descriptor)
            .map(|index| index as u32)
    }
}

impl Default for ClassTable {
    fn default() -> Self {
        Self::new()
    }
}

/// One `BootstrapMethods` entry, resolved to the method-handle and arguments it names.
#[derive(Debug, Clone)]
pub enum BootstrapMethod {
    /// A method handle plus its static arguments, still in constant-pool form.
    Unresolved {
        /// The `CONSTANT_MethodHandle` index.
        method_ref: u16,
        /// The argument constant-pool indices.
        arguments: Vec<u16>,
    },
}

/// The load status of a class, for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadMode {
    /// Normal loading through the class path.
    Normal,
    /// The class was synthesized (arrays, primitives, hidden lambda classes).
    Synthetic,
}

/// A convenience alias for the per-class error context used by verification.
pub type VerificationContext = String;

/// Marker: the host is only consulted for class bytes through the VM, never directly by classes.
pub trait ClassBytesHost: Host {}
