//! Class-file attributes (JVMS 4.7).
//!
//! Every standard attribute of JVMS 4.7 is modelled structurally; attributes with an unknown name
//! are preserved verbatim as [`AttributeData::Raw`] so that reading and writing a class file is
//! byte-exact even in the presence of vendor extensions.

use alloc::vec::Vec;

use crate::bytes::{Reader, Writer};
use crate::constant_pool::ConstantPool;
use crate::error::ParseError;

/// One attribute: the constant-pool index of its name plus its body.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    /// Index of the `CONSTANT_Utf8` naming this attribute.
    pub name_index: u16,
    /// The parsed body.
    pub data: AttributeData,
}

impl Attribute {
    /// Build an attribute around a body, leaving the name index unset.
    ///
    /// [`crate::ClassFile::intern_attribute_name`] fills the index in; the codec emits attributes
    /// exactly as stored, so a parsed attribute keeps its original name index.
    #[must_use]
    pub fn new(data: AttributeData) -> Self {
        Self {
            name_index: 0,
            data,
        }
    }
}

/// The body of one class-file attribute.
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeData {
    /// `ConstantValue` (4.7.2).
    ConstantValue(u16),
    /// `Code` (4.7.3).
    Code(CodeAttribute),
    /// `StackMapTable` (4.7.4).
    StackMapTable(Vec<StackMapFrame>),
    /// `Exceptions` (4.7.5).
    Exceptions(Vec<u16>),
    /// `InnerClasses` (4.7.6).
    InnerClasses(Vec<InnerClass>),
    /// `EnclosingMethod` (4.7.7).
    EnclosingMethod(EnclosingMethod),
    /// `Synthetic` (4.7.8).
    Synthetic,
    /// `Signature` (4.7.9).
    Signature(u16),
    /// `SourceFile` (4.7.10).
    SourceFile(u16),
    /// `SourceDebugExtension` (4.7.11).
    SourceDebugExtension(Vec<u8>),
    /// `LineNumberTable` (4.7.12).
    LineNumberTable(Vec<LineNumber>),
    /// `LocalVariableTable` (4.7.13).
    LocalVariableTable(Vec<LocalVariable>),
    /// `LocalVariableTypeTable` (4.7.14).
    LocalVariableTypeTable(Vec<LocalVariableType>),
    /// `Deprecated` (4.7.15).
    Deprecated,
    /// `RuntimeVisibleAnnotations` (4.7.16).
    RuntimeVisibleAnnotations(Vec<Annotation>),
    /// `RuntimeInvisibleAnnotations` (4.7.16).
    RuntimeInvisibleAnnotations(Vec<Annotation>),
    /// `RuntimeVisibleParameterAnnotations` (4.7.17).
    RuntimeVisibleParameterAnnotations(Vec<Vec<Annotation>>),
    /// `RuntimeInvisibleParameterAnnotations` (4.7.17).
    RuntimeInvisibleParameterAnnotations(Vec<Vec<Annotation>>),
    /// `RuntimeVisibleTypeAnnotations` (4.7.20).
    RuntimeVisibleTypeAnnotations(Vec<TypeAnnotation>),
    /// `RuntimeInvisibleTypeAnnotations` (4.7.20).
    RuntimeInvisibleTypeAnnotations(Vec<TypeAnnotation>),
    /// `AnnotationDefault` (4.7.22).
    AnnotationDefault(ElementValue),
    /// `BootstrapMethods` (4.7.23).
    BootstrapMethods(Vec<BootstrapMethod>),
    /// `MethodParameters` (4.7.24).
    MethodParameters(Vec<MethodParameter>),
    /// `Module` (4.7.25).
    Module(ModuleAttribute),
    /// `ModulePackages` (4.7.26).
    ModulePackages(Vec<u16>),
    /// `ModuleMainClass` (4.7.27).
    ModuleMainClass(u16),
    /// `NestHost` (4.7.28).
    NestHost(u16),
    /// `NestMembers` (4.7.29).
    NestMembers(Vec<u16>),
    /// `Record` (4.7.30).
    Record(Vec<RecordComponent>),
    /// `PermittedSubclasses` (4.7.31).
    PermittedSubclasses(Vec<u16>),
    /// An unrecognized attribute, preserved verbatim.
    Raw(Vec<u8>),
}

impl AttributeData {
    /// The standard name for this body, if it has one.
    #[must_use]
    pub const fn standard_name(&self) -> Option<&'static str> {
        Some(match self {
            Self::ConstantValue(_) => "ConstantValue",
            Self::Code(_) => "Code",
            Self::StackMapTable(_) => "StackMapTable",
            Self::Exceptions(_) => "Exceptions",
            Self::InnerClasses(_) => "InnerClasses",
            Self::EnclosingMethod(_) => "EnclosingMethod",
            Self::Synthetic => "Synthetic",
            Self::Signature(_) => "Signature",
            Self::SourceFile(_) => "SourceFile",
            Self::SourceDebugExtension(_) => "SourceDebugExtension",
            Self::LineNumberTable(_) => "LineNumberTable",
            Self::LocalVariableTable(_) => "LocalVariableTable",
            Self::LocalVariableTypeTable(_) => "LocalVariableTypeTable",
            Self::Deprecated => "Deprecated",
            Self::RuntimeVisibleAnnotations(_) => "RuntimeVisibleAnnotations",
            Self::RuntimeInvisibleAnnotations(_) => "RuntimeInvisibleAnnotations",
            Self::RuntimeVisibleParameterAnnotations(_) => "RuntimeVisibleParameterAnnotations",
            Self::RuntimeInvisibleParameterAnnotations(_) => "RuntimeInvisibleParameterAnnotations",
            Self::RuntimeVisibleTypeAnnotations(_) => "RuntimeVisibleTypeAnnotations",
            Self::RuntimeInvisibleTypeAnnotations(_) => "RuntimeInvisibleTypeAnnotations",
            Self::AnnotationDefault(_) => "AnnotationDefault",
            Self::BootstrapMethods(_) => "BootstrapMethods",
            Self::MethodParameters(_) => "MethodParameters",
            Self::Module(_) => "Module",
            Self::ModulePackages(_) => "ModulePackages",
            Self::ModuleMainClass(_) => "ModuleMainClass",
            Self::NestHost(_) => "NestHost",
            Self::NestMembers(_) => "NestMembers",
            Self::Record(_) => "Record",
            Self::PermittedSubclasses(_) => "PermittedSubclasses",
            Self::Raw(_) => return None,
        })
    }
}

