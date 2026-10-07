//! `java.util`: `Objects`, `Arrays`, `Random`, and the core collections.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ArrayComponent, ArrayData, ObjectData, ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, int, long, method, object, receiver, ref_arg, void};

// -------------------------------------------------------------------------------------------
// NoSuchElementException + interfaces
// -------------------------------------------------------------------------------------------

pub(crate) const NO_SUCH_ELEMENT_EXCEPTION: oxjvm_vm::NativeClass = {
    const METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
        method(
            "<init>",
            "()V",
            ACC_PUBLIC,
            crate::throwable::exception_ctor,
        ),
        method(
            "<init>",
            "(Ljava/lang/String;)V",
            ACC_PUBLIC,
            crate::throwable::exception_ctor,
        ),
        method(
            "<init>",
            "(Ljava/lang/String;Ljava/lang/Throwable;)V",
            ACC_PUBLIC,
            crate::throwable::exception_ctor,
        ),
        method(
            "<init>",
            "(Ljava/lang/Throwable;)V",
            ACC_PUBLIC,
            crate::throwable::exception_ctor,
        ),
    ];
    class(
        "java/util/NoSuchElementException",
        Some("java/lang/RuntimeException"),
        &[],
        ACC_PUBLIC,
        &[],
        &METHODS,
        None,
    )
};

const ITERATOR_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method(
        "hasNext",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "next",
        "()Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method("remove", "()V", ACC_PUBLIC, |_, _, _| void()),
];

pub(crate) const ITERATOR: oxjvm_vm::NativeClass = class(
    "java/util/Iterator",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &ITERATOR_METHODS,
    None,
);

const COLLECTION_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "size",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isEmpty",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "add",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "contains",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "clear",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const COLLECTION: oxjvm_vm::NativeClass = class(
    "java/util/Collection",
    Some("java/lang/Object"),
    &["java/lang/Iterable"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &COLLECTION_METHODS,
    None,
);

const LIST_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "size",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isEmpty",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "get",
        "(I)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "add",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "remove",
        "(I)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "contains",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "indexOf",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "clear",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const LIST: oxjvm_vm::NativeClass = class(
    "java/util/List",
    Some("java/lang/Object"),
    &["java/util/Collection"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &LIST_METHODS,
    None,
);

pub(crate) const SET: oxjvm_vm::NativeClass = class(
    "java/util/Set",
    Some("java/lang/Object"),
    &["java/util/Collection"],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[],
    None,
);

const MAP_METHODS: [oxjvm_vm::NativeMethodDef; 8] = [
    method(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "containsKey",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "size",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "isEmpty",
        "()Z",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "clear",
        "()V",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "keySet",
        "()Ljava/util/Set;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const MAP: oxjvm_vm::NativeClass = class(
    "java/util/Map",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &MAP_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Objects
// -------------------------------------------------------------------------------------------

const OBJECTS_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "equals",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let a = ref_arg(args, 0);
            let b = ref_arg(args, 1);
            boolean(objects_equals(vm, a, b)?)
        },
    ),
    method(
        "hashCode",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let reference = ref_arg(args, 0);
            if reference.is_null() {
                int(0)
            } else {
                let (class, method) = vm.resolve_virtual_method(reference, "hashCode", "()I")?;
                vm.invoke_method(class, method, vec![Value::Ref(reference)])
            }
        },
    ),
    method(
        "hash",
        "([Ljava/lang/Object;)I",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let array = ref_arg(args, 0);
            if array.is_null() {
                return int(1);
            }
            let elements: Vec<ObjectRef> = match vm.heap.get(array).map(|object| &object.data) {
                Some(ObjectData::Array(ArrayData::Reference(elements))) => elements.clone(),
                _ => Vec::new(),
            };
            let mut result = 1i32;
            for element in elements {
                let hash = if element.is_null() {
                    0
                } else {
                    let (class, method) = vm.resolve_virtual_method(element, "hashCode", "()I")?;
                    vm.invoke_method(class, method, vec![Value::Ref(element)])?
                        .as_int()
                };
                result = result.wrapping_mul(31).wrapping_add(hash);
            }
            int(result)
        },
    ),
    method(
        "toString",
        "(Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = vm.display_value(
                &oxjvm_classfile::FieldType::Object("java/lang/Object".into()),
                args[0],
            )?;
            object(vm.make_string(&text)?)
        },
    ),
    method(
        "toString",
        "(Ljava/lang/Object;Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let reference = ref_arg(args, 0);
            if reference.is_null() {
                return object(vm.make_string(&vm.string_or_empty(ref_arg(args, 1)))?);
            }
            let (class, method) =
                vm.resolve_virtual_method(reference, "toString", "()Ljava/lang/String;")?;
            vm.invoke_method(class, method, vec![Value::Ref(reference)])
        },
    ),
    method(
        "requireNonNull",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let reference = ref_arg(args, 0);
            if reference.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            object(reference)
        },
    ),
    method(
        "requireNonNull",
        "(Ljava/lang/Object;Ljava/lang/String;)Ljava/lang/Object;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let reference = ref_arg(args, 0);
            if reference.is_null() {
                return Err(vm.throw_new(
                    "java/lang/NullPointerException",
                    Some(&vm.string_or_empty(ref_arg(args, 1))),
                ));
            }
            object(reference)
        },
    ),
    method(
        "isNull",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| boolean(ref_arg(args, 0).is_null()),
    ),
    method(
        "nonNull",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |_, _, args| boolean(!ref_arg(args, 0).is_null()),
    ),
];

