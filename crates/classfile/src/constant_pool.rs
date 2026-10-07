//! The constant pool (JVMS 4.4).
//!
//! [`CpInfo`] is a 1:1 model of the seventeen `CONSTANT_*` structures. `Float` and `Double` keep
//! their raw IEEE-754 bit patterns so that NaN payloads and signed zeros survive a round-trip
//! byte-for-byte; use [`CpInfo::as_f32`] / [`CpInfo::as_f64`] for the arithmetic value.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::error::ParseError;

/// One entry of the constant pool.
#[derive(Debug, Clone, PartialEq)]
pub enum CpInfo {
    /// `CONSTANT_Utf8` (4.4.7): a modified-UTF-8 string.
    Utf8(String),
    /// `CONSTANT_Integer` (4.4.4).
    Integer(i32),
    /// `CONSTANT_Float` (4.4.4), stored as raw bits.
    Float(u32),
    /// `CONSTANT_Long` (4.4.5); occupies two pool slots.
    Long(i64),
    /// `CONSTANT_Double` (4.4.5), stored as raw bits; occupies two pool slots.
    Double(u64),
    /// `CONSTANT_Class` (4.4.1).
    Class(u16),
    /// `CONSTANT_String` (4.4.3).
    String(u16),
    /// `CONSTANT_Fieldref` (4.4.2).
    Fieldref {
        /// Constant pool index of the `CONSTANT_Class`.
        class: u16,
        /// Constant pool index of the `CONSTANT_NameAndType`.
        name_and_type: u16,
    },
    /// `CONSTANT_Methodref` (4.4.2).
    Methodref {
        /// Constant pool index of the `CONSTANT_Class`.
        class: u16,
        /// Constant pool index of the `CONSTANT_NameAndType`.
        name_and_type: u16,
    },
    /// `CONSTANT_InterfaceMethodref` (4.4.2).
    InterfaceMethodref {
        /// Constant pool index of the `CONSTANT_Class`.
        class: u16,
        /// Constant pool index of the `CONSTANT_NameAndType`.
        name_and_type: u16,
    },
    /// `CONSTANT_NameAndType` (4.4.6).
    NameAndType {
        /// Constant pool index of the name `CONSTANT_Utf8`.
        name: u16,
        /// Constant pool index of the descriptor `CONSTANT_Utf8`.
        descriptor: u16,
    },
    /// `CONSTANT_MethodHandle` (4.4.8).
    MethodHandle {
        /// Reference kind, 1..=9 (JVMS 5.4.3.5).
        reference_kind: u8,
        /// Constant pool index of the reference.
        reference_index: u16,
    },
    /// `CONSTANT_MethodType` (4.4.9).
    MethodType(u16),
    /// `CONSTANT_Dynamic` (4.4.10).
    Dynamic {
        /// Index into the `BootstrapMethods` attribute.
        bootstrap_method_attr_index: u16,
        /// Constant pool index of the `CONSTANT_NameAndType`.
        name_and_type: u16,
    },
    /// `CONSTANT_InvokeDynamic` (4.4.10).
    InvokeDynamic {
        /// Index into the `BootstrapMethods` attribute.
        bootstrap_method_attr_index: u16,
        /// Constant pool index of the `CONSTANT_NameAndType`.
        name_and_type: u16,
    },
    /// `CONSTANT_Module` (4.4.11).
    Module(u16),
    /// `CONSTANT_Package` (4.4.12).
    Package(u16),
}

impl CpInfo {
    /// The pool tag byte for this entry.
    #[must_use]
    pub const fn tag(&self) -> u8 {
        match self {
            Self::Utf8(_) => 1,
            Self::Integer(_) => 3,
            Self::Float(_) => 4,
            Self::Long(_) => 5,
            Self::Double(_) => 6,
            Self::Class(_) => 7,
            Self::String(_) => 8,
            Self::Fieldref { .. } => 9,
            Self::Methodref { .. } => 10,
            Self::InterfaceMethodref { .. } => 11,
            Self::NameAndType { .. } => 12,
            Self::MethodHandle { .. } => 15,
            Self::MethodType(_) => 16,
            Self::Dynamic { .. } => 17,
            Self::InvokeDynamic { .. } => 18,
            Self::Module(_) => 19,
            Self::Package(_) => 20,
        }
    }