/// The `Code` attribute (4.7.3).
#[derive(Debug, Clone, PartialEq)]
pub struct CodeAttribute {
    /// Maximum operand-stack depth in slots.
    pub max_stack: u16,
    /// Number of local-variable slots.
    pub max_locals: u16,
    /// The bytecode.
    pub code: Vec<u8>,
    /// The exception handler table.
    pub exception_table: Vec<ExceptionHandler>,
    /// Attributes of this `Code` attribute.
    pub attributes: Vec<Attribute>,
}

/// One entry of a `Code` attribute's exception table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExceptionHandler {
    /// Start of the protected range (inclusive).
    pub start_pc: u16,
    /// End of the protected range (exclusive).
    pub end_pc: u16,
    /// Handler entry point.
    pub handler_pc: u16,
    /// Caught class, or 0 for `finally`/`any`.
    pub catch_type: u16,
}

/// `StackMapTable` verification type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationType {
    /// `Top`.
    Top,
    /// `Integer`.
    Integer,
    /// `Float`.
    Float,
    /// `Double`.
    Double,
    /// `Long`.
    Long,
    /// `Null`.
    Null,
    /// `UninitializedThis`.
    UninitializedThis,
    /// `Object(cpool_index)`.
    Object(u16),
    /// `Uninitialized(offset)`.
    Uninitialized(u16),
}

/// One `StackMapTable` frame (4.7.4).
#[derive(Debug, Clone, PartialEq)]
pub enum StackMapFrame {
    /// `same_frame`.
    Same {
        /// `offset_delta`.
        offset_delta: u16,
    },
    /// `same_locals_1_stack_item_frame`.
    SameLocals1 {
        /// `offset_delta`.
        offset_delta: u16,
        /// The single stack item type.
        stack: VerificationType,
    },
    /// `same_locals_1_stack_item_frame_extended`.
    SameLocals1Extended {
        /// `offset_delta`.
        offset_delta: u16,
        /// The single stack item type.
        stack: VerificationType,
    },
    /// `chop_frame`.
    Chop {
        /// `offset_delta`.
        offset_delta: u16,
        /// Number of locals chopped (1..=3).
        k: u8,
    },
    /// `same_frame_extended`.
    SameExtended {
        /// `offset_delta`.
        offset_delta: u16,
    },
    /// `append_frame`.
    Append {
        /// `offset_delta`.
        offset_delta: u16,
        /// Appended local types.
        locals: Vec<VerificationType>,
    },
    /// `full_frame`.
    Full {
        /// `offset_delta`.
        offset_delta: u16,
        /// All local types.
        locals: Vec<VerificationType>,
        /// All stack types.
        stack: Vec<VerificationType>,
    },
}

/// One `InnerClasses` entry (4.7.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InnerClass {
    /// `inner_class_info_index`.
    pub inner_class_info_index: u16,
    /// `outer_class_info_index`.
    pub outer_class_info_index: u16,
    /// `inner_name_index`.
    pub inner_name_index: u16,
    /// Access flags of the inner class.
    pub inner_class_access_flags: u16,
}

/// `EnclosingMethod` (4.7.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnclosingMethod {
    /// `class_index`.
    pub class_index: u16,
    /// `method_index` (0 when not enclosed by a method).
    pub method_index: u16,
}

/// One `LineNumberTable` entry (4.7.12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineNumber {
    /// Bytecode offset.
    pub start_pc: u16,
    /// Source line.
    pub line_number: u16,
}

/// One `LocalVariableTable` entry (4.7.13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalVariable {
    /// First bytecode offset of the scope.
    pub start_pc: u16,
    /// Length of the scope in bytes.
    pub length: u16,
    /// Variable name index.
    pub name_index: u16,
    /// Type descriptor index.
    pub descriptor_index: u16,
    /// Local slot.
    pub index: u16,
}

/// One `LocalVariableTypeTable` entry (4.7.14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalVariableType {
    /// First bytecode offset of the scope.
    pub start_pc: u16,
    /// Length of the scope in bytes.
    pub length: u16,
    /// Variable name index.
    pub name_index: u16,
    /// Generic signature index.
    pub signature_index: u16,
    /// Local slot.
    pub index: u16,
}

/// One annotation (4.7.16).
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// `type_index`.
    pub type_index: u16,
    /// Element-value pairs.
    pub elements: Vec<ElementPair>,
}

/// One element-value pair.
#[derive(Debug, Clone, PartialEq)]
pub struct ElementPair {
    /// `element_name_index`.
    pub name_index: u16,
    /// The value.
    pub value: ElementValue,
}