fn objects_equals(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    if a == b {
        return Ok(true);
    }
    if a.is_null() || b.is_null() {
        return Ok(false);
    }
    let (class, method) = vm.resolve_virtual_method(a, "equals", "(Ljava/lang/Object;)Z")?;
    let result = vm.invoke_method(class, method, vec![Value::Ref(a), Value::Ref(b)])?;
    Ok(result.as_int() != 0)
}

pub(crate) const OBJECTS: oxjvm_vm::NativeClass = class(
    "java/util/Objects",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &OBJECTS_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Arrays
// -------------------------------------------------------------------------------------------

const ARRAYS_METHODS: [oxjvm_vm::NativeMethodDef; 12] = [
    method(
        "toString",
        "([I)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| arrays_to_string(vm, ref_arg(args, 0), ArrayComponent::Int),
    ),
    method(
        "toString",
        "([J)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| arrays_to_string(vm, ref_arg(args, 0), ArrayComponent::Long),
    ),
    method(
        "toString",
        "([D)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| arrays_to_string(vm, ref_arg(args, 0), ArrayComponent::Double),
    ),
    method(
        "toString",
        "([F)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| arrays_to_string(vm, ref_arg(args, 0), ArrayComponent::Float),
    ),
    method(
        "toString",
        "([B)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| arrays_to_string(vm, ref_arg(args, 0), ArrayComponent::Byte),
    ),
    method(
        "toString",
        "([C)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| arrays_to_string(vm, ref_arg(args, 0), ArrayComponent::Char),
    ),
    method(
        "toString",
        "([S)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| arrays_to_string(vm, ref_arg(args, 0), ArrayComponent::Short),
    ),
    method(
        "toString",
        "([Z)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| arrays_to_string(vm, ref_arg(args, 0), ArrayComponent::Boolean),
    ),
    method(
        "toString",
        "([Ljava/lang/Object;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        arrays_to_string_objects,
    ),
    method(
        "copyOf",
        "([II)[I",
        ACC_PUBLIC | ACC_STATIC,
        arrays_copy_of_int,
    ),
    method("fill", "([II)V", ACC_PUBLIC | ACC_STATIC, |vm, _, args| {
        let array = ref_arg(args, 0);
        let length = vm.array_length(array)?;
        for index in 0..length {
            vm.array_set(
                array,
                index as i32,
                Value::Int(crate::int_arg(args, 1)),
                false,
            )?;
        }
        void()
    }),
    method(
        "equals",
        "([I[I)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let a = ref_arg(args, 0);
            let b = ref_arg(args, 1);
            boolean(a == b || arrays_equal(vm, a, b)?)
        },
    ),
];

fn arrays_equal(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    let (Some(left), Some(right)) = (
        vm.heap.get(a).map(|object| object.data.clone()),
        vm.heap.get(b).map(|object| object.data.clone()),
    ) else {
        return Ok(false);
    };
    match (left, right) {
        (ObjectData::Array(ArrayData::Int(x)), ObjectData::Array(ArrayData::Int(y))) => Ok(x == y),
        (ObjectData::Array(ArrayData::Long(x)), ObjectData::Array(ArrayData::Long(y))) => {
            Ok(x == y)
        }
        (ObjectData::Array(ArrayData::Double(x)), ObjectData::Array(ArrayData::Double(y))) => {
            Ok(x == y)
        }
        (ObjectData::Array(ArrayData::Byte(x)), ObjectData::Array(ArrayData::Byte(y))) => {
            Ok(x == y)
        }
        (ObjectData::Array(ArrayData::Char(x)), ObjectData::Array(ArrayData::Char(y))) => {
            Ok(x == y)
        }
        (ObjectData::Array(ArrayData::Short(x)), ObjectData::Array(ArrayData::Short(y))) => {
            Ok(x == y)
        }
        (ObjectData::Array(ArrayData::Float(x)), ObjectData::Array(ArrayData::Float(y))) => {
            Ok(x == y)
        }
        (ObjectData::Array(ArrayData::Boolean(x)), ObjectData::Array(ArrayData::Boolean(y))) => {
            Ok(x == y)
        }
        (
            ObjectData::Array(ArrayData::Reference(x)),
            ObjectData::Array(ArrayData::Reference(y)),
        ) => Ok(x == y),
        _ => Ok(false),
    }
}

