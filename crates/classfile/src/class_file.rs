//! The `ClassFile` structure itself (JVMS 4.1), with `field_info` and `method_info`.

use alloc::vec::Vec;

use crate::attribute::{Attribute, AttributeData, CodeAttribute, read_attribute, write_attribute};
use crate::bytes::{Reader, Writer};
use crate::constant_pool::ConstantPool;
use crate::error::ParseError;

/// One `field_info` (JVMS 4.5).
#[derive(Debug, Clone, PartialEq)]
pub struct FieldInfo {
    /// Access flags.
    pub access_flags: u16,
    /// `name_index`.
    pub name_index: u16,
    /// `descriptor_index`.
    pub descriptor_index: u16,
    /// Attributes.
    pub attributes: Vec<Attribute>,
}

/// One `method_info` (JVMS 4.6).
#[derive(Debug, Clone, PartialEq)]
pub struct MethodInfo {
    /// Access flags.
    pub access_flags: u16,
    /// `name_index`.
    pub name_index: u16,
    /// `descriptor_index`.
    pub descriptor_index: u16,
    /// Attributes.
    pub attributes: Vec<Attribute>,
}

/// A complete JVM class file.
///
/// Reading is byte-for-byte reversible: `ClassFile::read(&bytes).write() == bytes` for every class
/// file that the parser accepts.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassFile {
    /// `minor_version`.
    pub minor_version: u16,
    /// `major_version`.
    pub major_version: u16,
    /// `constant_pool`.
    pub constant_pool: ConstantPool,
    /// `access_flags`.
    pub access_flags: u16,
    /// `this_class`.
    pub this_class: u16,
    /// `super_class` (zero only for `java/lang/Object`).
    pub super_class: u16,
    /// `interfaces`.
    pub interfaces: Vec<u16>,
    /// `fields`.
    pub fields: Vec<FieldInfo>,
    /// `methods`.
    pub methods: Vec<MethodInfo>,
    /// `attributes`.
    pub attributes: Vec<Attribute>,
}

/// The class-file magic number.
pub const MAGIC: u32 = 0xCAFE_BABE;

impl ClassFile {
    /// Parse a class file from a byte slice.
    ///
    /// # Errors
    ///
    /// Returns a [`ParseError`] describing the first malformed structure.
    pub fn read(bytes: &[u8]) -> Result<Self, ParseError> {
        let mut reader = Reader::new(bytes);
        let magic = reader.u4("magic")?;
        if magic != MAGIC {
            return Err(ParseError::BadMagic(magic));
        }
        let minor_version = reader.u2("minor_version")?;
        let major_version = reader.u2("major_version")?;
        let constant_pool = ConstantPool::read(&mut reader)?;
        let access_flags = reader.u2("access_flags")?;
        let this_class = reader.u2("this_class")?;
        let super_class = reader.u2("super_class")?;
        let interface_count = reader.u2("interfaces_count")?;
        let mut interfaces = Vec::with_capacity(usize::from(interface_count));
        for _ in 0..interface_count {
            interfaces.push(reader.u2("interface")?);
        }
        let field_count = reader.u2("fields_count")?;
        let mut fields = Vec::with_capacity(usize::from(field_count));
        for _ in 0..field_count {
            fields.push(read_field(&mut reader, &constant_pool)?);
        }
        let method_count = reader.u2("methods_count")?;
        let mut methods = Vec::with_capacity(usize::from(method_count));
        for _ in 0..method_count {
            methods.push(read_method(&mut reader, &constant_pool)?);
        }
        let attribute_count = reader.u2("attributes_count")?;
        let mut attributes = Vec::with_capacity(usize::from(attribute_count));
        for _ in 0..attribute_count {
            attributes.push(read_attribute(&mut reader, &constant_pool)?);
        }
        if !reader.is_empty() {
            return Err(ParseError::Other("trailing bytes after class file"));
        }
        Ok(Self {
            minor_version,
            major_version,
            constant_pool,
            access_flags,
            this_class,
            super_class,
            interfaces,
            fields,
            methods,
            attributes,
        })
    }

    /// Serialize the class file. Counts and lengths are derived from the model, so an edited class
    /// file still writes a self-consistent image.
    #[must_use]
    pub fn write(&self) -> Vec<u8> {
        let mut writer = Writer::with_capacity(1024);
        writer.u4(MAGIC);
        writer.u2(self.minor_version);
        writer.u2(self.major_version);
        self.constant_pool.write(&mut writer);
        writer.u2(self.access_flags);
        writer.u2(self.this_class);
        writer.u2(self.super_class);
        writer.u2(self.interfaces.len() as u16);
        for interface in &self.interfaces {
            writer.u2(*interface);
        }
        writer.u2(self.fields.len() as u16);
        for field in &self.fields {
            write_member(
                field.access_flags,
                field.name_index,
                field.descriptor_index,
                &field.attributes,
                &mut writer,
            );
        }
        writer.u2(self.methods.len() as u16);
        for method in &self.methods {
            write_member(
                method.access_flags,
                method.name_index,
                method.descriptor_index,
                &method.attributes,
                &mut writer,
            );
        }
        writer.u2(self.attributes.len() as u16);
        for attribute in &self.attributes {
            write_attribute(attribute, &mut writer);
        }
        writer.into_vec()
    }