/// An element value (4.7.16.1).
#[derive(Debug, Clone, PartialEq)]
pub enum ElementValue {
    /// A constant pool index (`const_value_index`) and its original tag byte
    /// (`B`, `C`, `D`, `F`, `I`, `J`, `S`, `Z`, or `s`), kept for byte-exact round-trips.
    Constant {
        /// The tag byte from the class file.
        tag: u8,
        /// The constant pool index.
        index: u16,
    },
    /// An enum constant.
    Enum {
        /// Enum type descriptor index.
        type_name_index: u16,
        /// Constant name index.
        const_name_index: u16,
    },
    /// A class literal.
    Class(u16),
    /// A nested annotation.
    Annotation(Annotation),
    /// An array of values.
    Array(Vec<ElementValue>),
}

/// One `BootstrapMethods` entry (4.7.23).
#[derive(Debug, Clone, PartialEq)]
pub struct BootstrapMethod {
    /// `bootstrap_method_ref`.
    pub bootstrap_method_ref: u16,
    /// `bootstrap_arguments`.
    pub bootstrap_arguments: Vec<u16>,
}

/// One `MethodParameters` entry (4.7.24).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodParameter {
    /// `name_index` (0 when nameless).
    pub name_index: u16,
    /// `access_flags`.
    pub access_flags: u16,
}

/// `Module` (4.7.25).
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleAttribute {
    /// `module_name_index`.
    pub module_name_index: u16,
    /// `module_flags`.
    pub module_flags: u16,
    /// `module_version_index`.
    pub module_version_index: u16,
    /// `requires`.
    pub requires: Vec<ModuleRequires>,
    /// `exports`.
    pub exports: Vec<ModuleExports>,
    /// `opens`.
    pub opens: Vec<ModuleOpens>,
    /// `uses`.
    pub uses: Vec<u16>,
    /// `provides`.
    pub provides: Vec<ModuleProvides>,
}

/// One `requires` entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleRequires {
    /// `requires_index`.
    pub requires_index: u16,
    /// `requires_flags`.
    pub requires_flags: u16,
    /// `requires_version_index`.
    pub requires_version_index: u16,
}

/// One `exports` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleExports {
    /// `exports_index`.
    pub exports_index: u16,
    /// `exports_flags`.
    pub exports_flags: u16,
    /// `exports_to_index`.
    pub exports_to_index: Vec<u16>,
}

/// One `opens` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleOpens {
    /// `opens_index`.
    pub opens_index: u16,
    /// `opens_flags`.
    pub opens_flags: u16,
    /// `opens_to_index`.
    pub opens_to_index: Vec<u16>,
}

/// One `provides` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleProvides {
    /// `provides_index`.
    pub provides_index: u16,
    /// `provides_with_index`.
    pub provides_with_index: Vec<u16>,
}

/// One `Record` component (4.7.30).
#[derive(Debug, Clone, PartialEq)]
pub struct RecordComponent {
    /// `name_index`.
    pub name_index: u16,
    /// `descriptor_index`.
    pub descriptor_index: u16,
    /// Component attributes.
    pub attributes: Vec<Attribute>,
}

/// One `RuntimeVisibleTypeAnnotations` entry (4.7.20).
#[derive(Debug, Clone, PartialEq)]
pub struct TypeAnnotation {
    /// `target_type`.
    pub target_type: u8,
    /// Target-dependent payload.
    pub target_info: TargetInfo,
    /// `target_path`.
    pub target_path: Vec<TypePathEntry>,
    /// `type_index`.
    pub type_index: u16,
    /// Element-value pairs.
    pub elements: Vec<ElementPair>,
}

/// Target-dependent payload of a type annotation.
#[derive(Debug, Clone, PartialEq)]
pub enum TargetInfo {
    /// `type_parameter_target`.
    TypeParameter {
        /// `type_parameter_index`.
        index: u8,
    },
    /// `supertype_target`.
    Supertype {
        /// `supertype_index`.
        index: u16,
    },
    /// `type_parameter_bound_target`.
    TypeParameterBound {
        /// `type_parameter_index`.
        parameter_index: u8,
        /// `bound_index`.
        bound_index: u8,
    },
    /// An empty `target_info` (`empty_target`).
    Empty,
    /// `formal_parameter_target`.
    FormalParameter {
        /// `formal_parameter_index`.
        index: u8,
    },
    /// `throws_target`.
    Throws {
        /// `throws_type_index`.
        index: u16,
    },
    /// `localvar_target`.
    LocalVar {
        /// The table entries.
        table: Vec<LocalVarTargetEntry>,
    },
    /// `catch_target`.
    Catch {
        /// `exception_table_index`.
        index: u16,
    },
    /// `offset_target`.
    Offset {
        /// `offset`.
        offset: u16,
    },
    /// `type_argument_target`.
    TypeArgument {
        /// `offset`.
        offset: u16,
        /// `type_argument_index`.
        index: u8,
    },
}

/// One `localvar_target` table entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalVarTargetEntry {
    /// `start_pc`.
    pub start_pc: u16,
    /// `length`.
    pub length: u16,
    /// `index`.
    pub index: u16,
}

/// One `target_path` entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypePathEntry {
    /// `type_path_kind`.
    pub type_path_kind: u8,
    /// `type_argument_index`.
    pub type_argument_index: u8,
}

// ---------------------------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------------------------

pub(crate) fn read_attribute(
    reader: &mut Reader<'_>,
    cp: &ConstantPool,
) -> Result<Attribute, ParseError> {
    let name_index = reader.u2("attribute_name_index")?;
    let length = reader.u4("attribute_length")? as usize;
    let mut body = Reader::new(reader.bytes(length, "attribute body")?);
    let name = cp.utf8(name_index)?;
    let data = read_attribute_body(name, &mut body, cp)?;
    if !body.is_empty() {
        return Err(ParseError::Other("attribute body was not fully consumed"));
    }
    Ok(Attribute { name_index, data })
}