fn arrays_to_string(
    vm: &mut Vm<'_>,
    array: ObjectRef,
    component: ArrayComponent,
) -> Result<Value, VmError> {
    if array.is_null() {
        return object(vm.make_string("null")?);
    }
    let length = vm.array_length(array)?;
    let mut text = String::from("[");
    for index in 0..length {
        if index > 0 {
            text.push_str(", ");
        }
        let value = vm.array_get(array, index as i32)?;
        text.push_str(&format_array_element(value, &component));
    }
    text.push(']');
    object(vm.make_string(&text)?)
}

fn format_array_element(value: Value, component: &ArrayComponent) -> String {
    match component {
        ArrayComponent::Boolean => if value.as_int() != 0 { "true" } else { "false" }.to_string(),
        ArrayComponent::Char => {
            let unit = value.as_int() as u32 & 0xFFFF;
            char::from_u32(unit).map_or_else(String::new, |ch| ch.to_string())
        }
        ArrayComponent::Byte => oxjvm_vm::format::int_to_string(value.as_int() as i8 as i32),
        ArrayComponent::Short => oxjvm_vm::format::int_to_string(value.as_int() as i16 as i32),
        ArrayComponent::Int => oxjvm_vm::format::int_to_string(value.as_int()),
        ArrayComponent::Long => oxjvm_vm::format::long_to_string(value.as_long()),
        ArrayComponent::Float => oxjvm_vm::format::float_to_string(value.as_float()),
        ArrayComponent::Double => oxjvm_vm::format::double_to_string(value.as_double()),
        ArrayComponent::Reference => String::new(),
    }
}

fn arrays_to_string_objects(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let array = ref_arg(args, 0);
    if array.is_null() {
        return object(vm.make_string("null")?);
    }
    let elements = match vm.heap.get(array).map(|object| &object.data) {
        Some(ObjectData::Array(ArrayData::Reference(elements))) => elements.clone(),
        _ => Vec::new(),
    };
    let mut text = String::from("[");
    for (index, element) in elements.iter().enumerate() {
        if index > 0 {
            text.push_str(", ");
        }
        if element.is_null() {
            text.push_str("null");
        } else {
            let (class, method) =
                vm.resolve_virtual_method(*element, "toString", "()Ljava/lang/String;")?;
            let string = vm.invoke_method(class, method, vec![Value::Ref(*element)])?;
            text.push_str(&vm.string_or_empty(string.as_ref()));
        }
    }
    text.push(']');
    object(vm.make_string(&text)?)
}

fn arrays_copy_of_int(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let array = ref_arg(args, 0);
    let length = crate::int_arg(args, 1);
    if length < 0 {
        return Err(vm.throw_new("java/lang/NegativeArraySizeException", None));
    }
    let source = vm.array_length(array)?;
    let copy = vm.allocate_array_of(ArrayComponent::Int, length as usize)?;
    for index in 0..(length as usize).min(source) {
        let value = vm.array_get(array, index as i32)?;
        vm.array_set(copy, index as i32, value, false)?;
    }
    object(copy)
}

pub(crate) const ARRAYS: oxjvm_vm::NativeClass = class(
    "java/util/Arrays",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[],
    &ARRAYS_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Random (exactly the java.util.Random LCG)
// -------------------------------------------------------------------------------------------

const RANDOM_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method("<init>", "()V", ACC_PUBLIC, random_init),
    method("<init>", "(J)V", ACC_PUBLIC, random_init_seed),
    method("setSeed", "(J)V", ACC_PUBLIC, |vm, _, args| {
        set_random_seed(vm, receiver(args), crate::long_arg(args, 1));
        void()
    }),
    method("next", "(I)I", ACC_PUBLIC, {
        fn next_bits(
            vm: &mut Vm<'_>,
            _c: oxjvm_vm::NativeContext,
            args: &[Value],
        ) -> Result<Value, VmError> {
            let bits = crate::int_arg(args, 1);
            let this = receiver(args);
            let mut seed = read_random_seed(vm, this);
            seed = seed.wrapping_mul(0x5DEECE66D).wrapping_add(0xB) & ((1i64 << 48) - 1);
            write_random_seed(vm, this, seed);
            int((seed >> (48 - bits)) as i32)
        }
        next_bits
    }),
    method("nextInt", "()I", ACC_PUBLIC, |vm, _, args| {
        next_random_int(vm, receiver(args), 32, &[])
    }),
    method("nextInt", "(I)I", ACC_PUBLIC, |vm, _, args| {
        let bound = crate::int_arg(args, 1);
        if bound <= 0 {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("bound must be positive"),
            ));
        }
        next_random_int(vm, receiver(args), 31, &[bound])
    }),
    method("nextLong", "()J", ACC_PUBLIC, |vm, _, args| {
        let high = next_random_int(vm, receiver(args), 32, &[])?;
        let low = next_random_int(vm, receiver(args), 32, &[])?;
        long(((high.as_int() as i64) << 32).wrapping_add(i64::from(low.as_int())))
    }),
    method("nextBoolean", "()Z", ACC_PUBLIC, |vm, _, args| {
        let value = next_random_int(vm, receiver(args), 1, &[])?;
        boolean(value.as_int() != 0)
    }),
    method("nextDouble", "()D", ACC_PUBLIC, |vm, _, args| {
        let high = next_random_int(vm, receiver(args), 26, &[])?;
        let low = next_random_int(vm, receiver(args), 27, &[])?;
        let value = ((high.as_int() as i64) << 27).wrapping_add(i64::from(low.as_int()));
        Ok(Value::Double(value as f64 / (1i64 << 53) as f64))
    }),
];

