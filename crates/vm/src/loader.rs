//! Class loading, linking, resolution, and initialization (JVMS ch. 5).
//!
//! The loader is the single gateway between bytes and [`Class`]: it parses with
//! `oxjvm-classfile`, assigns field slots, resolves supertypes, links methods, runs the verifier,
//! and computes dynamic call sites. Constant-pool *member* resolution (5.4.3) lives here too, so
//! the interpreter only ever dispatches resolved targets.

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::attribute::AttributeData;
use oxjvm_classfile::descriptor::MethodDescriptor;
use oxjvm_classfile::flags::*;
use oxjvm_classfile::{ClassFile, ConstantPool, CpInfo, FieldType};

use crate::class::{
    ArrayComponent, Class, ClassId, ClassKind, ClassState, Code, Field, Handler, Method,
    NativeClass, NativeConstant,
};
use crate::error::VmError;
use crate::heap::{MethodHandleValue, ObjectData};
use crate::value::{ObjectRef, Value};
use crate::{CallSite, ConcatConstant, LambdaDef, Vm};

/// A resolved method reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedMethod {
    /// The class that declares the method.
    pub class: ClassId,
    /// Index into the declaring class's method table.
    pub method: u32,
    /// Whether the reference is an interface method reference.
    pub is_interface: bool,
}

impl ResolvedMethod {
    /// Whether the method is static.
    #[must_use]
    pub fn is_static(self, vm: &Vm<'_>) -> bool {
        vm.classes.get(self.class).methods[self.method as usize].is_static()
    }
}

impl<'a> Vm<'a> {
    // -----------------------------------------------------------------------------------------
    // Loading
    // -----------------------------------------------------------------------------------------

    /// Resolve an internal name to a loaded class, loading it when necessary.
    ///
    /// # Errors
    ///
    /// Throws `NoClassDefFoundError` when neither the native registry nor the host class path
    /// provides the class, and `LinkageError` for circularity or malformed class files.
    pub fn resolve_class(&mut self, name: &str) -> Result<ClassId, VmError> {
        if let Some(id) = self.classes.by_name(name) {
            return Ok(id);
        }
        if name.starts_with('[') {
            return self.resolve_array_class(name);
        }
        if let Some(def) = self.natives.iter().find(|native| native.name == name) {
            return Ok(self.load_native_class(def));
        }
        if self.loading.contains(name) {
            return Err(self.throw_new(
                "java/lang/ClassCircularityError",
                Some(&format!("circular loading of {name}")),
            ));
        }
        let bytes = self.host.load_class(name).ok_or_else(|| {
            self.throw_new(
                "java/lang/NoClassDefFoundError",
                Some(&name.replace('/', ".")),
            )
        })?;
        self.loading.insert(name.into());
        let result = self.define_class(name, &bytes);
        self.loading.remove(name);
        result
    }

    /// Resolve a class and never fail: `None` when it cannot be loaded.
    pub(crate) fn resolve_class_lenient(&mut self, name: &str) -> Option<ClassId> {
        self.resolve_class(name).ok()
    }

    /// Define and link a class from class-file bytes that are already in hand.
    ///
    /// # Errors
    ///
    /// Fails on parse errors, linkage errors, and verification failures.
    pub fn define_class(&mut self, name: &str, bytes: &[u8]) -> Result<ClassId, VmError> {
        let parsed = ClassFile::read(bytes).map_err(|error| {
            VmError::invalid_code(name, format!("class file parse error: {error}"))
        })?;
        let this_name = parsed
            .this_name()
            .map_err(|error| VmError::invalid_code(name, format!("bad this_class: {error}")))?
            .to_string();
        if this_name != name {
            return Err(VmError::invalid_code(
                name,
                format!("expected class {name}, found {this_name}"),
            ));
        }
        if let Some(existing) = self.classes.by_name(name) {
            return Ok(existing);
        }
        self.loading.insert(name.into());
        let result = self.link_class(name, &parsed);
        self.loading.remove(name);
        result
    }