#[allow(clippy::match_same_arms)]
fn read_attribute_body(
    name: &str,
    body: &mut Reader<'_>,
    cp: &ConstantPool,
) -> Result<AttributeData, ParseError> {
    Ok(match name {
        "ConstantValue" => AttributeData::ConstantValue(body.u2("constantvalue_index")?),
        "Code" => AttributeData::Code(read_code(body, cp)?),
        "StackMapTable" => AttributeData::StackMapTable(read_stack_map_table(body)?),
        "Exceptions" => {
            let count = body.u2("number_of_exceptions")?;
            let mut table = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                table.push(body.u2("exception_index_table")?);
            }
            AttributeData::Exceptions(table)
        }
        "InnerClasses" => {
            let count = body.u2("number_of_classes")?;
            let mut classes = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                classes.push(InnerClass {
                    inner_class_info_index: body.u2("inner_class_info_index")?,
                    outer_class_info_index: body.u2("outer_class_info_index")?,
                    inner_name_index: body.u2("inner_name_index")?,
                    inner_class_access_flags: body.u2("inner_class_access_flags")?,
                });
            }
            AttributeData::InnerClasses(classes)
        }
        "EnclosingMethod" => AttributeData::EnclosingMethod(EnclosingMethod {
            class_index: body.u2("class_index")?,
            method_index: body.u2("method_index")?,
        }),
        "Synthetic" => AttributeData::Synthetic,
        "Signature" => AttributeData::Signature(body.u2("signature_index")?),
        "SourceFile" => AttributeData::SourceFile(body.u2("sourcefile_index")?),
        "SourceDebugExtension" => AttributeData::SourceDebugExtension(body.rest().to_vec()),
        "LineNumberTable" => {
            let count = body.u2("line_number_table_length")?;
            let mut table = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                table.push(LineNumber {
                    start_pc: body.u2("start_pc")?,
                    line_number: body.u2("line_number")?,
                });
            }
            AttributeData::LineNumberTable(table)
        }
        "LocalVariableTable" => {
            let count = body.u2("local_variable_table_length")?;
            let mut table = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                table.push(LocalVariable {
                    start_pc: body.u2("start_pc")?,
                    length: body.u2("length")?,
                    name_index: body.u2("name_index")?,
                    descriptor_index: body.u2("descriptor_index")?,
                    index: body.u2("index")?,
                });
            }
            AttributeData::LocalVariableTable(table)
        }
        "LocalVariableTypeTable" => {
            let count = body.u2("local_variable_type_table_length")?;
            let mut table = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                table.push(LocalVariableType {
                    start_pc: body.u2("start_pc")?,
                    length: body.u2("length")?,
                    name_index: body.u2("name_index")?,
                    signature_index: body.u2("signature_index")?,
                    index: body.u2("index")?,
                });
            }
            AttributeData::LocalVariableTypeTable(table)
        }
        "Deprecated" => AttributeData::Deprecated,
        "RuntimeVisibleAnnotations" => {
            AttributeData::RuntimeVisibleAnnotations(read_annotations(body)?)
        }
        "RuntimeInvisibleAnnotations" => {
            AttributeData::RuntimeInvisibleAnnotations(read_annotations(body)?)
        }
        "RuntimeVisibleParameterAnnotations" => {
            AttributeData::RuntimeVisibleParameterAnnotations(read_parameter_annotations(body)?)
        }
        "RuntimeInvisibleParameterAnnotations" => {
            AttributeData::RuntimeInvisibleParameterAnnotations(read_parameter_annotations(body)?)
        }
        "RuntimeVisibleTypeAnnotations" => {
            AttributeData::RuntimeVisibleTypeAnnotations(read_type_annotations(body)?)
        }
        "RuntimeInvisibleTypeAnnotations" => {
            AttributeData::RuntimeInvisibleTypeAnnotations(read_type_annotations(body)?)
        }
        "AnnotationDefault" => AttributeData::AnnotationDefault(read_element_value(body)?),
        "BootstrapMethods" => {
            let count = body.u2("num_bootstrap_methods")?;
            let mut methods = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                let bootstrap_method_ref = body.u2("bootstrap_method_ref")?;
                let arg_count = body.u2("num_bootstrap_arguments")?;
                let mut bootstrap_arguments = Vec::with_capacity(usize::from(arg_count));
                for _ in 0..arg_count {
                    bootstrap_arguments.push(body.u2("bootstrap_argument")?);
                }
                methods.push(BootstrapMethod {
                    bootstrap_method_ref,
                    bootstrap_arguments,
                });
            }
            AttributeData::BootstrapMethods(methods)
        }
        "MethodParameters" => {
            let count = body.u1("parameters_count")?;
            let mut parameters = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                parameters.push(MethodParameter {
                    name_index: body.u2("name_index")?,
                    access_flags: body.u2("access_flags")?,
                });
            }
            AttributeData::MethodParameters(parameters)
        }
        "Module" => AttributeData::Module(read_module(body)?),
        "ModulePackages" => {
            let count = body.u2("package_count")?;
            let mut packages = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                packages.push(body.u2("package_index")?);
            }
            AttributeData::ModulePackages(packages)
        }
        "ModuleMainClass" => AttributeData::ModuleMainClass(body.u2("main_class_index")?),
        "NestHost" => AttributeData::NestHost(body.u2("host_class_index")?),
        "NestMembers" => {
            let count = body.u2("number_of_classes")?;
            let mut classes = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                classes.push(body.u2("class_index")?);
            }
            AttributeData::NestMembers(classes)
        }
        "Record" => {
            let count = body.u2("components_count")?;
            let mut components = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                components.push(read_record_component(body, cp)?);
            }
            AttributeData::Record(components)
        }
        "PermittedSubclasses" => {
            let count = body.u2("number_of_classes")?;
            let mut classes = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                classes.push(body.u2("class_index")?);
            }
            AttributeData::PermittedSubclasses(classes)
        }
        _ => AttributeData::Raw(body.rest().to_vec()),
    })
}