fn random_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let now = vm.host_time_millis();
    set_random_seed(vm, receiver(args), now);
    void()
}

fn random_init_seed(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    set_random_seed(vm, receiver(args), crate::long_arg(args, 1));
    void()
}

fn set_random_seed(vm: &mut Vm<'_>, this: ObjectRef, seed: i64) {
    let scrambled = (seed ^ 0x5DEECE66D) & ((1i64 << 48) - 1);
    write_random_seed(vm, this, scrambled);
}

fn read_random_seed(vm: &Vm<'_>, this: ObjectRef) -> i64 {
    match vm.heap.get(this).map(|object| &object.data) {
        Some(ObjectData::Instance(fields)) if !fields.is_empty() => fields[0].as_long(),
        _ => 0,
    }
}

fn write_random_seed(vm: &mut Vm<'_>, this: ObjectRef, seed: i64) {
    let class = vm.class_of(this);
    if let Ok((declaring, field)) = vm.find_field(class, "seed", "J") {
        let slot = vm.classes.get(declaring).fields[field as usize].slot as usize;
        if let Some(ObjectData::Instance(fields)) =
            vm.heap.get_mut(this).map(|object| &mut object.data)
        {
            fields[slot] = Value::Long(seed);
        }
    }
}

fn next_random_int(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    bits: i32,
    _bound: &[i32],
) -> Result<Value, VmError> {
    let mut seed = read_random_seed(vm, this);
    seed = seed.wrapping_mul(0x5DEECE66D).wrapping_add(0xB) & ((1i64 << 48) - 1);
    write_random_seed(vm, this, seed);
    Ok(Value::Int((seed >> (48 - bits)) as i32))
}

pub(crate) const RANDOM: oxjvm_vm::NativeClass = class(
    "java/util/Random",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[field("seed", "J", ACC_PRIVATE, None)],
    &RANDOM_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// ArrayList + iterator
// -------------------------------------------------------------------------------------------

const ARRAY_LIST_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method("<init>", "()V", ACC_PUBLIC, array_list_init),
    method("<init>", "(I)V", ACC_PUBLIC, array_list_init),
    method("add", "(Ljava/lang/Object;)Z", ACC_PUBLIC, array_list_add),
    method("get", "(I)Ljava/lang/Object;", ACC_PUBLIC, array_list_get),
    method(
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        array_list_set,
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "size"))
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "size") == 0)
    }),
    method(
        "remove",
        "(I)Ljava/lang/Object;",
        ACC_PUBLIC,
        array_list_remove,
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        array_list_remove_object,
    ),
    method(
        "contains",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| boolean(array_list_index_of(vm, ref_arg(args, 1))? >= 0),
    ),
    method(
        "indexOf",
        "(Ljava/lang/Object;)I",
        ACC_PUBLIC,
        |vm, _, args| int(array_list_index_of(vm, ref_arg(args, 1))?),
    ),
    method("clear", "()V", ACC_PUBLIC, array_list_clear),
    method(
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC,
        array_list_iterator,
    ),
    method(
        "toArray",
        "()[Ljava/lang/Object;",
        ACC_PUBLIC,
        array_list_to_array,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        array_list_to_string,
    ),
    method(
        "add",
        "(ILjava/lang/Object;)V",
        ACC_PUBLIC,
        array_list_insert,
    ),
];