    /// The `(major, minor)` version pair, e.g. `(52, 0)` for Java 8.
    #[must_use]
    pub const fn version(&self) -> (u16, u16) {
        (self.major_version, self.minor_version)
    }

    /// The internal name of this class (`java/lang/String`).
    ///
    /// # Errors
    ///
    /// Fails when `this_class` does not resolve to a class name.
    pub fn this_name(&self) -> Result<&str, ParseError> {
        self.constant_pool.class_name(self.this_class)
    }

    /// The internal name of the superclass, or `None` for `java/lang/Object`.
    ///
    /// # Errors
    ///
    /// Fails when a nonzero `super_class` does not resolve to a class name.
    pub fn super_name(&self) -> Result<Option<&str>, ParseError> {
        if self.super_class == 0 {
            Ok(None)
        } else {
            Ok(Some(self.constant_pool.class_name(self.super_class)?))
        }
    }

    /// The internal names of the direct superinterfaces.
    ///
    /// # Errors
    ///
    /// Fails when an interface index does not resolve.
    pub fn interface_names(&self) -> Result<Vec<&str>, ParseError> {
        self.interfaces
            .iter()
            .map(|index| self.constant_pool.class_name(*index))
            .collect()
    }

    /// Look up a field by name and descriptor.
    #[must_use]
    pub fn field(&self, name: &str, descriptor: &str) -> Option<&FieldInfo> {
        self.fields.iter().find(|field| {
            self.constant_pool.utf8(field.name_index).ok() == Some(name)
                && self.constant_pool.utf8(field.descriptor_index).ok() == Some(descriptor)
        })
    }

    /// Look up a method by name and descriptor.
    #[must_use]
    pub fn method(&self, name: &str, descriptor: &str) -> Option<&MethodInfo> {
        self.methods.iter().find(|method| {
            self.constant_pool.utf8(method.name_index).ok() == Some(name)
                && self.constant_pool.utf8(method.descriptor_index).ok() == Some(descriptor)
        })
    }

    /// The `Code` attribute of the first method matching `name`/`descriptor`.
    #[must_use]
    pub fn code_of(&self, name: &str, descriptor: &str) -> Option<&CodeAttribute> {
        self.method(name, descriptor)?
            .attributes
            .iter()
            .find_map(|a| match &a.data {
                AttributeData::Code(code) => Some(code),
                _ => None,
            })
    }

    /// Find the first class-level attribute whose body is `expected`.
    #[must_use]
    pub fn find_attribute(&self, name: &str) -> Option<&Attribute> {
        self.attributes
            .iter()
            .find(|a| self.attribute_name(a) == Some(name))
    }

    /// The name of an attribute, resolved through the pool.
    #[must_use]
    pub fn attribute_name<'a>(&'a self, attribute: &'a Attribute) -> Option<&'a str> {
        self.constant_pool.utf8(attribute.name_index).ok()
    }

    /// Whether this class is an interface.
    #[must_use]
    pub const fn is_interface(&self) -> bool {
        self.access_flags & crate::flags::ACC_INTERFACE != 0
    }

    /// Whether this class is `java/lang/Object`.
    #[must_use]
    pub fn is_object(&self) -> bool {
        self.this_name().ok() == Some("java/lang/Object")
    }

    /// Name an attribute body with its standard name, interning the `Utf8` entry in this class's
    /// pool and returning the complete attribute. Unknown bodies must carry their name index.
    ///
    /// # Panics
    ///
    /// Panics when `data` has no standard name; such attributes must be built manually.
    pub fn attribute(&mut self, data: AttributeData) -> Attribute {
        let name = data
            .standard_name()
            .expect("attribute body without a standard name must be built manually");
        let name_index = self.constant_pool.intern_utf8(name);
        Attribute { name_index, data }
    }
}

fn read_field(reader: &mut Reader<'_>, cp: &ConstantPool) -> Result<FieldInfo, ParseError> {
    let access_flags = reader.u2("field access_flags")?;
    let name_index = reader.u2("field name_index")?;
    let descriptor_index = reader.u2("field descriptor_index")?;
    let count = reader.u2("field attributes_count")?;
    let mut attributes = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        attributes.push(read_attribute(reader, cp)?);
    }
    Ok(FieldInfo {
        access_flags,
        name_index,
        descriptor_index,
        attributes,
    })
}

fn read_method(reader: &mut Reader<'_>, cp: &ConstantPool) -> Result<MethodInfo, ParseError> {
    let access_flags = reader.u2("method access_flags")?;
    let name_index = reader.u2("method name_index")?;
    let descriptor_index = reader.u2("method descriptor_index")?;
    let count = reader.u2("method attributes_count")?;
    let mut attributes = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        attributes.push(read_attribute(reader, cp)?);
    }
    Ok(MethodInfo {
        access_flags,
        name_index,
        descriptor_index,
        attributes,
    })
}

fn write_member(
    access_flags: u16,
    name_index: u16,
    descriptor_index: u16,
    attributes: &[Attribute],
    writer: &mut Writer,
) {
    writer.u2(access_flags);
    writer.u2(name_index);
    writer.u2(descriptor_index);
    writer.u2(attributes.len() as u16);
    for attribute in attributes {
        write_attribute(attribute, writer);
    }
}