fn read_code(body: &mut Reader<'_>, cp: &ConstantPool) -> Result<CodeAttribute, ParseError> {
    let max_stack = body.u2("max_stack")?;
    let max_locals = body.u2("max_locals")?;
    let code_length = body.u4("code_length")? as usize;
    let code = body.vec(code_length, "code")?;
    let exception_count = body.u2("exception_table_length")?;
    let mut exception_table = Vec::with_capacity(usize::from(exception_count));
    for _ in 0..exception_count {
        exception_table.push(ExceptionHandler {
            start_pc: body.u2("start_pc")?,
            end_pc: body.u2("end_pc")?,
            handler_pc: body.u2("handler_pc")?,
            catch_type: body.u2("catch_type")?,
        });
    }
    let attribute_count = body.u2("attributes_count")?;
    let mut attributes = Vec::with_capacity(usize::from(attribute_count));
    for _ in 0..attribute_count {
        attributes.push(read_attribute(body, cp)?);
    }
    Ok(CodeAttribute {
        max_stack,
        max_locals,
        code,
        exception_table,
        attributes,
    })
}

fn read_record_component(
    body: &mut Reader<'_>,
    cp: &ConstantPool,
) -> Result<RecordComponent, ParseError> {
    let name_index = body.u2("name_index")?;
    let descriptor_index = body.u2("descriptor_index")?;
    let attribute_count = body.u2("attributes_count")?;
    let mut attributes = Vec::with_capacity(usize::from(attribute_count));
    for _ in 0..attribute_count {
        attributes.push(read_attribute(body, cp)?);
    }
    Ok(RecordComponent {
        name_index,
        descriptor_index,
        attributes,
    })
}

fn read_stack_map_table(body: &mut Reader<'_>) -> Result<Vec<StackMapFrame>, ParseError> {
    let count = body.u2("number_of_entries")?;
    let mut frames = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        let frame_type = body.u1("frame_type")?;
        frames.push(match frame_type {
            0..=63 => StackMapFrame::Same {
                offset_delta: u16::from(frame_type),
            },
            64..=127 => StackMapFrame::SameLocals1 {
                offset_delta: u16::from(frame_type - 64),
                stack: read_verification_type(body)?,
            },
            247 => StackMapFrame::SameLocals1Extended {
                offset_delta: body.u2("offset_delta")?,
                stack: read_verification_type(body)?,
            },
            248..=250 => StackMapFrame::Chop {
                offset_delta: body.u2("offset_delta")?,
                k: 251 - frame_type,
            },
            251 => StackMapFrame::SameExtended {
                offset_delta: body.u2("offset_delta")?,
            },
            252..=254 => {
                let n = frame_type - 251;
                let offset_delta = body.u2("offset_delta")?;
                let mut locals = Vec::with_capacity(usize::from(n));
                for _ in 0..n {
                    locals.push(read_verification_type(body)?);
                }
                StackMapFrame::Append {
                    offset_delta,
                    locals,
                }
            }
            255 => {
                let offset_delta = body.u2("offset_delta")?;
                let locals_count = body.u2("number_of_locals")?;
                let mut locals = Vec::with_capacity(usize::from(locals_count));
                for _ in 0..locals_count {
                    locals.push(read_verification_type(body)?);
                }
                let stack_count = body.u2("number_of_stack_items")?;
                let mut stack = Vec::with_capacity(usize::from(stack_count));
                for _ in 0..stack_count {
                    stack.push(read_verification_type(body)?);
                }
                StackMapFrame::Full {
                    offset_delta,
                    locals,
                    stack,
                }
            }
            128..=246 => {
                return Err(ParseError::Other("reserved StackMapTable frame type"));
            }
        });
    }
    Ok(frames)
}

fn read_verification_type(body: &mut Reader<'_>) -> Result<VerificationType, ParseError> {
    Ok(match body.u1("verification_type_info tag")? {
        0 => VerificationType::Top,
        1 => VerificationType::Integer,
        2 => VerificationType::Float,
        3 => VerificationType::Double,
        4 => VerificationType::Long,
        5 => VerificationType::Null,
        6 => VerificationType::UninitializedThis,
        7 => VerificationType::Object(body.u2("cpool_index")?),
        8 => VerificationType::Uninitialized(body.u2("offset")?),
        _ => return Err(ParseError::Other("invalid verification_type_info tag")),
    })
}

fn read_annotations(body: &mut Reader<'_>) -> Result<Vec<Annotation>, ParseError> {
    let count = body.u2("num_annotations")?;
    let mut annotations = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        annotations.push(read_annotation(body)?);
    }
    Ok(annotations)
}

fn read_annotation(body: &mut Reader<'_>) -> Result<Annotation, ParseError> {
    let type_index = body.u2("type_index")?;
    let count = body.u2("num_element_value_pairs")?;
    let mut elements = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        elements.push(ElementPair {
            name_index: body.u2("element_name_index")?,
            value: read_element_value(body)?,
        });
    }
    Ok(Annotation {
        type_index,
        elements,
    })
}

fn read_element_value(body: &mut Reader<'_>) -> Result<ElementValue, ParseError> {
    let tag = body.u1("element_value tag")?;
    Ok(match tag {
        b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' | b's' => ElementValue::Constant {
            tag,
            index: body.u2("const_value_index")?,
        },
        b'e' => ElementValue::Enum {
            type_name_index: body.u2("type_name_index")?,
            const_name_index: body.u2("const_name_index")?,
        },
        b'c' => ElementValue::Class(body.u2("class_info_index")?),
        b'@' => ElementValue::Annotation(read_annotation(body)?),
        b'[' => {
            let count = body.u2("num_values")?;
            let mut values = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                values.push(read_element_value(body)?);
            }
            ElementValue::Array(values)
        }
        _ => return Err(ParseError::Other("invalid element_value tag")),
    })
}