fn array_list_elements(vm: &Vm<'_>, this: ObjectRef) -> Vec<ObjectRef> {
    let size = crate::lang::read_int_field(vm, this, "size").max(0) as usize;
    match vm.read_ref_field(
        this,
        vm.class_of(this),
        "elementData",
        "[Ljava/lang/Object;",
    ) {
        Some(array) => match vm.heap.get(array).map(|object| &object.data) {
            Some(ObjectData::Array(ArrayData::Reference(elements))) => {
                let mut elements = elements.clone();
                elements.truncate(size.min(elements.len()));
                elements
            }
            _ => Vec::new(),
        },
        None => Vec::new(),
    }
}

fn array_list_store(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    elements: &[ObjectRef],
) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let (declaring, field) = vm.find_field(class, "elementData", "[Ljava/lang/Object;")?;
    let object_class = vm.resolve_class("java/lang/Object")?;
    let array = vm.allocate_object_array(object_class, elements.len().max(1))?;
    for (index, element) in elements.iter().enumerate() {
        vm.array_set_ref(array, index, *element)?;
    }
    vm.set_instance_ref(this, declaring, field, array);
    crate::lang::write_int_field(vm, this, "size", elements.len() as i32);
    Ok(())
}

fn array_list_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    array_list_store(vm, receiver(args), &[])?;
    void()
}

fn array_list_add(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let mut elements = array_list_elements(vm, this);
    elements.push(ref_arg(args, 1));
    array_list_store(vm, this, &elements)?;
    boolean(true)
}

fn array_list_get(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let elements = array_list_elements(vm, receiver(args));
    let index = crate::int_arg(args, 1);
    if index < 0 || index as usize >= elements.len() {
        return Err(vm.throw_new(
            "java/lang/IndexOutOfBoundsException",
            Some(&alloc::format!(
                "Index {index} out of bounds for length {}",
                elements.len()
            )),
        ));
    }
    object(elements[index as usize])
}

fn array_list_set(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let mut elements = array_list_elements(vm, this);
    let index = crate::int_arg(args, 1);
    if index < 0 || index as usize >= elements.len() {
        return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
    }
    let previous = elements[index as usize];
    elements[index as usize] = ref_arg(args, 2);
    array_list_store(vm, this, &elements)?;
    object(previous)
}

fn array_list_insert(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let mut elements = array_list_elements(vm, this);
    let index = crate::int_arg(args, 1);
    if index < 0 || index as usize > elements.len() {
        return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
    }
    elements.insert(index as usize, ref_arg(args, 2));
    array_list_store(vm, this, &elements)?;
    void()
}

fn array_list_remove(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let mut elements = array_list_elements(vm, this);
    let index = crate::int_arg(args, 1);
    if index < 0 || index as usize >= elements.len() {
        return Err(vm.throw_new("java/lang/IndexOutOfBoundsException", None));
    }
    let removed = elements.remove(index as usize);
    array_list_store(vm, this, &elements)?;
    object(removed)
}

fn array_list_remove_object(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let target = ref_arg(args, 1);
    let index = array_list_index_of(vm, target)?;
    if index < 0 {
        return boolean(false);
    }
    let mut elements = array_list_elements(vm, this);
    elements.remove(index as usize);
    array_list_store(vm, this, &elements)?;
    boolean(true)
}

fn array_list_index_of(_vm: &mut Vm<'_>, target: ObjectRef) -> Result<i32, VmError> {
    let _ = target;
    Ok(-1)
}

fn array_list_clear(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    array_list_store(vm, receiver(args), &[])?;
    void()
}

fn array_list_iterator(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/ArrayList$Itr")?;
    let iterator = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "list", "Ljava/util/ArrayList;")?;
    vm.set_instance_ref(iterator, declaring, field, receiver(args));
    crate::lang::write_int_field(vm, iterator, "cursor", 0);
    object(iterator)
}

fn array_list_to_array(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let elements = array_list_elements(vm, receiver(args));
    let object_class = vm.resolve_class("java/lang/Object")?;
    let array = vm.allocate_object_array(object_class, elements.len())?;
    for (index, element) in elements.iter().enumerate() {
        vm.array_set_ref(array, index, *element)?;
    }
    object(array)
}

fn array_list_to_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let elements = array_list_elements(vm, receiver(args));
    let mut text = String::from("[");
    for (index, element) in elements.iter().enumerate() {
        if index > 0 {
            text.push_str(", ");
        }
        if element.is_null() {
            text.push_str("null");
        } else {
            let (class, method) =
                vm.resolve_virtual_method(*element, "toString", "()Ljava/lang/String;")?;
            let string = vm.invoke_method(class, method, vec![Value::Ref(*element)])?;
            text.push_str(&vm.string_or_empty(string.as_ref()));
        }
    }
    text.push(']');
    object(vm.make_string(&text)?)
}

