#![no_std]
#![deny(unsafe_code)]
//! `oxjvm-classfile`: a complete, byte-exact model of the JVM `.class` file format (JVMS ch. 4).
//!
//! This crate is deliberately dependency-free, `no_std` + `alloc`, and free of I/O: it turns
//! bytes into a structural model and back. The runtime (`oxjvm-vm`) builds on this model; the CLI
//! uses it for disassembly and inspection.
//!
//! ```
//! # let bytes: Vec<u8> = Vec::new();
//! # if let Ok(class) = oxjvm_classfile::ClassFile::read(&bytes) {
//! let text = class.this_name().unwrap().to_string();
//! let reparsed = class.write();
//! assert_eq!(reparsed, bytes); // byte-exact round-trip
//! # }
//! ```

extern crate alloc;

pub mod attribute;
pub mod bytes;
pub mod class_file;
pub mod constant_pool;
pub mod descriptor;
pub mod error;
pub mod flags;
pub mod mutf8;
pub mod opcode;

pub use attribute::{
    Annotation, Attribute, AttributeData, BootstrapMethod, CodeAttribute, ElementPair,
    ElementValue, EnclosingMethod, ExceptionHandler, InnerClass, LineNumber, LocalVariable,
    LocalVariableType, MethodParameter, ModuleAttribute, RecordComponent, StackMapFrame,
    TypeAnnotation, VerificationType,
};
pub use class_file::{ClassFile, FieldInfo, MAGIC, MethodInfo};
pub use constant_pool::{ConstantPool, CpInfo};
pub use descriptor::{BaseType, FieldType, MethodDescriptor};
pub use error::ParseError;
pub use opcode::{DecodedInstruction, decode};