fn read_parameter_annotations(body: &mut Reader<'_>) -> Result<Vec<Vec<Annotation>>, ParseError> {
    let count = body.u1("num_parameters")?;
    let mut parameters = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        parameters.push(read_annotations(body)?);
    }
    Ok(parameters)
}

fn read_type_annotations(body: &mut Reader<'_>) -> Result<Vec<TypeAnnotation>, ParseError> {
    let count = body.u2("num_annotations")?;
    let mut annotations = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        annotations.push(read_type_annotation(body)?);
    }
    Ok(annotations)
}

fn read_type_annotation(body: &mut Reader<'_>) -> Result<TypeAnnotation, ParseError> {
    let target_type = body.u1("target_type")?;
    let target_info = match target_type {
        0x00 | 0x01 => TargetInfo::TypeParameter {
            index: body.u1("type_parameter_index")?,
        },
        0x10 => TargetInfo::Supertype {
            index: body.u2("supertype_index")?,
        },
        0x11 | 0x12 => TargetInfo::TypeParameterBound {
            parameter_index: body.u1("type_parameter_index")?,
            bound_index: body.u1("bound_index")?,
        },
        0x13..=0x15 => TargetInfo::Empty,
        0x16 => TargetInfo::FormalParameter {
            index: body.u1("formal_parameter_index")?,
        },
        0x17 => TargetInfo::Throws {
            index: body.u2("throws_type_index")?,
        },
        0x40 | 0x41 => {
            let count = body.u2("table_length")?;
            let mut table = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                table.push(LocalVarTargetEntry {
                    start_pc: body.u2("start_pc")?,
                    length: body.u2("length")?,
                    index: body.u2("index")?,
                });
            }
            TargetInfo::LocalVar { table }
        }
        0x42 => TargetInfo::Catch {
            index: body.u2("exception_table_index")?,
        },
        0x43..=0x46 => TargetInfo::Offset {
            offset: body.u2("offset")?,
        },
        0x47..=0x4B => TargetInfo::TypeArgument {
            offset: body.u2("offset")?,
            index: body.u1("type_argument_index")?,
        },
        _ => return Err(ParseError::Other("invalid type annotation target_type")),
    };
    let path_length = body.u1("path_length")?;
    let mut target_path = Vec::with_capacity(usize::from(path_length));
    for _ in 0..path_length {
        target_path.push(TypePathEntry {
            type_path_kind: body.u1("type_path_kind")?,
            type_argument_index: body.u1("type_argument_index")?,
        });
    }
    let type_index = body.u2("type_index")?;
    let count = body.u2("num_element_value_pairs")?;
    let mut elements = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        elements.push(ElementPair {
            name_index: body.u2("element_name_index")?,
            value: read_element_value(body)?,
        });
    }
    Ok(TypeAnnotation {
        target_type,
        target_info,
        target_path,
        type_index,
        elements,
    })
}

fn read_module(body: &mut Reader<'_>) -> Result<ModuleAttribute, ParseError> {
    let module_name_index = body.u2("module_name_index")?;
    let module_flags = body.u2("module_flags")?;
    let module_version_index = body.u2("module_version_index")?;
    let requires_count = body.u2("requires_count")?;
    let mut requires = Vec::with_capacity(usize::from(requires_count));
    for _ in 0..requires_count {
        requires.push(ModuleRequires {
            requires_index: body.u2("requires_index")?,
            requires_flags: body.u2("requires_flags")?,
            requires_version_index: body.u2("requires_version_index")?,
        });
    }
    let exports_count = body.u2("exports_count")?;
    let mut exports = Vec::with_capacity(usize::from(exports_count));
    for _ in 0..exports_count {
        let exports_index = body.u2("exports_index")?;
        let exports_flags = body.u2("exports_flags")?;
        let to_count = body.u2("exports_to_count")?;
        let mut exports_to_index = Vec::with_capacity(usize::from(to_count));
        for _ in 0..to_count {
            exports_to_index.push(body.u2("exports_to_index")?);
        }
        exports.push(ModuleExports {
            exports_index,
            exports_flags,
            exports_to_index,
        });
    }
    let opens_count = body.u2("opens_count")?;
    let mut opens = Vec::with_capacity(usize::from(opens_count));
    for _ in 0..opens_count {
        let opens_index = body.u2("opens_index")?;
        let opens_flags = body.u2("opens_flags")?;
        let to_count = body.u2("opens_to_count")?;
        let mut opens_to_index = Vec::with_capacity(usize::from(to_count));
        for _ in 0..to_count {
            opens_to_index.push(body.u2("opens_to_index")?);
        }
        opens.push(ModuleOpens {
            opens_index,
            opens_flags,
            opens_to_index,
        });
    }
    let uses_count = body.u2("uses_count")?;
    let mut uses = Vec::with_capacity(usize::from(uses_count));
    for _ in 0..uses_count {
        uses.push(body.u2("uses_index")?);
    }
    let provides_count = body.u2("provides_count")?;
    let mut provides = Vec::with_capacity(usize::from(provides_count));
    for _ in 0..provides_count {
        let provides_index = body.u2("provides_index")?;
        let with_count = body.u2("provides_with_count")?;
        let mut provides_with_index = Vec::with_capacity(usize::from(with_count));
        for _ in 0..with_count {
            provides_with_index.push(body.u2("provides_with_index")?);
        }
        provides.push(ModuleProvides {
            provides_index,
            provides_with_index,
        });
    }
    Ok(ModuleAttribute {
        module_name_index,
        module_flags,
        module_version_index,
        requires,
        exports,
        opens,
        uses,
        provides,
    })
}