pub(crate) const ARRAY_LIST: oxjvm_vm::NativeClass = class(
    "java/util/ArrayList",
    Some("java/lang/Object"),
    &[
        "java/util/List",
        "java/lang/Cloneable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC,
    &[
        field("elementData", "[Ljava/lang/Object;", ACC_PRIVATE, None),
        field("size", "I", ACC_PRIVATE, None),
    ],
    &ARRAY_LIST_METHODS,
    None,
);

const ITR_METHODS: [oxjvm_vm::NativeMethodDef; 3] = [
    method("hasNext", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let list = vm
            .read_ref_field(this, vm.class_of(this), "list", "Ljava/util/ArrayList;")
            .unwrap_or(ObjectRef::NULL);
        let cursor = crate::lang::read_int_field(vm, this, "cursor");
        boolean(cursor < crate::lang::read_int_field(vm, list, "size"))
    }),
    method("next", "()Ljava/lang/Object;", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let list = vm
            .read_ref_field(this, vm.class_of(this), "list", "Ljava/util/ArrayList;")
            .unwrap_or(ObjectRef::NULL);
        let cursor = crate::lang::read_int_field(vm, this, "cursor");
        let elements = array_list_elements(vm, list);
        if cursor < 0 || cursor as usize >= elements.len() {
            return Err(vm.throw_new("java/util/NoSuchElementException", None));
        }
        crate::lang::write_int_field(vm, this, "cursor", cursor + 1);
        object(elements[cursor as usize])
    }),
    method(
        "remove",
        "()V",
        ACC_PUBLIC,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const ARRAY_LIST_ITERATOR: oxjvm_vm::NativeClass = class(
    "java/util/ArrayList$Itr",
    Some("java/lang/Object"),
    &["java/util/Iterator"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[
        field("list", "Ljava/util/ArrayList;", ACC_PRIVATE, None),
        field("cursor", "I", ACC_PRIVATE, None),
    ],
    &ITR_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// HashMap (linear-probing arrays)
// -------------------------------------------------------------------------------------------

const HASH_MAP_METHODS: [oxjvm_vm::NativeMethodDef; 16] = [
    method("<init>", "()V", ACC_PUBLIC, hash_map_init),
    method(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        hash_map_put,
    ),
    method(
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        hash_map_get,
    ),
    method(
        "getOrDefault",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let key = ref_arg(args, 1);
            let index = hash_map_index_of(vm, receiver(args), key)?;
            if index >= 0 {
                hash_map_value_at(vm, receiver(args), index as usize)
            } else {
                object(ref_arg(args, 2))
            }
        },
    ),
    method(
        "containsKey",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| boolean(hash_map_index_of(vm, receiver(args), ref_arg(args, 1))? >= 0),
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        hash_map_remove,
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "size"))
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(crate::lang::read_int_field(vm, receiver(args), "size") == 0)
    }),
    method("clear", "()V", ACC_PUBLIC, hash_map_init),
    method("keySet", "()Ljava/util/Set;", ACC_PUBLIC, hash_map_key_set),
    method(
        "values",
        "()Ljava/util/Collection;",
        ACC_PUBLIC,
        hash_map_values,
    ),
    method(
        "containsValue",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let values = hash_map_values_of(vm, receiver(args))?;
            boolean(values.iter().any(|value| *value == ref_arg(args, 1)))
        },
    ),
    method("putAll", "(Ljava/util/Map;)V", ACC_PUBLIC, |_, _, _| void()),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        hash_map_to_string,
    ),
    method(
        "entrySet",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        crate::lang::abstract_method_pub,
    ),
    method(
        "replace",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let index = hash_map_index_of(vm, receiver(args), ref_arg(args, 1))?;
            if index < 0 {
                return object(ObjectRef::NULL);
            }
            let previous = hash_map_value_at(vm, receiver(args), index as usize)?;
            let mut pairs = hash_map_pairs(vm, receiver(args));
            pairs[index as usize].1 = ref_arg(args, 2);
            hash_map_store(vm, receiver(args), &pairs)?;
            Ok(previous)
        },
    ),
];

fn hash_map_pairs(vm: &Vm<'_>, this: ObjectRef) -> Vec<(ObjectRef, ObjectRef)> {
    let keys = match vm.read_ref_field(this, vm.class_of(this), "keys", "[Ljava/lang/Object;") {
        Some(array) => match vm.heap.get(array).map(|object| &object.data) {
            Some(ObjectData::Array(ArrayData::Reference(elements))) => elements.clone(),
            _ => Vec::new(),
        },
        None => Vec::new(),
    };
    let values = match vm.read_ref_field(this, vm.class_of(this), "values", "[Ljava/lang/Object;") {
        Some(array) => match vm.heap.get(array).map(|object| &object.data) {
            Some(ObjectData::Array(ArrayData::Reference(elements))) => elements.clone(),
            _ => Vec::new(),
        },
        None => Vec::new(),
    };
    keys.into_iter().zip(values).collect()
}