    fn link_class(&mut self, name: &str, parsed: &ClassFile) -> Result<ClassId, VmError> {
        let super_name = parsed
            .super_name()
            .map_err(|error| VmError::invalid_code(name, format!("bad super_class: {error}")))?;
        let super_class = match super_name {
            Some(super_name) if super_name != name => Some(self.resolve_class(super_name)?),
            Some(_) => {
                return Err(self.throw_new(
                    "java/lang/ClassCircularityError",
                    Some(&format!("{name} extends itself")),
                ));
            }
            None => None,
        };
        let mut interfaces = Vec::new();
        for interface in parsed
            .interface_names()
            .map_err(|error| VmError::invalid_code(name, format!("bad interfaces: {error}")))?
        {
            interfaces.push(self.resolve_class(interface)?);
        }

        crate::verifier::verify_class(parsed)
            .map_err(|message| VmError::invalid_code(name, message))?;

        // Fields.
        let mut fields = Vec::with_capacity(parsed.fields.len());
        let mut instance_slot = super_class.map_or(0, |id| self.classes.get(id).instance_slots);
        let mut static_slot = 0u16;
        for field in &parsed.fields {
            let field_name = parsed
                .constant_pool
                .utf8(field.name_index)
                .map_err(|error| VmError::invalid_code(name, error.to_string()))?
                .to_string();
            let descriptor = parsed
                .constant_pool
                .utf8(field.descriptor_index)
                .map_err(|error| VmError::invalid_code(name, error.to_string()))?
                .to_string();
            FieldType::parse(&descriptor).map_err(|message| {
                VmError::invalid_code(name, format!("field {field_name}: {message}"))
            })?;
            let is_static = field.access_flags & ACC_STATIC != 0;
            let constant = if is_static && field.access_flags & ACC_FINAL != 0 {
                field.attributes.iter().find_map(|attribute| {
                    if let AttributeData::ConstantValue(index) = &attribute.data {
                        Some(*index)
                    } else {
                        None
                    }
                })
            } else {
                None
            };
            let constant = match constant {
                Some(index) => Some(self.resolve_constant(name, &parsed.constant_pool, index)?),
                None => None,
            };
            let slot = if is_static {
                let slot = static_slot;
                static_slot += FieldType::parse(&descriptor).expect("checked").slots();
                slot
            } else {
                let slot = instance_slot;
                instance_slot += FieldType::parse(&descriptor).expect("checked").slots();
                slot
            };
            fields.push(Field {
                name: field_name,
                descriptor,
                access_flags: field.access_flags,
                owner: ClassId(0),
                is_static,
                slot,
                constant,
            });
        }

        // Methods.
        let mut methods = Vec::with_capacity(parsed.methods.len());
        for method in &parsed.methods {
            let method_name = parsed
                .constant_pool
                .utf8(method.name_index)
                .map_err(|error| VmError::invalid_code(name, error.to_string()))?
                .to_string();
            let descriptor = parsed
                .constant_pool
                .utf8(method.descriptor_index)
                .map_err(|error| VmError::invalid_code(name, error.to_string()))?
                .to_string();
            MethodDescriptor::parse(&descriptor).map_err(|message| {
                VmError::invalid_code(name, format!("method {method_name}: {message}"))
            })?;
            let code = method
                .attributes
                .iter()
                .find_map(|attribute| match &attribute.data {
                    AttributeData::Code(code) => Some(code),
                    _ => None,
                });
            let code = match code {
                Some(code) => Some(Arc::new(self.link_code(name, &method_name, code)?)),
                None => None,
            };
            let exceptions = method
                .attributes
                .iter()
                .find_map(|attribute| match &attribute.data {
                    AttributeData::Exceptions(indices) => Some(indices.clone()),
                    _ => None,
                })
                .map(|indices| {
                    indices
                        .iter()
                        .filter_map(|index| {
                            let class_name = parsed.constant_pool.class_name(*index).ok()?;
                            self.resolve_class_lenient(class_name)
                        })
                        .collect()
                })
                .unwrap_or_default();
            let parameter_names = self.parameter_names(&descriptor, method);
            methods.push(Method {
                name: method_name,
                descriptor,
                access_flags: method.access_flags,
                owner: ClassId(0),
                code,
                native: None,
                exceptions,
                parameter_names,
            });
        }

        // Class attributes.
        let source_file = parsed
            .find_attribute("SourceFile")
            .and_then(|attribute| match &attribute.data {
                AttributeData::SourceFile(index) => parsed.constant_pool.utf8(*index).ok(),
                _ => None,
            })
            .map(str::to_string);
        let bootstrap_methods = parsed
            .find_attribute("BootstrapMethods")
            .and_then(|attribute| match &attribute.data {
                AttributeData::BootstrapMethods(methods) => Some(
                    methods
                        .iter()
                        .map(|method| crate::class::BootstrapMethod::Unresolved {
                            method_ref: method.bootstrap_method_ref,
                            arguments: method.bootstrap_arguments.clone(),
                        })
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default();
        let nest_host = parsed
            .find_attribute("NestHost")
            .and_then(|attribute| match &attribute.data {
                AttributeData::NestHost(index) => parsed.constant_pool.class_name(*index).ok(),
                _ => None,
            })
            .and_then(|class_name| self.resolve_class_lenient(class_name));
        let permitted_subclasses = parsed
            .find_attribute("PermittedSubclasses")
            .and_then(|attribute| match &attribute.data {
                AttributeData::PermittedSubclasses(indices) => Some(indices.clone()),
                _ => None,
            })
            .map(|indices| {
                indices
                    .iter()
                    .filter_map(|index| {
                        let class_name = parsed.constant_pool.class_name(*index).ok()?;
                        self.resolve_class_lenient(class_name)
                    })
                    .collect()
            })
            .unwrap_or_default();

        let id = self.classes.insert(Class {
            id: ClassId(0),
            name: name.into(),
            super_class,
            interfaces,
            access_flags: parsed.access_flags,
            state: ClassState::Prepared,
            constant_pool: parsed.constant_pool.clone(),
            fields,
            methods,
            static_values: Vec::new(),
            instance_defaults: Vec::new(),
            instance_slots: instance_slot,
            source_file,
            bootstrap_methods,
            class_object: None,
            kind: ClassKind::Class,
            component: None,
            component_class: None,
            primitive: None,
            native_definition: None,
            initialization_error: None,
            nest_host,
            permitted_subclasses,
            is_enum: parsed.access_flags & ACC_ENUM != 0,
            verified: true,
        });
        // Fix up back-pointers and default values now that the class has a stable id.
        let mut defaults = super_class.map_or_else(Vec::new, |super_class| {
            self.classes.get(super_class).instance_defaults.clone()
        });
        defaults.resize(instance_slot as usize, Value::Int(0));
        let mut static_values = vec![Value::Int(0); static_slot as usize];
        {
            let class = self.classes.get_mut(id);
            class.id = id;
            let mut index = 0;
            while index < class.fields.len() {
                let field = &mut class.fields[index];
                field.owner = id;
                let descriptor = field.descriptor.as_bytes().first().copied().unwrap_or(b'I');
                if field.is_static {
                    static_values[field.slot as usize] = field
                        .constant
                        .unwrap_or_else(|| Value::default_for(descriptor));
                } else {
                    defaults[field.slot as usize] = field
                        .constant
                        .unwrap_or_else(|| Value::default_for(descriptor));
                }
                index += 1;
            }
            let mut index = 0;
            while index < class.methods.len() {
                class.methods[index].owner = id;
                index += 1;
            }
            class.static_values = static_values;
            class.instance_defaults = defaults;
        }
        Ok(id)
    }

    fn resolve_constant(
        &mut self,
        context: &str,
        pool: &ConstantPool,
        index: u16,
    ) -> Result<Value, VmError> {
        let info = pool
            .get(index)
            .map_err(|error| VmError::invalid_code(context, error.to_string()))?;
        Ok(match info {
            CpInfo::Integer(value) => Value::Int(*value),
            CpInfo::Long(value) => Value::Long(*value),
            CpInfo::Float(bits) => Value::Float(f32::from_bits(*bits)),
            CpInfo::Double(bits) => Value::Double(f64::from_bits(*bits)),
            CpInfo::String(string_index) => {
                let text = pool
                    .utf8(*string_index)
                    .map_err(|error| VmError::invalid_code(context, error.to_string()))?;
                Value::Ref(self.intern(text))
            }
            other => {
                return Err(VmError::invalid_code(
                    context,
                    format!("ConstantValue names {}", other.kind_name()),
                ));
            }
        })
    }

    fn link_code(
        &mut self,
        class_name: &str,
        method_name: &str,
        code: &oxjvm_classfile::CodeAttribute,
    ) -> Result<Code, VmError> {
        let exception_table = code
            .exception_table
            .iter()
            .map(|handler| Handler {
                start_pc: handler.start_pc,
                end_pc: handler.end_pc,
                handler_pc: handler.handler_pc,
                catch_type_index: handler.catch_type,
            })
            .collect();
        let mut line_numbers = Vec::new();
        for attribute in &code.attributes {
            if let AttributeData::LineNumberTable(table) = &attribute.data {
                for line in table {
                    line_numbers.push((line.start_pc, line.line_number));
                }
            }
        }
        line_numbers.sort_unstable();
        let _ = (class_name, method_name);
        Ok(Code {
            bytes: code.code.clone(),
            max_stack: code.max_stack,
            max_locals: code.max_locals,
            exception_table,
            line_numbers,
        })
    }

    fn parameter_names(
        &self,
        descriptor: &str,
        method: &oxjvm_classfile::MethodInfo,
    ) -> Vec<Option<String>> {
        let parsed = MethodDescriptor::parse(descriptor).ok();
        let count = parsed.as_ref().map_or(0, |d| d.parameters.len());
        let mut names = vec![None; count];
        // MethodParameters is authoritative when present.
        for attribute in &method.attributes {
            if let AttributeData::MethodParameters(parameters) = &attribute.data {
                if parameters.len() == count {
                    for (index, parameter) in parameters.iter().enumerate() {
                        if parameter.name_index != 0 {
                            names[index] = Some(format!("arg{index}"));
                        }
                    }
                }
                return names;
            }
        }
        names
    }

    // -----------------------------------------------------------------------------------------
    // Native classes
    // -----------------------------------------------------------------------------------------

    /// Load a native class from the registry.
    pub fn load_native_class(&mut self, definition: &'static NativeClass) -> ClassId {
        if let Some(id) = self.classes.by_name(definition.name) {
            return id;
        }
        let super_class = definition.super_name.map(|name| {
            self.resolve_class(name)
                .expect("native superclass must load")
        });
        let interfaces: Vec<ClassId> = definition
            .interfaces
            .iter()
            .map(|name| {
                self.resolve_class(name)
                    .expect("native interface must load")
            })
            .collect();

        let base_slots = super_class.map_or(0, |id| self.classes.get(id).instance_slots);
        let mut instance_slot = base_slots;
        let mut static_slot = 0u16;
        let mut fields = Vec::with_capacity(definition.fields.len());
        for field in definition.fields {
            let is_static = field.access_flags & ACC_STATIC != 0;
            let ty = FieldType::parse(field.descriptor).expect("native field descriptor");
            let slot = if is_static {
                let slot = static_slot;
                static_slot += ty.slots();
                slot
            } else {
                let slot = instance_slot;
                instance_slot += ty.slots();
                slot
            };
            fields.push(Field {
                name: field.name.into(),
                descriptor: field.descriptor.into(),
                access_flags: field.access_flags,
                owner: ClassId(0),
                is_static,
                slot,
                constant: None,
            });
        }

        let mut methods = Vec::with_capacity(definition.methods.len());
        for method in definition.methods {
            methods.push(Method {
                name: method.name.into(),
                descriptor: method.descriptor.into(),
                access_flags: method.access_flags,
                owner: ClassId(0),
                code: None,
                native: Some(method.native),
                exceptions: Vec::new(),
                parameter_names: Vec::new(),
            });
        }

        let id = self.classes.insert(Class {
            id: ClassId(0),
            name: definition.name.into(),
            super_class,
            interfaces,
            access_flags: definition.access_flags,
            state: if definition.clinit.is_some() {
                ClassState::Prepared
            } else {
                ClassState::Initialized
            },
            constant_pool: ConstantPool::new(),
            fields,
            methods,
            static_values: Vec::new(),
            instance_defaults: Vec::new(),
            instance_slots: instance_slot,
            source_file: None,
            bootstrap_methods: Vec::new(),
            class_object: None,
            kind: ClassKind::Class,
            component: None,
            component_class: None,
            primitive: None,
            native_definition: Some(definition),
            initialization_error: None,
            nest_host: None,
            permitted_subclasses: Vec::new(),
            is_enum: definition.access_flags & ACC_ENUM != 0,
            verified: true,
        });
        let mut defaults = super_class.map_or_else(Vec::new, |super_class| {
            self.classes.get(super_class).instance_defaults.clone()
        });
        defaults.resize(instance_slot as usize, Value::Int(0));
        let mut static_values = vec![Value::Int(0); static_slot as usize];
        // Resolve native constants (which may intern strings) before borrowing the class table.
        let native_constants: Vec<Option<Value>> = definition
            .fields
            .iter()
            .map(|native_field| {
                let is_static = native_field.access_flags & ACC_STATIC != 0;
                match (native_field.constant, is_static) {
                    (Some(NativeConstant::Int(value)), true) => Some(Value::Int(value)),
                    (Some(NativeConstant::Long(value)), true) => Some(Value::Long(value)),
                    (Some(NativeConstant::Float(value)), true) => Some(Value::Float(value)),
                    (Some(NativeConstant::Double(value)), true) => Some(Value::Double(value)),
                    (Some(NativeConstant::Str(text)), true) => Some(Value::Ref(self.intern(text))),
                    _ => None,
                }
            })
            .collect();
        {
            let class = self.classes.get_mut(id);
            class.id = id;
            let mut index = 0;
            while index < class.fields.len() {
                let field = &class.fields[index];
                let descriptor = field.descriptor.as_bytes().first().copied().unwrap_or(b'I');
                let value =
                    native_constants[index].unwrap_or_else(|| Value::default_for(descriptor));
                if field.is_static {
                    static_values[field.slot as usize] = value;
                } else {
                    defaults[field.slot as usize] = value;
                }
                index += 1;
            }
            let mut index = 0;
            while index < class.methods.len() {
                class.methods[index].owner = id;
                index += 1;
            }
            class.static_values = static_values;
            class.instance_defaults = defaults;
        }
        id
    }

    // -----------------------------------------------------------------------------------------
    // Arrays and primitives
    // -----------------------------------------------------------------------------------------

    fn resolve_array_class(&mut self, descriptor: &str) -> Result<ClassId, VmError> {
        if let Some(id) = self.classes.by_name(descriptor) {
            return Ok(id);
        }
        let (component, component_class) = self.parse_component(descriptor)?;
        let name = descriptor.to_string();
        let object = ClassId::OBJECT;
        let cloneable = self.resolve_class("java/lang/Cloneable")?;
        let serializable = self.resolve_class("java/io/Serializable")?;
        let id = self.classes.insert(Class {
            id: ClassId(0),
            name,
            super_class: Some(object),
            interfaces: vec![cloneable, serializable],
            access_flags: ACC_PUBLIC | ACC_FINAL | ACC_ABSTRACT,
            state: ClassState::Initialized,
            constant_pool: ConstantPool::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            static_values: Vec::new(),
            instance_defaults: Vec::new(),
            instance_slots: 0,
            source_file: None,
            bootstrap_methods: Vec::new(),
            class_object: None,
            kind: ClassKind::Array,
            component: Some(component),
            component_class,
            primitive: None,
            native_definition: None,
            initialization_error: None,
            nest_host: None,
            permitted_subclasses: Vec::new(),
            is_enum: false,
            verified: true,
        });
        self.classes.get_mut(id).id = id;
        Ok(id)
    }

    fn parse_component(
        &mut self,
        descriptor: &str,
    ) -> Result<(ArrayComponent, Option<ClassId>), VmError> {
        let rest = descriptor
            .strip_prefix('[')
            .ok_or_else(|| VmError::internal("array descriptor must start with '['"))?;
        if let Some(object) = rest.strip_prefix('L').and_then(|s| s.strip_suffix(';')) {
            let component = self.resolve_class(object)?;
            Ok((ArrayComponent::Reference, Some(component)))
        } else if rest.starts_with('[') {
            let component = self.resolve_array_class(rest)?;
            Ok((ArrayComponent::Reference, Some(component)))
        } else {
            let field_type = FieldType::parse(rest)
                .map_err(|message| VmError::invalid_code(descriptor, message))?;
            let component = ArrayComponent::from_field_type(&field_type)
                .ok_or_else(|| VmError::invalid_code(descriptor, "void array component"))?;
            Ok((component, None))
        }
    }

    /// The array class whose component is `component_class`.
    pub fn array_class_for(&mut self, component_class: ClassId) -> Result<ClassId, VmError> {
        let component_name = self.classes.get(component_class).name.clone();
        let descriptor = if component_name.starts_with('[') {
            format!("[{component_name}")
        } else {
            format!("[L{component_name};")
        };
        self.resolve_class(&descriptor)
    }

    /// The array class for a primitive or reference component.
    pub fn array_class_of_component(
        &mut self,
        component: ArrayComponent,
        component_class: Option<ClassId>,
    ) -> Result<ClassId, VmError> {
        if let Some(class) = component_class {
            return self.array_class_for(class);
        }
        let descriptor = format!("[{}", component.descriptor());
        self.resolve_class(&descriptor)
    }

    // -----------------------------------------------------------------------------------------
    // Member lookup and resolution (JVMS 5.4.3)
    // -----------------------------------------------------------------------------------------

    /// Find a field by name and descriptor in a class and its supertypes.
    ///
    /// # Errors
    ///
    /// Throws `NoSuchFieldError` when no such field exists.
    pub fn find_field(
        &mut self,
        class: ClassId,
        name: &str,
        descriptor: &str,
    ) -> Result<(ClassId, u32), VmError> {
        let mut current = Some(class);
        while let Some(id) = current {
            if let Some(index) = self.classes.find_declared_field(id, name, descriptor) {
                return Ok((id, index));
            }
            current = self.classes.get(id).super_class;
        }
        // Interfaces may declare constants.
        for interface in self.classes.get(class).interfaces.clone() {
            if let Ok(found) = self.find_field(interface, name, descriptor) {
                return Ok(found);
            }
        }
        Err(self.throw_new(
            "java/lang/NoSuchFieldError",
            Some(&format!("{name}:{descriptor}")),
        ))
    }

    /// Find a method by name and descriptor walking the class hierarchy and interfaces.
    #[must_use]
    pub fn find_method(
        &self,
        class: ClassId,
        name: &str,
        descriptor: &str,
    ) -> Option<(ClassId, u32)> {
        if let Some(index) = self.classes.find_declared_method(class, name, descriptor) {
            return Some((class, index));
        }
        if let Some(super_class) = self.classes.get(class).super_class {
            if let Some(found) = self.find_method(super_class, name, descriptor) {
                return Some(found);
            }
        }
        for interface in &self.classes.get(class).interfaces {
            if let Some(found) = self.find_method(*interface, name, descriptor) {
                return Some(found);
            }
        }
        None
    }

    /// Resolve a `Methodref` or `InterfaceMethodref` constant (JVMS 5.4.3.3/5.4.3.4).
    ///
    /// # Errors
    ///
    /// Throws `NoSuchMethodError`, `IncompatibleClassChangeError`, or `IllegalAccessError` as the
    /// specification requires.
    pub fn resolve_method_ref(
        &mut self,
        class: ClassId,
        cp_index: u16,
    ) -> Result<ResolvedMethod, VmError> {
        let (target_name, member_name, descriptor, is_interface) = {
            let target_class = self.classes.get(class);
            let pool = &target_class.constant_pool;
            let info = pool.get(cp_index).map_err(|error| {
                VmError::invalid_code(target_class.name.clone(), error.to_string())
            })?;
            match info {
                CpInfo::Methodref {
                    class: class_index,
                    name_and_type,
                } => {
                    let name = pool.class_name(*class_index).map_err(|error| {
                        VmError::invalid_code(target_class.name.clone(), error.to_string())
                    })?;
                    let (member_name, descriptor) =
                        pool.name_and_type(*name_and_type).map_err(|error| {
                            VmError::invalid_code(target_class.name.clone(), error.to_string())
                        })?;
                    (
                        name.to_string(),
                        member_name.to_string(),
                        descriptor.to_string(),
                        false,
                    )
                }
                CpInfo::InterfaceMethodref {
                    class: class_index,
                    name_and_type,
                } => {
                    let name = pool.class_name(*class_index).map_err(|error| {
                        VmError::invalid_code(target_class.name.clone(), error.to_string())
                    })?;
                    let (member_name, descriptor) =
                        pool.name_and_type(*name_and_type).map_err(|error| {
                            VmError::invalid_code(target_class.name.clone(), error.to_string())
                        })?;
                    (
                        name.to_string(),
                        member_name.to_string(),
                        descriptor.to_string(),
                        true,
                    )
                }
                other => {
                    return Err(VmError::invalid_code(
                        self.classes.get(class).name.clone(),
                        format!("method reference names {}", other.kind_name()),
                    ));
                }
            }
        };
        let target = self.resolve_class(&target_name)?;
        let target_is_interface = self.classes.get(target).is_interface();
        if is_interface && !target_is_interface {
            return Err(VmError::internal("interface ref names a class"));
        }
        if !is_interface && target_is_interface && !target_name.starts_with('[') {
            // A Methodref to an interface is only legal for Object methods; treat as interface
            // resolution to be permissive with older toolchains.
        }
        let found = self.find_method(target, &member_name, &descriptor);
        let (declaring, index) = match found {
            Some(found) => found,
            None => {
                return Err(self.throw_new(
                    "java/lang/NoSuchMethodError",
                    Some(&format!("{target_name}.{member_name}{descriptor}")),
                ));
            }
        };
        Ok(ResolvedMethod {
            class: declaring,
            method: index,
            is_interface: target_is_interface,
        })
    }

    /// Resolve a `Fieldref` constant (JVMS 5.4.3.2).
    ///
    /// # Errors
    ///
    /// Throws `NoSuchFieldError` when the field does not exist.
    pub fn resolve_field_ref(
        &mut self,
        class: ClassId,
        cp_index: u16,
    ) -> Result<(ClassId, u32, bool), VmError> {
        let (target_name, member_name, descriptor) = {
            let target_class = self.classes.get(class);
            let pool = &target_class.constant_pool;
            let info = pool.get(cp_index).map_err(|error| {
                VmError::invalid_code(target_class.name.clone(), error.to_string())
            })?;
            match info {
                CpInfo::Fieldref {
                    class: class_index,
                    name_and_type,
                } => {
                    let name = pool.class_name(*class_index).map_err(|error| {
                        VmError::invalid_code(target_class.name.clone(), error.to_string())
                    })?;
                    let (member_name, descriptor) =
                        pool.name_and_type(*name_and_type).map_err(|error| {
                            VmError::invalid_code(target_class.name.clone(), error.to_string())
                        })?;
                    (
                        name.to_string(),
                        member_name.to_string(),
                        descriptor.to_string(),
                    )
                }
                other => {
                    return Err(VmError::invalid_code(
                        target_class.name.clone(),
                        format!("field reference names {}", other.kind_name()),
                    ));
                }
            }
        };
        let target = self.resolve_class(&target_name)?;
        let (declaring, index) = self.find_field(target, &member_name, &descriptor)?;
        let is_static = self.classes.get(declaring).fields[index as usize].is_static;
        Ok((declaring, index, is_static))
    }

    /// Select the method invoked by `invokevirtual`/`invokeinterface` on a receiver.
    ///
    /// # Errors
    ///
    /// Throws `AbstractMethodError` when the selected method has no body, and
    /// `NoSuchMethodError` when none is found.
    pub fn resolve_virtual_method(
        &mut self,
        receiver: ObjectRef,
        name: &str,
        descriptor: &str,
    ) -> Result<(ClassId, u32), VmError> {
        if receiver.is_null() {
            return Err(self.throw_new(
                "java/lang/NullPointerException",
                Some(&format!(
                    "Cannot invoke \"{name}{descriptor}\" because the receiver is null"
                )),
            ));
        }
        let class = self.class_of(receiver);
        let found = self.find_method(class, name, descriptor).ok_or_else(|| {
            let class_name = self.class_name(class).to_string();
            self.throw_new(
                "java/lang/NoSuchMethodError",
                Some(&format!("{class_name}.{name}{descriptor}")),
            )
        })?;
        Ok(found)
    }

    // -----------------------------------------------------------------------------------------
    // Initialization (JVMS 5.5)
    // -----------------------------------------------------------------------------------------

    /// Ensure a class is initialized.
    ///
    /// # Errors
    ///
    /// Throws `ExceptionInInitializerError`/`NoClassDefFoundError` exactly as JVMS 5.5 prescribes.
    pub fn ensure_initialized(&mut self, class: ClassId) -> Result<(), VmError> {
        match self.classes.get(class).state {
            ClassState::Initialized | ClassState::Initializing => Ok(()),
            ClassState::Errored => {
                if let Some(exception) = self.classes.get(class).initialization_error {
                    Err(VmError::Thrown(exception))
                } else {
                    Err(self.throw_new("java/lang/NoClassDefFoundError", None))
                }
            }
            ClassState::Prepared => self.initialize(class),
        }
    }

    fn initialize(&mut self, class: ClassId) -> Result<(), VmError> {
        if let Some(super_class) = self.classes.get(class).super_class {
            if let Err(error) = self.ensure_initialized(super_class) {
                self.classes.get_mut(class).state = ClassState::Errored;
                if let VmError::Thrown(exception) = &error {
                    self.classes.get_mut(class).initialization_error = Some(*exception);
                }
                return Err(error);
            }
        }
        // Superinterfaces declaring default methods are initialized first (JVMS 5.5).
        for interface in self.classes.get(class).interfaces.clone() {
            if self.interface_has_default_methods(interface) {
                self.ensure_initialized(interface)?;
            }
        }
        self.classes.get_mut(class).state = ClassState::Initializing;
        let hook = self
            .classes
            .get(class)
            .native_definition
            .and_then(|definition| definition.clinit);
        let result = if let Some(hook) = hook {
            hook(self)
        } else if let Some((declaring, index)) = self
            .classes
            .find_declared_method(class, "<clinit>", "()V")
            .map(|index| (class, index))
        {
            self.invoke_method(declaring, index, Vec::new()).map(|_| ())
        } else {
            Ok(())
        };
        match result {
            Ok(()) => {
                self.classes.get_mut(class).state = ClassState::Initialized;
                Ok(())
            }
            Err(error) => {
                self.classes.get_mut(class).state = ClassState::Errored;
                let thrown = match error {
                    VmError::Thrown(exception) => {
                        self.classes.get_mut(class).initialization_error = Some(exception);
                        let error_class = self.error_class();
                        if self.is_instance(exception, error_class) {
                            exception
                        } else {
                            let wrapper =
                                self.throw_new("java/lang/ExceptionInInitializerError", None);
                            if let VmError::Thrown(wrapper) = wrapper {
                                wrapper
                            } else {
                                exception
                            }
                        }
                    }
                    other => {
                        self.classes.get_mut(class).initialization_error = None;
                        return Err(other);
                    }
                };
                Err(VmError::Thrown(thrown))
            }
        }
    }

    fn error_class(&mut self) -> ClassId {
        self.resolve_class("java/lang/Error")
            .unwrap_or(ClassId::OBJECT)
    }

    fn interface_has_default_methods(&self, interface: ClassId) -> bool {
        let class = self.classes.get(interface);
        class.methods.iter().any(|method| {
            !method.is_static()
                && method.access_flags & ACC_ABSTRACT == 0
                && method.access_flags & ACC_PRIVATE == 0
                && method.name != "<clinit>"
                && !method.is_abstract()
        })
    }

    // -----------------------------------------------------------------------------------------
    // Instances and field access helpers
    // -----------------------------------------------------------------------------------------

    /// Allocate an instance of an initialized class with default field values.
    pub fn new_instance(&mut self, class: ClassId) -> Result<ObjectRef, VmError> {
        let defaults = self.classes.get(class).instance_defaults.clone();
        self.maybe_gc();
        Ok(self.heap.allocate(class, ObjectData::Instance(defaults)))
    }

    /// Write an instance reference field.
    pub fn set_instance_ref(
        &mut self,
        object: ObjectRef,
        class: ClassId,
        field: u32,
        value: ObjectRef,
    ) {
        let slot = self.classes.get(class).fields[field as usize].slot as usize;
        if let Some(ObjectData::Instance(fields)) =
            self.heap.get_mut(object).map(|object| &mut object.data)
        {
            fields[slot] = Value::Ref(value);
        }
    }

    /// Write an instance `int` field.
    pub fn set_instance_int(&mut self, object: ObjectRef, class: ClassId, field: u32, value: i32) {
        let slot = self.classes.get(class).fields[field as usize].slot as usize;
        if let Some(ObjectData::Instance(fields)) =
            self.heap.get_mut(object).map(|object| &mut object.data)
        {
            fields[slot] = Value::Int(value);
        }
    }

    /// Write an instance `boolean` field.
    pub fn set_boolean_field(
        &mut self,
        object: ObjectRef,
        class: ClassId,
        name: &str,
        value: bool,
    ) -> Result<(), VmError> {
        let (declaring, field) = self.find_field(class, name, "Z")?;
        self.set_instance_int(object, declaring, field, i32::from(value));
        Ok(())
    }

    /// Write an instance `String` field by name.
    pub fn set_string_field(
        &mut self,
        object: ObjectRef,
        class: ClassId,
        name: &str,
        value: &str,
    ) -> Result<(), VmError> {
        let (declaring, field) = self.find_field(class, name, "Ljava/lang/String;")?;
        let string = self.intern(value);
        self.set_instance_ref(object, declaring, field, string);
        Ok(())
    }

    /// Read an instance `int` field.
    #[must_use]
    pub fn read_instance_int(&self, object: ObjectRef, class: ClassId, field: u32) -> Option<i32> {
        let slot = self.classes.get(class).fields[field as usize].slot as usize;
        match self.heap.get(object)?.data {
            ObjectData::Instance(ref fields) => Some(fields[slot].as_int()),
            _ => None,
        }
    }

    /// Read an instance `int` field by name and descriptor.
    #[must_use]
    pub fn read_instance_int_named(
        &self,
        object: ObjectRef,
        class: ClassId,
        name: &str,
        descriptor: &str,
    ) -> Option<i32> {
        let (declaring, field) = self.find_method_field(class, name, descriptor)?;
        self.read_instance_int(object, declaring, field)
    }

    fn find_method_field(
        &self,
        class: ClassId,
        name: &str,
        descriptor: &str,
    ) -> Option<(ClassId, u32)> {
        let mut current = Some(class);
        while let Some(id) = current {
            if let Some(index) = self.classes.find_declared_field(id, name, descriptor) {
                return Some((id, index));
            }
            current = self.classes.get(id).super_class;
        }
        None
    }

    /// Read an instance reference field by name and descriptor.
    #[must_use]
    pub fn read_ref_field(
        &self,
        object: ObjectRef,
        class: ClassId,
        name: &str,
        descriptor: &str,
    ) -> Option<ObjectRef> {
        let (declaring, field) = self.find_method_field(class, name, descriptor)?;
        let slot = self.classes.get(declaring).fields[field as usize].slot as usize;
        match self.heap.get(object)?.data {
            ObjectData::Instance(ref fields) => Some(fields[slot].as_ref()),
            _ => None,
        }
    }

    /// Read a `String` instance field by class.
    #[must_use]
    pub fn read_string_field(
        &self,
        object: ObjectRef,
        class: ClassId,
        name: &str,
    ) -> Option<String> {
        let reference = self.read_ref_field(object, class, name, "Ljava/lang/String;")?;
        self.string_value(reference)
    }

    /// Read a `String` instance field, searching the class hierarchy for a field with that name.
    #[must_use]
    pub fn read_string_field_any(&self, object: ObjectRef, name: &str) -> Option<String> {
        if object.is_null() {
            return None;
        }
        let mut class = self.class_of(object);
        loop {
            if let Some(reference) = self.read_ref_field(object, class, name, "Ljava/lang/String;")
            {
                return self.string_value(reference);
            }
            class = self.classes.get(class).super_class?;
        }
    }

    /// Read a reference field anywhere in the hierarchy.
    #[must_use]
    pub fn read_ref_field_any(&self, object: ObjectRef, name: &str) -> Option<ObjectRef> {
        if object.is_null() {
            return None;
        }
        let mut class = self.class_of(object);
        loop {
            let mut found = None;
            for field in &self.classes.get(class).fields {
                if field.name == name && field.descriptor.starts_with('L') {
                    found = Some(field.slot as usize);
                    break;
                }
            }
            if let Some(slot) = found {
                if let ObjectData::Instance(ref fields) = self.heap.get(object)?.data {
                    return Some(fields[slot].as_ref());
                }
            }
            class = self.classes.get(class).super_class?;
        }
    }

    /// Read a static reference field on an initialized class.
    #[must_use]
    pub fn read_static_ref(
        &self,
        class: ClassId,
        name: &str,
        descriptor: &str,
    ) -> Option<ObjectRef> {
        let (declaring, field) = self.find_method_field(class, name, descriptor)?;
        let slot = self.classes.get(declaring).fields[field as usize].slot as usize;
        match self.classes.get(declaring).static_values[slot] {
            Value::Ref(reference) => Some(reference),
            _ => None,
        }
    }

    /// Write a static field with an owned value.
    pub fn set_static_value(&mut self, class: ClassId, name: &str, descriptor: &str, value: Value) {
        if let Some((declaring, field)) = self.find_method_field(class, name, descriptor) {
            let slot = self.classes.get(declaring).fields[field as usize].slot as usize;
            self.classes.get_mut(declaring).static_values[slot] = value;
        }
    }

    // -----------------------------------------------------------------------------------------
    // Dynamic call sites (invokedynamic / condy)
    // -----------------------------------------------------------------------------------------

    /// Resolve an `invokedynamic` constant to a call site.
    ///
    /// # Errors
    ///
    /// Throws `BootstrapMethodError` for unsupported bootstrap methods.
    pub(crate) fn resolve_call_site(
        &mut self,
        class: ClassId,
        cp_index: u16,
    ) -> Result<CallSite, VmError> {
        if let Some(site) = self.call_sites.get(&(class.raw(), cp_index)) {
            return Ok(site.clone());
        }
        let (bootstrap_index, name, descriptor) = {
            let target = self.classes.get(class);
            let pool = &target.constant_pool;
            let info = pool
                .get(cp_index)
                .map_err(|error| VmError::invalid_code(target.name.clone(), error.to_string()))?;
            match info {
                CpInfo::InvokeDynamic {
                    bootstrap_method_attr_index,
                    name_and_type,
                } => {
                    let (name, descriptor) =
                        pool.name_and_type(*name_and_type).map_err(|error| {
                            VmError::invalid_code(target.name.clone(), error.to_string())
                        })?;
                    (
                        *bootstrap_method_attr_index,
                        name.to_string(),
                        descriptor.to_string(),
                    )
                }
                other => {
                    return Err(VmError::invalid_code(
                        target.name.clone(),
                        format!("invokedynamic names {}", other.kind_name()),
                    ));
                }
            }
        };
        let site = self.build_call_site(class, bootstrap_index, &name, &descriptor)?;
        self.call_sites
            .insert((class.raw(), cp_index), site.clone());
        Ok(site)
    }

    fn build_call_site(
        &mut self,
        class: ClassId,
        bootstrap_index: u16,
        name: &str,
        descriptor: &str,
    ) -> Result<CallSite, VmError> {
        let bootstrap = {
            let target = self.classes.get(class);
            match target.bootstrap_methods.get(bootstrap_index as usize) {
                Some(crate::class::BootstrapMethod::Unresolved {
                    method_ref,
                    arguments,
                }) => (*method_ref, arguments.clone()),
                None => {
                    return Err(self.throw_new(
                        "java/lang/BootstrapMethodError",
                        Some(&format!("missing bootstrap method {bootstrap_index}")),
                    ));
                }
            }
        };
        let (bootstrap_class, bootstrap_name, bootstrap_descriptor) =
            self.method_handle_identity(class, bootstrap.0)?;
        match (bootstrap_class.as_str(), bootstrap_name.as_str()) {
            ("java/lang/invoke/LambdaMetafactory", "metafactory")
            | ("java/lang/invoke/LambdaMetafactory", "altMetafactory") => self.build_lambda_site(
                class,
                name,
                descriptor,
                &bootstrap.1,
                bootstrap_name.as_str(),
            ),
            ("java/lang/invoke/StringConcatFactory", "makeConcat") => {
                let _ = bootstrap_descriptor;
                self.build_concat_site(class, &bootstrap.1, false)
            }
            ("java/lang/invoke/StringConcatFactory", "makeConcatWithConstants") => {
                self.build_concat_site(class, &bootstrap.1, true)
            }
            _ => Err(self.throw_new(
                "java/lang/BootstrapMethodError",
                Some(&format!(
                    "unsupported bootstrap method {bootstrap_class}.{bootstrap_name}"
                )),
            )),
        }
    }

    fn method_handle_identity(
        &mut self,
        class: ClassId,
        cp_index: u16,
    ) -> Result<(String, String, String), VmError> {
        let (kind, reference) = {
            let target = self.classes.get(class);
            match target.constant_pool.get(cp_index) {
                Ok(CpInfo::MethodHandle {
                    reference_kind,
                    reference_index,
                }) => (*reference_kind, *reference_index),
                Ok(other) => {
                    return Err(VmError::invalid_code(
                        target.name.clone(),
                        format!("bootstrap method names {}", other.kind_name()),
                    ));
                }
                Err(error) => {
                    return Err(VmError::invalid_code(
                        target.name.clone(),
                        error.to_string(),
                    ));
                }
            }
        };
        let _ = kind;
        let resolved = self.resolve_method_ref(class, reference)?;
        let target_class = self.classes.get(resolved.class);
        Ok((
            target_class.name.clone(),
            target_class.methods[resolved.method as usize].name.clone(),
            target_class.methods[resolved.method as usize]
                .descriptor
                .clone(),
        ))
    }

    fn build_concat_site(
        &mut self,
        site_class: ClassId,
        arguments: &[u16],
        with_constants: bool,
    ) -> Result<CallSite, VmError> {
        if arguments.is_empty() {
            return Err(self.throw_new(
                "java/lang/BootstrapMethodError",
                Some("StringConcatFactory bootstrap has no recipe"),
            ));
        }
        // The bootstrapping class owns the constant pool: bootstrap argument indices are indices
        // into the pool of the class that declares the `invokedynamic`.
        let recipe = match self.classes.get(site_class).constant_pool.get(arguments[0]) {
            Ok(CpInfo::String(index)) => self
                .classes
                .get(site_class)
                .constant_pool
                .utf8(*index)
                .ok()
                .map(str::to_string),
            Ok(CpInfo::Utf8(text)) => Some(text.clone()),
            _ => None,
        }
        .ok_or_else(|| {
            self.throw_new(
                "java/lang/BootstrapMethodError",
                Some("StringConcatFactory recipe is not a String"),
            )
        })?;
        let mut constants = Vec::new();
        for index in arguments.iter().skip(1) {
            let value = match self.classes.get(site_class).constant_pool.get(*index) {
                Ok(CpInfo::String(string_index)) => self
                    .classes
                    .get(site_class)
                    .constant_pool
                    .utf8(*string_index)
                    .ok()
                    .map(|text| ConcatConstant::Str(text.to_string())),
                Ok(CpInfo::Integer(value)) => Some(ConcatConstant::Int(*value)),
                Ok(CpInfo::Long(value)) => Some(ConcatConstant::Long(*value)),
                Ok(CpInfo::Float(bits)) => Some(ConcatConstant::Float(f32::from_bits(*bits))),
                Ok(CpInfo::Double(bits)) => Some(ConcatConstant::Double(f64::from_bits(*bits))),
                _ => None,
            };
            if let Some(value) = value {
                constants.push(value);
            }
        }
        let _ = with_constants;
        Ok(CallSite::Concat { recipe, constants })
    }

    fn build_lambda_site(
        &mut self,
        class: ClassId,
        name: &str,
        descriptor: &str,
        arguments: &[u16],
        _bootstrap_name: &str,
    ) -> Result<CallSite, VmError> {
        // metafactory(Lookup, String, MethodType samMethodType, MethodHandle implMethod,
        //             MethodType instantiatedMethodType[, flags, markers...])
        if arguments.len() < 3 {
            return Err(self.throw_new(
                "java/lang/BootstrapMethodError",
                Some("LambdaMetafactory bootstrap missing arguments"),
            ));
        }
        let sam_descriptor = self.method_type_argument(class, arguments[0])?;
        let target = self.method_handle_value(class, arguments[1])?;
        let _instantiated = self.method_type_argument(class, arguments[2])?;
        // The functional interface is the return type of the indy descriptor.
        let indy = MethodDescriptor::parse(descriptor).map_err(|message| {
            VmError::invalid_code(self.class_name(class).to_string(), message)
        })?;
        let interface_name = match indy.returns {
            Some(FieldType::Object(ref name)) => name.clone(),
            _ => {
                return Err(self.throw_new(
                    "java/lang/BootstrapMethodError",
                    Some("lambda call site does not return an interface"),
                ));
            }
        };
        let interface = self.resolve_class(&interface_name)?;

        // Captured argument descriptors come from the indy descriptor's parameters.
        let captured: Vec<FieldType> = indy.parameters.clone();
        let captured_descriptors: Vec<String> =
            captured.iter().map(FieldType::descriptor).collect();

        let lambda_index = self.lambda_defs.len() + 1;
        let simple_name = {
            let full = self.class_name(class);
            let base = full.rsplit('/').next().unwrap_or(full);
            alloc::format!("{base}$$Lambda${lambda_index}")
        };

        // Instance fields for the captured values, plus the SAM.
        let mut instance_slot = 0u16;
        let mut fields = Vec::new();
        let mut captured_slots = Vec::new();
        for (index, field_type) in captured.iter().enumerate() {
            let slot = instance_slot;
            instance_slot += field_type.slots();
            captured_slots.push(slot);
            fields.push(Field {
                name: alloc::format!("arg${index}"),
                descriptor: field_type.descriptor(),
                access_flags: ACC_PRIVATE | ACC_FINAL | ACC_SYNTHETIC,
                owner: ClassId(0),
                is_static: false,
                slot,
                constant: None,
            });
        }
        let constructor_descriptor = alloc::format!("({})V", captured_descriptors.join(""));
        let constructor = Method {
            name: "<init>".into(),
            descriptor: constructor_descriptor,
            access_flags: ACC_PRIVATE,
            owner: ClassId(0),
            code: None,
            native: Some(lambda_constructor_native),
            exceptions: Vec::new(),
            parameter_names: Vec::new(),
        };
        let sam = Method {
            name: name.into(),
            descriptor: sam_descriptor.clone(),
            access_flags: ACC_PUBLIC,
            owner: ClassId(0),
            code: None,
            native: Some(lambda_invoke_native),
            exceptions: Vec::new(),
            parameter_names: Vec::new(),
        };
        let mut methods = vec![constructor, sam];
        // Bridge methods for erased generics would normally be generated by javac; the SAM
        // descriptor given to the metafactory already matches the call site, so no bridge is
        // needed here. `equals`/`hashCode`/`toString` inherit from Object.

        // Sort methods so `<init>` is first (not required, but stable for diagnostics).
        methods.sort_by(|a, b| a.name.cmp(&b.name));

        let id = self.classes.insert(Class {
            id: ClassId(0),
            name: simple_name,
            super_class: Some(ClassId::OBJECT),
            interfaces: vec![interface],
            access_flags: ACC_FINAL | ACC_SUPER | ACC_SYNTHETIC,
            state: ClassState::Initialized,
            constant_pool: ConstantPool::new(),
            fields,
            methods,
            static_values: Vec::new(),
            instance_defaults: Vec::new(),
            instance_slots: instance_slot,
            source_file: None,
            bootstrap_methods: Vec::new(),
            class_object: None,
            kind: ClassKind::Class,
            component: None,
            component_class: None,
            primitive: None,
            native_definition: None,
            initialization_error: None,
            nest_host: None,
            permitted_subclasses: Vec::new(),
            is_enum: false,
            verified: true,
        });
        {
            let class = self.classes.get_mut(id);
            class.id = id;
            class.instance_defaults = vec![Value::Int(0); class.instance_slots as usize];
            for field in &mut class.fields {
                field.owner = id;
            }
            for method in &mut class.methods {
                method.owner = id;
            }
        }
        self.lambda_defs.insert(
            id,
            LambdaDef {
                target,
                sam_name: name.into(),
                sam_descriptor,
                captured_slots,
            },
        );
        Ok(CallSite::Lambda { class: id })
    }

    fn method_type_argument(&mut self, class: ClassId, cp_index: u16) -> Result<String, VmError> {
        let pool = &self.classes.get(class).constant_pool;
        match pool.get(cp_index) {
            Ok(CpInfo::MethodType(index)) => {
                pool.utf8(*index).map(str::to_string).map_err(|error| {
                    VmError::invalid_code(self.classes.get(class).name.clone(), error.to_string())
                })
            }
            Ok(other) => Err(VmError::invalid_code(
                self.classes.get(class).name.clone(),
                format!("expected MethodType, found {}", other.kind_name()),
            )),
            Err(error) => Err(VmError::invalid_code(
                self.classes.get(class).name.clone(),
                error.to_string(),
            )),
        }
    }

    /// Resolve a `CONSTANT_MethodHandle` into a runtime method handle value.
    ///
    /// # Errors
    ///
    /// Fails when the referenced member cannot be resolved.
    pub fn method_handle_value(
        &mut self,
        class: ClassId,
        cp_index: u16,
    ) -> Result<MethodHandleValue, VmError> {
        let (kind, reference) = {
            let target = self.classes.get(class);
            match target.constant_pool.get(cp_index) {
                Ok(CpInfo::MethodHandle {
                    reference_kind,
                    reference_index,
                }) => (*reference_kind, *reference_index),
                Ok(other) => {
                    return Err(VmError::invalid_code(
                        target.name.clone(),
                        format!("expected MethodHandle, found {}", other.kind_name()),
                    ));
                }
                Err(error) => {
                    return Err(VmError::invalid_code(
                        target.name.clone(),
                        error.to_string(),
                    ));
                }
            }
        };
        Ok(match kind {
            1 => {
                let (field_class, field, _) = self.resolve_field_ref(class, reference)?;
                MethodHandleValue::Getter {
                    class: field_class,
                    field,
                }
            }
            2 => {
                let (field_class, field, _) = self.resolve_field_ref(class, reference)?;
                MethodHandleValue::StaticGetter {
                    class: field_class,
                    field,
                }
            }
            3 => {
                let (field_class, field, _) = self.resolve_field_ref(class, reference)?;
                MethodHandleValue::Setter {
                    class: field_class,
                    field,
                }
            }
            4 => {
                let (field_class, field, _) = self.resolve_field_ref(class, reference)?;
                MethodHandleValue::StaticSetter {
                    class: field_class,
                    field,
                }
            }
            5 | 9 => {
                let resolved = self.resolve_method_ref(class, reference)?;
                MethodHandleValue::Virtual {
                    class: resolved.class,
                    method: resolved.method,
                }
            }
            6 => {
                let resolved = self.resolve_method_ref(class, reference)?;
                MethodHandleValue::Static {
                    class: resolved.class,
                    method: resolved.method,
                }
            }
            7 => {
                let resolved = self.resolve_method_ref(class, reference)?;
                MethodHandleValue::Special {
                    class: resolved.class,
                    method: resolved.method,
                }
            }
            8 => {
                let resolved = self.resolve_method_ref(class, reference)?;
                MethodHandleValue::New {
                    class: resolved.class,
                }
            }
            other => {
                return Err(VmError::invalid_code(
                    self.classes.get(class).name.clone(),
                    format!("invalid method handle kind {other}"),
                ));
            }
        })
    }

    /// Invoke a resolved method handle value.
    ///
    /// # Errors
    ///
    /// Propagates whatever the target throws.
    pub fn invoke_handle(
        &mut self,
        handle: &MethodHandleValue,
        args: &[Value],
    ) -> Result<Value, VmError> {
        match handle {
            MethodHandleValue::Static { class, method } => {
                self.invoke_method(*class, *method, args.to_vec())
            }
            MethodHandleValue::Virtual { class, method } => {
                let receiver = args.first().copied().unwrap_or(Value::Ref(ObjectRef::NULL));
                if receiver.is_null_ref() {
                    return Err(self.throw_new("java/lang/NullPointerException", None));
                }
                let target_class = self.class_of(receiver.as_ref());
                let name = self.classes.get(*class).methods[*method as usize]
                    .name
                    .clone();
                let descriptor = self.classes.get(*class).methods[*method as usize]
                    .descriptor
                    .clone();
                let (declaring, index) = self
                    .find_method(target_class, &name, &descriptor)
                    .unwrap_or((*class, *method));
                self.invoke_method(declaring, index, args.to_vec())
            }
            MethodHandleValue::Special { class, method } => {
                self.invoke_method(*class, *method, args.to_vec())
            }
            MethodHandleValue::New { class } => {
                let object = self.new_instance(*class)?;
                let descriptor = {
                    let mut text = String::from("(");
                    for value in args {
                        text.push_str(&descriptor_of_value(self, value));
                    }
                    text.push_str(")V");
                    text
                };
                let (declaring, constructor) = self
                    .find_method(*class, "<init>", &descriptor)
                    .ok_or_else(|| self.throw_new("java/lang/NoSuchMethodError", Some("<init>")))?;
                let mut call_args = vec![Value::Ref(object)];
                call_args.extend_from_slice(args);
                self.invoke_method(declaring, constructor, call_args)?;
                Ok(Value::Ref(object))
            }
            MethodHandleValue::StaticGetter { class, field } => {
                let slot = self.classes.get(*class).fields[*field as usize].slot as usize;
                Ok(self.classes.get(*class).static_values[slot])
            }
            MethodHandleValue::StaticSetter { class, field } => {
                let slot = self.classes.get(*class).fields[*field as usize].slot as usize;
                self.classes.get_mut(*class).static_values[slot] =
                    args.first().copied().unwrap_or(Value::Int(0));
                Ok(Value::Int(0))
            }
            MethodHandleValue::Getter { class, field } => {
                let receiver = args.first().copied().unwrap_or(Value::Ref(ObjectRef::NULL));
                let slot = self.classes.get(*class).fields[*field as usize].slot as usize;
                match self.heap.get(receiver.as_ref()).map(|object| &object.data) {
                    Some(ObjectData::Instance(fields)) => Ok(fields[slot]),
                    _ => Err(self.throw_new("java/lang/NullPointerException", None)),
                }
            }
            MethodHandleValue::Setter { class, field } => {
                let receiver = args.first().copied().unwrap_or(Value::Ref(ObjectRef::NULL));
                let value = args.get(1).copied().unwrap_or(Value::Int(0));
                let slot = self.classes.get(*class).fields[*field as usize].slot as usize;
                match self
                    .heap
                    .get_mut(receiver.as_ref())
                    .map(|object| &mut object.data)
                {
                    Some(ObjectData::Instance(fields)) => {
                        fields[slot] = value;
                        Ok(Value::Int(0))
                    }
                    _ => Err(self.throw_new("java/lang/NullPointerException", None)),
                }
            }
            MethodHandleValue::Bound { receiver, target } => {
                let mut call_args = vec![Value::Ref(*receiver)];
                call_args.extend_from_slice(args);
                self.invoke_handle(target, &call_args)
            }
            MethodHandleValue::Identity { class } => {
                let value = args.first().copied().unwrap_or(Value::Ref(ObjectRef::NULL));
                if value.is_null_ref() || self.is_instance(value.as_ref(), *class) {
                    Ok(value)
                } else {
                    Err(self.throw_new("java/lang/ClassCastException", None))
                }
            }
        }
    }
}

/// The descriptor of a runtime value, used to build `<init>` descriptors for reflective calls.
pub(crate) fn descriptor_of_value(vm: &Vm<'_>, value: &Value) -> String {
    match value {
        Value::Int(_) => "I".into(),
        Value::Long(_) => "J".into(),
        Value::Float(_) => "F".into(),
        Value::Double(_) => "D".into(),
        Value::Ref(reference) => {
            if reference.is_null() {
                "Ljava/lang/Object;".into()
            } else {
                let name = vm.class_name(vm.class_of(*reference));
                if name.starts_with('[') {
                    name.into()
                } else {
                    alloc::format!("L{name};")
                }
            }
        }
    }
}

/// A lambda constructor: store captured arguments into their fields.
fn lambda_constructor_native(
    vm: &mut Vm<'_>,
    _context: crate::class::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let receiver = args[0].as_ref();
    let class = vm.class_of(receiver);
    let Some(definition) = vm.lambda_defs.get(&class) else {
        return Err(VmError::internal(
            "lambda constructor on an unregistered class",
        ));
    };
    let slots = definition.captured_slots.clone();
    if slots.len() + 1 != args.len() {
        return Err(VmError::internal("lambda constructor arity mismatch"));
    }
    if let Some(ObjectData::Instance(fields)) =
        vm.heap.get_mut(receiver).map(|object| &mut object.data)
    {
        for (index, slot) in slots.iter().enumerate() {
            fields[*slot as usize] = args[index + 1];
        }
    }
    Ok(Value::Int(0))
}

/// A lambda SAM body: capture the receiver plus invocation arguments, then invoke the target.
fn lambda_invoke_native(
    vm: &mut Vm<'_>,
    _context: crate::class::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let receiver = args[0].as_ref();
    let class = vm.class_of(receiver);
    let Some(definition) = vm.lambda_defs.get(&class) else {
        return Err(VmError::internal(
            "lambda invocation on an unregistered class",
        ));
    };
    let target = definition.target.clone();
    let slots = definition.captured_slots.clone();
    let mut call_args = Vec::with_capacity(slots.len() + args.len());
    if let Some(ObjectData::Instance(fields)) = vm.heap.get(receiver).map(|object| &object.data) {
        for slot in &slots {
            call_args.push(fields[*slot as usize]);
        }
    }
    call_args.extend_from_slice(&args[1..]);
    vm.invoke_handle(&target, &call_args)
}

/// A set of class names in the process of being loaded.
pub type LoadingSet = BTreeSet<String>;