// ---------------------------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------------------------

pub(crate) fn write_attribute(attribute: &Attribute, writer: &mut Writer) {
    writer.u2(attribute.name_index);
    let mut body = Writer::new();
    write_attribute_body(&attribute.data, &mut body);
    writer.u4(body.len() as u32);
    writer.bytes(body.as_slice());
}

fn write_attribute_body(data: &AttributeData, body: &mut Writer) {
    match data {
        AttributeData::ConstantValue(index) => body.u2(*index),
        AttributeData::Code(code) => write_code(code, body),
        AttributeData::StackMapTable(frames) => write_stack_map_table(frames, body),
        AttributeData::Exceptions(table) => {
            body.u2(table.len() as u16);
            for index in table {
                body.u2(*index);
            }
        }
        AttributeData::InnerClasses(classes) => {
            body.u2(classes.len() as u16);
            for class in classes {
                body.u2(class.inner_class_info_index);
                body.u2(class.outer_class_info_index);
                body.u2(class.inner_name_index);
                body.u2(class.inner_class_access_flags);
            }
        }
        AttributeData::EnclosingMethod(method) => {
            body.u2(method.class_index);
            body.u2(method.method_index);
        }
        AttributeData::Synthetic | AttributeData::Deprecated => {}
        AttributeData::Signature(index) => body.u2(*index),
        AttributeData::SourceFile(index) => body.u2(*index),
        AttributeData::SourceDebugExtension(bytes) => body.bytes(bytes),
        AttributeData::LineNumberTable(table) => {
            body.u2(table.len() as u16);
            for line in table {
                body.u2(line.start_pc);
                body.u2(line.line_number);
            }
        }
        AttributeData::LocalVariableTable(table) => {
            body.u2(table.len() as u16);
            for var in table {
                body.u2(var.start_pc);
                body.u2(var.length);
                body.u2(var.name_index);
                body.u2(var.descriptor_index);
                body.u2(var.index);
            }
        }
        AttributeData::LocalVariableTypeTable(table) => {
            body.u2(table.len() as u16);
            for var in table {
                body.u2(var.start_pc);
                body.u2(var.length);
                body.u2(var.name_index);
                body.u2(var.signature_index);
                body.u2(var.index);
            }
        }
        AttributeData::RuntimeVisibleAnnotations(annotations)
        | AttributeData::RuntimeInvisibleAnnotations(annotations) => {
            write_annotations(annotations, body);
        }
        AttributeData::RuntimeVisibleParameterAnnotations(parameters)
        | AttributeData::RuntimeInvisibleParameterAnnotations(parameters) => {
            body.u1(parameters.len() as u8);
            for annotations in parameters {
                write_annotations(annotations, body);
            }
        }
        AttributeData::RuntimeVisibleTypeAnnotations(annotations)
        | AttributeData::RuntimeInvisibleTypeAnnotations(annotations) => {
            body.u2(annotations.len() as u16);
            for annotation in annotations {
                write_type_annotation(annotation, body);
            }
        }
        AttributeData::AnnotationDefault(value) => write_element_value(value, body),
        AttributeData::BootstrapMethods(methods) => {
            body.u2(methods.len() as u16);
            for method in methods {
                body.u2(method.bootstrap_method_ref);
                body.u2(method.bootstrap_arguments.len() as u16);
                for argument in &method.bootstrap_arguments {
                    body.u2(*argument);
                }
            }
        }
        AttributeData::MethodParameters(parameters) => {
            body.u1(parameters.len() as u8);
            for parameter in parameters {
                body.u2(parameter.name_index);
                body.u2(parameter.access_flags);
            }
        }
        AttributeData::Module(module) => write_module(module, body),
        AttributeData::ModulePackages(packages) => {
            body.u2(packages.len() as u16);
            for package in packages {
                body.u2(*package);
            }
        }
        AttributeData::ModuleMainClass(index) => body.u2(*index),
        AttributeData::NestHost(index) => body.u2(*index),
        AttributeData::NestMembers(classes) => {
            body.u2(classes.len() as u16);
            for class in classes {
                body.u2(*class);
            }
        }
        AttributeData::Record(components) => {
            body.u2(components.len() as u16);
            for component in components {
                body.u2(component.name_index);
                body.u2(component.descriptor_index);
                body.u2(component.attributes.len() as u16);
                for attribute in &component.attributes {
                    write_attribute(attribute, body);
                }
            }
        }
        AttributeData::PermittedSubclasses(classes) => {
            body.u2(classes.len() as u16);
            for class in classes {
                body.u2(*class);
            }
        }
        AttributeData::Raw(bytes) => body.bytes(bytes),
    }
}

fn write_code(code: &CodeAttribute, body: &mut Writer) {
    body.u2(code.max_stack);
    body.u2(code.max_locals);
    body.u4(code.code.len() as u32);
    body.bytes(&code.code);
    body.u2(code.exception_table.len() as u16);
    for handler in &code.exception_table {
        body.u2(handler.start_pc);
        body.u2(handler.end_pc);
        body.u2(handler.handler_pc);
        body.u2(handler.catch_type);
    }
    body.u2(code.attributes.len() as u16);
    for attribute in &code.attributes {
        write_attribute(attribute, body);
    }
}