fn hash_map_store(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    pairs: &[(ObjectRef, ObjectRef)],
) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let (declaring, key_field) = vm.find_field(class, "keys", "[Ljava/lang/Object;")?;
    let (declaring2, value_field) = vm.find_field(class, "values", "[Ljava/lang/Object;")?;
    let object_class = vm.resolve_class("java/lang/Object")?;
    let keys = vm.allocate_object_array(object_class, pairs.len().max(1))?;
    let values = vm.allocate_object_array(object_class, pairs.len().max(1))?;
    for (index, (key, value)) in pairs.iter().enumerate() {
        vm.array_set_ref(keys, index, *key)?;
        vm.array_set_ref(values, index, *value)?;
    }
    vm.set_instance_ref(this, declaring, key_field, keys);
    vm.set_instance_ref(this, declaring2, value_field, values);
    crate::lang::write_int_field(vm, this, "size", pairs.len() as i32);
    Ok(())
}

fn hash_map_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    hash_map_store(vm, receiver(args), &[])?;
    void()
}

fn hash_map_index_of(vm: &mut Vm<'_>, this: ObjectRef, key: ObjectRef) -> Result<i32, VmError> {
    let pairs = hash_map_pairs(vm, this);
    for (index, (candidate, _)) in pairs.iter().enumerate() {
        if keys_equal(vm, *candidate, key)? {
            return Ok(index as i32);
        }
    }
    Ok(-1)
}

fn keys_equal(vm: &mut Vm<'_>, a: ObjectRef, b: ObjectRef) -> Result<bool, VmError> {
    if a == b {
        return Ok(true);
    }
    if a.is_null() || b.is_null() {
        return Ok(false);
    }
    let (class, method) = vm.resolve_virtual_method(a, "equals", "(Ljava/lang/Object;)Z")?;
    let result = vm.invoke_method(class, method, vec![Value::Ref(a), Value::Ref(b)])?;
    Ok(result.as_int() != 0)
}

fn hash_map_value_at(vm: &mut Vm<'_>, this: ObjectRef, index: usize) -> Result<Value, VmError> {
    let pairs = hash_map_pairs(vm, this);
    object(pairs[index].1)
}

fn hash_map_put(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let key = ref_arg(args, 1);
    let value = ref_arg(args, 2);
    let mut pairs = hash_map_pairs(vm, this);
    if let Some(index) = pairs
        .iter()
        .position(|(candidate, _)| keys_equal(vm, *candidate, key).unwrap_or(false))
    {
        let previous = pairs[index].1;
        pairs[index].1 = value;
        hash_map_store(vm, this, &pairs)?;
        object(previous)
    } else {
        pairs.push((key, value));
        hash_map_store(vm, this, &pairs)?;
        object(ObjectRef::NULL)
    }
}

fn hash_map_get(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let index = hash_map_index_of(vm, receiver(args), ref_arg(args, 1))?;
    if index < 0 {
        object(ObjectRef::NULL)
    } else {
        hash_map_value_at(vm, receiver(args), index as usize)
    }
}

fn hash_map_remove(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let index = hash_map_index_of(vm, this, ref_arg(args, 1))?;
    if index < 0 {
        return object(ObjectRef::NULL);
    }
    let mut pairs = hash_map_pairs(vm, this);
    let (_, removed) = pairs.remove(index as usize);
    hash_map_store(vm, this, &pairs)?;
    object(removed)
}

fn hash_map_key_set(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let pairs = hash_map_pairs(vm, receiver(args));
    let list = array_list_from(vm, &pairs.iter().map(|(key, _)| *key).collect::<Vec<_>>())?;
    object(list)
}

fn hash_map_values(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let pairs = hash_map_pairs(vm, receiver(args));
    let list = array_list_from(
        vm,
        &pairs.iter().map(|(_, value)| *value).collect::<Vec<_>>(),
    )?;
    object(list)
}

fn hash_map_values_of(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Vec<ObjectRef>, VmError> {
    Ok(hash_map_pairs(vm, this)
        .into_iter()
        .map(|(_, value)| value)
        .collect())
}

fn array_list_from(vm: &mut Vm<'_>, elements: &[ObjectRef]) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/ArrayList")?;
    let list = vm.new_instance(class)?;
    array_list_store(vm, list, elements)?;
    Ok(list)
}

fn hash_map_to_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let pairs = hash_map_pairs(vm, receiver(args));
    let mut text = String::from("{");
    for (index, (key, value)) in pairs.iter().enumerate() {
        if index > 0 {
            text.push_str(", ");
        }
        text.push_str(&value_to_string(vm, *key)?);
        text.push('=');
        text.push_str(&value_to_string(vm, *value)?);
    }
    text.push('}');
    object(vm.make_string(&text)?)
}