    /// Number of pool slots occupied (two for `Long` and `Double`, one otherwise).
    #[must_use]
    pub const fn slots(&self) -> u16 {
        match self {
            Self::Long(_) | Self::Double(_) => 2,
            _ => 1,
        }
    }

    /// The `CONSTANT_Float` value as `f32`.
    #[must_use]
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Self::Float(bits) => Some(f32::from_bits(*bits)),
            _ => None,
        }
    }

    /// The `CONSTANT_Double` value as `f64`.
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Double(bits) => Some(f64::from_bits(*bits)),
            _ => None,
        }
    }

    /// Whether this is a loadable constant (JVMS 4.4: Integer, Float, Long, Double, String,
    /// Class, MethodType, MethodHandle, Dynamic).
    #[must_use]
    pub const fn is_loadable(&self) -> bool {
        matches!(
            self,
            Self::Integer(_)
                | Self::Float(_)
                | Self::Long(_)
                | Self::Double(_)
                | Self::String(_)
                | Self::Class(_)
                | Self::MethodType(_)
                | Self::MethodHandle { .. }
                | Self::Dynamic { .. }
        )
    }

    /// Short human-readable kind name, for diagnostics.
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::Utf8(_) => "Utf8",
            Self::Integer(_) => "Integer",
            Self::Float(_) => "Float",
            Self::Long(_) => "Long",
            Self::Double(_) => "Double",
            Self::Class(_) => "Class",
            Self::String(_) => "String",
            Self::Fieldref { .. } => "Fieldref",
            Self::Methodref { .. } => "Methodref",
            Self::InterfaceMethodref { .. } => "InterfaceMethodref",
            Self::NameAndType { .. } => "NameAndType",
            Self::MethodHandle { .. } => "MethodHandle",
            Self::MethodType(_) => "MethodType",
            Self::Dynamic { .. } => "Dynamic",
            Self::InvokeDynamic { .. } => "InvokeDynamic",
            Self::Module(_) => "Module",
            Self::Package(_) => "Package",
        }
    }
}

/// A slot of the constant pool: either an entry or the phantom second slot of a `Long`/`Double`.
#[derive(Debug, Clone, PartialEq)]
enum Slot {
    Entry(CpInfo),
    Phantom,
}

/// The constant pool of one class file.
///
/// `constant_pool_count` is always `entries.len()` on write; index 0 is a permanently unusable
/// slot, exactly as JVMS 4.1 specifies.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConstantPool {
    slots: Vec<Slot>,
    /// Interned `Utf8` strings for O(log n) deduplication while building.
    utf8_index: BTreeMap<String, u16>,
}