fn write_stack_map_table(frames: &[StackMapFrame], body: &mut Writer) {
    body.u2(frames.len() as u16);
    for frame in frames {
        match frame {
            StackMapFrame::Same { offset_delta } => {
                body.u1(*offset_delta as u8);
            }
            StackMapFrame::SameLocals1 {
                offset_delta,
                stack,
            } => {
                body.u1(64 + *offset_delta as u8);
                write_verification_type(stack, body);
            }
            StackMapFrame::SameLocals1Extended {
                offset_delta,
                stack,
            } => {
                body.u1(247);
                body.u2(*offset_delta);
                write_verification_type(stack, body);
            }
            StackMapFrame::Chop { offset_delta, k } => {
                body.u1(251 - k);
                body.u2(*offset_delta);
            }
            StackMapFrame::SameExtended { offset_delta } => {
                body.u1(251);
                body.u2(*offset_delta);
            }
            StackMapFrame::Append {
                offset_delta,
                locals,
            } => {
                body.u1(251 + locals.len() as u8);
                body.u2(*offset_delta);
                for local in locals {
                    write_verification_type(local, body);
                }
            }
            StackMapFrame::Full {
                offset_delta,
                locals,
                stack,
            } => {
                body.u1(255);
                body.u2(*offset_delta);
                body.u2(locals.len() as u16);
                for local in locals {
                    write_verification_type(local, body);
                }
                body.u2(stack.len() as u16);
                for item in stack {
                    write_verification_type(item, body);
                }
            }
        }
    }
}

fn write_verification_type(ty: &VerificationType, body: &mut Writer) {
    match ty {
        VerificationType::Top => body.u1(0),
        VerificationType::Integer => body.u1(1),
        VerificationType::Float => body.u1(2),
        VerificationType::Double => body.u1(3),
        VerificationType::Long => body.u1(4),
        VerificationType::Null => body.u1(5),
        VerificationType::UninitializedThis => body.u1(6),
        VerificationType::Object(index) => {
            body.u1(7);
            body.u2(*index);
        }
        VerificationType::Uninitialized(offset) => {
            body.u1(8);
            body.u2(*offset);
        }
    }
}

fn write_annotations(annotations: &[Annotation], body: &mut Writer) {
    body.u2(annotations.len() as u16);
    for annotation in annotations {
        write_annotation(annotation, body);
    }
}

fn write_annotation(annotation: &Annotation, body: &mut Writer) {
    body.u2(annotation.type_index);
    body.u2(annotation.elements.len() as u16);
    for element in &annotation.elements {
        body.u2(element.name_index);
        write_element_value(&element.value, body);
    }
}

fn write_element_value(value: &ElementValue, body: &mut Writer) {
    match value {
        ElementValue::Constant { tag, index } => {
            body.u1(*tag);
            body.u2(*index);
        }
        ElementValue::Enum {
            type_name_index,
            const_name_index,
        } => {
            body.u1(b'e');
            body.u2(*type_name_index);
            body.u2(*const_name_index);
        }
        ElementValue::Class(index) => {
            body.u1(b'c');
            body.u2(*index);
        }
        ElementValue::Annotation(annotation) => {
            body.u1(b'@');
            write_annotation(annotation, body);
        }
        ElementValue::Array(values) => {
            body.u1(b'[');
            body.u2(values.len() as u16);
            for value in values {
                write_element_value(value, body);
            }
        }
    }
}

fn write_type_annotation(annotation: &TypeAnnotation, body: &mut Writer) {
    body.u1(annotation.target_type);
    match &annotation.target_info {
        TargetInfo::TypeParameter { index } => body.u1(*index),
        TargetInfo::Supertype { index } => body.u2(*index),
        TargetInfo::TypeParameterBound {
            parameter_index,
            bound_index,
        } => {
            body.u1(*parameter_index);
            body.u1(*bound_index);
        }
        TargetInfo::Empty => {}
        TargetInfo::FormalParameter { index } => body.u1(*index),
        TargetInfo::Throws { index } => body.u2(*index),
        TargetInfo::LocalVar { table } => {
            body.u2(table.len() as u16);
            for entry in table {
                body.u2(entry.start_pc);
                body.u2(entry.length);
                body.u2(entry.index);
            }
        }
        TargetInfo::Catch { index } => body.u2(*index),
        TargetInfo::Offset { offset } => body.u2(*offset),
        TargetInfo::TypeArgument { offset, index } => {
            body.u2(*offset);
            body.u1(*index);
        }
    }
    body.u1(annotation.target_path.len() as u8);
    for entry in &annotation.target_path {
        body.u1(entry.type_path_kind);
        body.u1(entry.type_argument_index);
    }
    body.u2(annotation.type_index);
    body.u2(annotation.elements.len() as u16);
    for element in &annotation.elements {
        body.u2(element.name_index);
        write_element_value(&element.value, body);
    }
}

fn write_module(module: &ModuleAttribute, body: &mut Writer) {
    body.u2(module.module_name_index);
    body.u2(module.module_flags);
    body.u2(module.module_version_index);
    body.u2(module.requires.len() as u16);
    for requires in &module.requires {
        body.u2(requires.requires_index);
        body.u2(requires.requires_flags);
        body.u2(requires.requires_version_index);
    }
    body.u2(module.exports.len() as u16);
    for exports in &module.exports {
        body.u2(exports.exports_index);
        body.u2(exports.exports_flags);
        body.u2(exports.exports_to_index.len() as u16);
        for index in &exports.exports_to_index {
            body.u2(*index);
        }
    }
    body.u2(module.opens.len() as u16);
    for opens in &module.opens {
        body.u2(opens.opens_index);
        body.u2(opens.opens_flags);
        body.u2(opens.opens_to_index.len() as u16);
        for index in &opens.opens_to_index {
            body.u2(*index);
        }
    }
    body.u2(module.uses.len() as u16);
    for index in &module.uses {
        body.u2(*index);
    }
    body.u2(module.provides.len() as u16);
    for provides in &module.provides {
        body.u2(provides.provides_index);
        body.u2(provides.provides_with_index.len() as u16);
        for index in &provides.provides_with_index {
            body.u2(*index);
        }
    }
}