fn value_to_string(vm: &mut Vm<'_>, reference: ObjectRef) -> Result<String, VmError> {
    if reference.is_null() {
        return Ok("null".into());
    }
    if let Some(text) = vm.string_value(reference) {
        return Ok(text);
    }
    let (class, method) =
        vm.resolve_virtual_method(reference, "toString", "()Ljava/lang/String;")?;
    let string = vm.invoke_method(class, method, vec![Value::Ref(reference)])?;
    Ok(vm.string_or_empty(string.as_ref()))
}

pub(crate) const HASH_MAP: oxjvm_vm::NativeClass = class(
    "java/util/HashMap",
    Some("java/lang/Object"),
    &[
        "java/util/Map",
        "java/lang/Cloneable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC,
    &[
        field("keys", "[Ljava/lang/Object;", ACC_PRIVATE, None),
        field("values", "[Ljava/lang/Object;", ACC_PRIVATE, None),
        field("size", "I", ACC_PRIVATE, None),
    ],
    &HASH_MAP_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// Collections
// -------------------------------------------------------------------------------------------

const COLLECTIONS_METHODS: [oxjvm_vm::NativeMethodDef; 2] = [
    method(
        "sort",
        "(Ljava/util/List;)V",
        ACC_PUBLIC | ACC_STATIC,
        collections_sort,
    ),
    method(
        "reverse",
        "(Ljava/util/List;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let list = ref_arg(args, 0);
            let size = invoke_int(vm, list, "size")?;
            for index in 0..size / 2 {
                let a = invoke_object(vm, list, "get", &[Value::Int(index)])?;
                let b = invoke_object(vm, list, "get", &[Value::Int(size - 1 - index)])?;
                invoke_void(vm, list, "set", &[Value::Int(index), Value::Ref(b)])?;
                invoke_void(
                    vm,
                    list,
                    "set",
                    &[Value::Int(size - 1 - index), Value::Ref(a)],
                )?;
            }
            void()
        },
    ),
];

fn collections_sort(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let list = ref_arg(args, 0);
    let size = invoke_int(vm, list, "size")?;
    // Insertion sort using `compareTo` on each element.
    for index in 1..size {
        let mut cursor = index;
        while cursor > 0 {
            let a = invoke_object(vm, list, "get", &[Value::Int(cursor - 1)])?;
            let b = invoke_object(vm, list, "get", &[Value::Int(cursor)])?;
            let ordering = if a.is_null() || b.is_null() {
                i32::from(a.is_null() && !b.is_null())
            } else if vm.string_utf16(a).is_some() && vm.string_utf16(b).is_some() {
                let an = vm.string_or_empty(a);
                let bn = vm.string_or_empty(b);
                match an.cmp(&bn) {
                    core::cmp::Ordering::Less => -1,
                    core::cmp::Ordering::Equal => 0,
                    core::cmp::Ordering::Greater => 1,
                }
            } else {
                let (class, method) =
                    vm.resolve_virtual_method(a, "compareTo", "(Ljava/lang/Object;)I")?;
                vm.invoke_method(class, method, vec![Value::Ref(a), Value::Ref(b)])?
                    .as_int()
            };
            if ordering <= 0 {
                break;
            }
            invoke_void(vm, list, "set", &[Value::Int(cursor - 1), Value::Ref(b)])?;
            invoke_void(vm, list, "set", &[Value::Int(cursor), Value::Ref(a)])?;
            cursor -= 1;
        }
    }
    void()
}

fn invoke_int(vm: &mut Vm<'_>, receiver: ObjectRef, name: &str) -> Result<i32, VmError> {
    let (class, method) = vm.resolve_virtual_method(receiver, name, "()I")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(receiver)])?
        .as_int())
}

fn invoke_object(
    vm: &mut Vm<'_>,
    receiver: ObjectRef,
    name: &str,
    extra: &[Value],
) -> Result<ObjectRef, VmError> {
    let (class, method) = vm.resolve_virtual_method(receiver, name, "(I)Ljava/lang/Object;")?;
    let mut args = vec![Value::Ref(receiver)];
    args.extend_from_slice(extra);
    Ok(vm.invoke_method(class, method, args)?.as_ref())
}

fn invoke_void(
    vm: &mut Vm<'_>,
    receiver: ObjectRef,
    name: &str,
    extra: &[Value],
) -> Result<(), VmError> {
    let (class, method) =
        vm.resolve_virtual_method(receiver, name, "(ILjava/lang/Object;)Ljava/lang/Object;")?;
    let mut args = vec![Value::Ref(receiver)];
    args.extend_from_slice(extra);
    vm.invoke_method(class, method, args)?;
    Ok(())
}

pub(crate) const COLLECTIONS: oxjvm_vm::NativeClass = class(
    "java/util/Collections",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC,
    &[],
    &COLLECTIONS_METHODS,
    None,
);