impl ConstantPool {
    /// An empty pool (just the unusable slot 0).
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: alloc::vec![Slot::Phantom],
            utf8_index: BTreeMap::new(),
        }
    }

    /// Number of slots, i.e. the `constant_pool_count` written to disk.
    #[must_use]
    pub fn count(&self) -> u16 {
        self.slots.len() as u16
    }

    /// Resolve a pool index.
    ///
    /// # Errors
    ///
    /// Fails when `index` is zero, out of range, or names a phantom slot.
    pub fn get(&self, index: u16) -> Result<&CpInfo, ParseError> {
        self.slots
            .get(usize::from(index))
            .and_then(|slot| match slot {
                Slot::Entry(info) => Some(info),
                Slot::Phantom => None,
            })
            .ok_or(ParseError::BadConstantPoolIndex {
                index,
                expected: "an occupied constant pool entry",
            })
    }

    /// Resolve a pool index to a `CONSTANT_Utf8` string.
    ///
    /// # Errors
    ///
    /// Fails when the index does not name a `CONSTANT_Utf8`.
    pub fn utf8(&self, index: u16) -> Result<&str, ParseError> {
        match self.get(index)? {
            CpInfo::Utf8(text) => Ok(text),
            _ => Err(ParseError::BadConstantPoolIndex {
                index,
                expected: "CONSTANT_Utf8",
            }),
        }
    }

    /// Resolve a `CONSTANT_Class` index to the internal class name it names.
    ///
    /// # Errors
    ///
    /// Fails when the index is not a `Class` whose name is a valid `Utf8`.
    pub fn class_name(&self, index: u16) -> Result<&str, ParseError> {
        match self.get(index)? {
            CpInfo::Class(name) => self.utf8(*name),
            _ => Err(ParseError::BadConstantPoolIndex {
                index,
                expected: "CONSTANT_Class",
            }),
        }
    }

    /// Resolve a `CONSTANT_NameAndType` index to `(name, descriptor)`.
    ///
    /// # Errors
    ///
    /// Fails when the index is not a `NameAndType` of two `Utf8` entries.
    pub fn name_and_type(&self, index: u16) -> Result<(&str, &str), ParseError> {
        match self.get(index)? {
            CpInfo::NameAndType { name, descriptor } => {
                Ok((self.utf8(*name)?, self.utf8(*descriptor)?))
            }
            _ => Err(ParseError::BadConstantPoolIndex {
                index,
                expected: "CONSTANT_NameAndType",
            }),
        }
    }

    /// Append an entry, returning its index. `Long` and `Double` reserve their phantom slot.
    pub fn push(&mut self, info: CpInfo) -> u16 {
        let index = self.count();
        if let CpInfo::Utf8(text) = &info {
            self.utf8_index.insert(text.clone(), index);
        }
        let slots = info.slots();
        self.slots.push(Slot::Entry(info));
        if slots == 2 {
            self.slots.push(Slot::Phantom);
        }
        index
    }

    /// Intern a `CONSTANT_Utf8`, returning the existing index when already present.
    pub fn intern_utf8(&mut self, text: &str) -> u16 {
        if let Some(&index) = self.utf8_index.get(text) {
            return index;
        }
        self.push(CpInfo::Utf8(text.to_string()))
    }

    /// Append a `CONSTANT_Class`, returning its index.
    pub fn push_class(&mut self, internal_name: &str) -> u16 {
        let name = self.intern_utf8(internal_name);
        self.push(CpInfo::Class(name))
    }

    /// Append a `CONSTANT_String` wrapping `text`, returning its index.
    pub fn push_string(&mut self, text: &str) -> u16 {
        let utf8 = self.intern_utf8(text);
        self.push(CpInfo::String(utf8))
    }

    /// Append a `CONSTANT_NameAndType`, returning its index.
    pub fn push_name_and_type(&mut self, name: &str, descriptor: &str) -> u16 {
        let name = self.intern_utf8(name);
        let descriptor = self.intern_utf8(descriptor);
        self.push(CpInfo::NameAndType { name, descriptor })
    }

    /// Parse a pool from a cursor positioned at `constant_pool_count`.
    pub(crate) fn read(reader: &mut crate::bytes::Reader<'_>) -> Result<Self, ParseError> {
        let count = reader.u2("constant_pool_count")?;
        if count == 0 {
            return Err(ParseError::EmptyConstantPool);
        }
        let mut slots = Vec::with_capacity(usize::from(count));
        slots.push(Slot::Phantom);
        let mut utf8_index = BTreeMap::new();
        let mut index = 1u16;
        while index < count {
            let tag = reader.u1("constant pool tag")?;
            let info = match tag {
                1 => {
                    let len = usize::from(reader.u2("Utf8 length")?);
                    let bytes = reader.bytes(len, "Utf8 bytes")?;
                    CpInfo::Utf8(crate::mutf8::decode(bytes, index)?)
                }
                3 => CpInfo::Integer(reader.i4("Integer")?),
                4 => CpInfo::Float(reader.u4("Float")?),
                5 => CpInfo::Long(reader.i8("Long")?),
                6 => CpInfo::Double(reader.u8("Double")?),
                7 => CpInfo::Class(reader.u2("Class name_index")?),
                8 => CpInfo::String(reader.u2("String string_index")?),
                9 => CpInfo::Fieldref {
                    class: reader.u2("Fieldref class_index")?,
                    name_and_type: reader.u2("Fieldref name_and_type_index")?,
                },
                10 => CpInfo::Methodref {
                    class: reader.u2("Methodref class_index")?,
                    name_and_type: reader.u2("Methodref name_and_type_index")?,
                },
                11 => CpInfo::InterfaceMethodref {
                    class: reader.u2("InterfaceMethodref class_index")?,
                    name_and_type: reader.u2("InterfaceMethodref name_and_type_index")?,
                },
                12 => CpInfo::NameAndType {
                    name: reader.u2("NameAndType name_index")?,
                    descriptor: reader.u2("NameAndType descriptor_index")?,
                },
                15 => CpInfo::MethodHandle {
                    reference_kind: reader.u1("MethodHandle reference_kind")?,
                    reference_index: reader.u2("MethodHandle reference_index")?,
                },
                16 => CpInfo::MethodType(reader.u2("MethodType descriptor_index")?),
                17 => CpInfo::Dynamic {
                    bootstrap_method_attr_index: reader
                        .u2("Dynamic bootstrap_method_attr_index")?,
                    name_and_type: reader.u2("Dynamic name_and_type_index")?,
                },
                18 => CpInfo::InvokeDynamic {
                    bootstrap_method_attr_index: reader
                        .u2("InvokeDynamic bootstrap_method_attr_index")?,
                    name_and_type: reader.u2("InvokeDynamic name_and_type_index")?,
                },
                19 => CpInfo::Module(reader.u2("Module name_index")?),
                20 => CpInfo::Package(reader.u2("Package name_index")?),
                other => return Err(ParseError::InvalidConstantTag { index, tag: other }),
            };
            if let CpInfo::Utf8(text) = &info {
                utf8_index.insert(text.clone(), index);
            }
            let reserved = info.slots();
            slots.push(Slot::Entry(info));
            if reserved == 2 {
                slots.push(Slot::Phantom);
                index += 1;
            }
            index += 1;
        }
        if index != count {
            return Err(ParseError::Other(
                "constant pool count does not match parsed entries",
            ));
        }
        Ok(Self { slots, utf8_index })
    }

    /// Serialize the pool, including the leading `constant_pool_count`.
    pub(crate) fn write(&self, writer: &mut crate::bytes::Writer) {
        writer.u2(self.count());
        for slot in &self.slots[1..] {
            let Slot::Entry(info) = slot else {
                continue;
            };
            writer.u1(info.tag());
            match info {
                CpInfo::Utf8(text) => {
                    let bytes = crate::mutf8::encode(text);
                    writer.u2(bytes.len() as u16);
                    writer.bytes(&bytes);
                }
                CpInfo::Integer(value) => writer.i4(*value),
                CpInfo::Float(bits) => writer.u4(*bits),
                CpInfo::Long(value) => writer.i8(*value),
                CpInfo::Double(bits) => writer.u8(*bits),
                CpInfo::Class(name) => writer.u2(*name),
                CpInfo::String(string) => writer.u2(*string),
                CpInfo::Fieldref {
                    class,
                    name_and_type,
                } => {
                    writer.u2(*class);
                    writer.u2(*name_and_type);
                }
                CpInfo::Methodref {
                    class,
                    name_and_type,
                } => {
                    writer.u2(*class);
                    writer.u2(*name_and_type);
                }
                CpInfo::InterfaceMethodref {
                    class,
                    name_and_type,
                } => {
                    writer.u2(*class);
                    writer.u2(*name_and_type);
                }
                CpInfo::NameAndType { name, descriptor } => {
                    writer.u2(*name);
                    writer.u2(*descriptor);
                }
                CpInfo::MethodHandle {
                    reference_kind,
                    reference_index,
                } => {
                    writer.u1(*reference_kind);
                    writer.u2(*reference_index);
                }
                CpInfo::MethodType(descriptor) => writer.u2(*descriptor),
                CpInfo::Dynamic {
                    bootstrap_method_attr_index,
                    name_and_type,
                }
                | CpInfo::InvokeDynamic {
                    bootstrap_method_attr_index,
                    name_and_type,
                } => {
                    writer.u2(*bootstrap_method_attr_index);
                    writer.u2(*name_and_type);
                }
                CpInfo::Module(name) => writer.u2(*name),
                CpInfo::Package(name) => writer.u2(*name),
            }
        }
    }

    /// Iterate over `(index, entry)` pairs in pool order, skipping phantom slots.
    pub fn iter(&self) -> impl Iterator<Item = (u16, &CpInfo)> {
        self.slots
            .iter()
            .enumerate()
            .skip(1)
            .filter_map(|(index, slot)| match slot {
                Slot::Entry(info) => Some((index as u16, info)),
                Slot::Phantom => None,
            })
    }
}
