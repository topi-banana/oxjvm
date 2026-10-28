//! `java.util.TreeMap` and `java.util.TreeSet`: the sorted collections.
//!
//! Both classes keep their entries in the VM's ordered-map payload, a Rust `BTreeMap` whose
//! keys are *ranks*. The order itself comes from the guest: a map compares its keys with the
//! `Comparator` it was built with, or with the keys' own `compareTo` when it has none, and the
//! payload keeps the entries in that order. So `firstKey`, `keySet` and the rest walk the map in
//! the order the JDK promises, and the collector never has to ask the guest a question.
//!
//! The range views (`headMap`, `tailMap`, `subMap`, and the `Set` views) hand back a copy rather
//! than a live view, and the sorted views are snapshots as well.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ClassId, ObjectRef, Value, Vm, VmError};

use crate::util::{
    array_list_from, collection_elements, collection_remove_all, collection_retain_all,
    collection_to_array_into, hash_set_present, keys_equal, new_entry, object_hash,
    value_to_string,
};
use crate::{boolean, class, field, int, method, object, receiver, ref_arg, void};

// -------------------------------------------------------------------------------------------
// Shared helpers
// -------------------------------------------------------------------------------------------

/// Call a method on a guest object.
fn call(
    vm: &mut Vm<'_>,
    target: ObjectRef,
    name: &str,
    descriptor: &str,
    arguments: &[Value],
) -> Result<Value, VmError> {
    let (class, method) = vm.resolve_virtual_method(target, name, descriptor)?;
    let mut args = vec![Value::Ref(target)];
    args.extend_from_slice(arguments);
    vm.invoke_method(class, method, args)
}

/// The entries of a sorted map, in the map's order.
fn entries(vm: &Vm<'_>, this: ObjectRef) -> Vec<(ObjectRef, ObjectRef)> {
    match vm.sorted_map(this) {
        Some(map) => map.iter().map(|entry| (entry.key, entry.value)).collect(),
        None => Vec::new(),
    }
}

/// The keys of a sorted map, in the map's order.
fn keys(vm: &Vm<'_>, this: ObjectRef) -> Vec<ObjectRef> {
    match vm.sorted_map(this) {
        Some(map) => map.keys().collect(),
        None => Vec::new(),
    }
}

/// The values of a sorted map, in the map's order.
fn values(vm: &Vm<'_>, this: ObjectRef) -> Vec<ObjectRef> {
    match vm.sorted_map(this) {
        Some(map) => map.values().collect(),
        None => Vec::new(),
    }
}

/// The number of entries of a sorted map.
fn size_of(vm: &Vm<'_>, this: ObjectRef) -> i32 {
    vm.sorted_map(this).map_or(0, |map| map.len() as i32)
}

/// The `Comparator` a sorted map orders its keys with, or null.
fn ordering_of(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    vm.sorted_map(this)
        .map_or(ObjectRef::NULL, |map| map.ordering())
}

/// The `compareTo` of a key, or `None` when the key is not `Comparable`.
fn find_compare_to(vm: &Vm<'_>, key: ObjectRef) -> Option<(ClassId, u32)> {
    let class = vm.class_of(key);
    for descriptor in ["(Ljava/lang/Object;)I", "(Ljava/lang/Comparable;)I"] {
        if let Some(found) = vm.find_method(class, "compareTo", descriptor) {
            return Some(found);
        }
    }
    None
}

/// Compare two keys the way a sorted map orders them.
///
/// The map's `Comparator` decides when it has one; otherwise the keys order themselves through
/// `compareTo`, and a key that is not `Comparable` is a `ClassCastException` as in the JDK.
fn compare_keys(
    vm: &mut Vm<'_>,
    ordering: ObjectRef,
    left: ObjectRef,
    right: ObjectRef,
) -> Result<i32, VmError> {
    if left == right {
        return Ok(0);
    }
    if ordering.is_null() {
        if left.is_null() || right.is_null() {
            return Err(vm.throw_new("java/lang/NullPointerException", None));
        }
        let Some((class, method)) = find_compare_to(vm, left) else {
            return Err(vm.throw_new("java/lang/ClassCastException", None));
        };
        Ok(vm
            .invoke_method(class, method, vec![Value::Ref(left), Value::Ref(right)])?
            .as_int())
    } else {
        let (class, method) = vm.resolve_virtual_method(
            ordering,
            "compare",
            "(Ljava/lang/Object;Ljava/lang/Object;)I",
        )?;
        Ok(vm
            .invoke_method(
                class,
                method,
                vec![Value::Ref(ordering), Value::Ref(left), Value::Ref(right)],
            )?
            .as_int())
    }
}

/// The position of `key` in a sorted map's order, and whether an equal key is already there.
fn search_key(vm: &mut Vm<'_>, this: ObjectRef, key: ObjectRef) -> Result<(usize, bool), VmError> {
    let ordering = ordering_of(vm, this);
    let mut low = 0usize;
    let mut high = vm.sorted_map(this).map_or(0, |map| map.len());
    while low < high {
        let mid = low + (high - low) / 2;
        let Some(candidate) = vm.sorted_map(this).and_then(|map| map.key_at(mid)) else {
            break;
        };
        match compare_keys(vm, ordering, candidate, key)? {
            result if result < 0 => low = mid + 1,
            result if result > 0 => high = mid,
            _ => return Ok((mid, true)),
        }
    }
    Ok((low, false))
}

/// Put a key/value pair into a sorted map, returning the value it replaced.
fn put_value(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    key: ObjectRef,
    value: ObjectRef,
) -> Result<ObjectRef, VmError> {
    let (index, found) = search_key(vm, this, key)?;
    let Some(map) = vm.sorted_map_mut(this) else {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    };
    if found {
        Ok(map.set_value_at(index, value).unwrap_or(ObjectRef::NULL))
    } else {
        map.insert_at(index, key, value);
        Ok(ObjectRef::NULL)
    }
}

/// Remove a key from a sorted map, returning the value it held.
fn remove_key(vm: &mut Vm<'_>, this: ObjectRef, key: ObjectRef) -> Result<ObjectRef, VmError> {
    let (index, found) = search_key(vm, this, key)?;
    if !found {
        return Ok(ObjectRef::NULL);
    }
    Ok(vm
        .sorted_map_mut(this)
        .and_then(|map| map.remove_at(index))
        .map_or(ObjectRef::NULL, |entry| entry.value))
}

/// A fresh sorted map with the given ordering.
fn new_sorted_map(vm: &mut Vm<'_>, class: &str, ordering: ObjectRef) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class(class)?;
    let instance = vm.new_instance(class)?;
    let (declaring, constructor) = vm
        .find_method(class, "<init>", "()V")
        .ok_or_else(|| VmError::internal("sorted map constructor missing"))?;
    vm.invoke_method(declaring, constructor, vec![Value::Ref(instance)])?;
    if let Some(map) = vm.sorted_map_mut(instance) {
        map.set_ordering(ordering);
    }
    Ok(instance)
}

/// The key of a `Map.Entry`.
fn entry_key(vm: &mut Vm<'_>, entry: ObjectRef) -> Result<ObjectRef, VmError> {
    let (class, method) = vm.resolve_virtual_method(entry, "getKey", "()Ljava/lang/Object;")?;
    Ok(vm
        .invoke_method(class, method, vec![Value::Ref(entry)])?
        .as_ref())
}

// -------------------------------------------------------------------------------------------
// TreeMap
// -------------------------------------------------------------------------------------------

/// `TreeMap.<init>()`: an empty map that orders its keys by themselves.
fn tree_map_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    vm.set_sorted_map(this, oxjvm_vm::SortedMap::new());
    void()
}

/// `TreeMap.<init>(Comparator)`: an empty map that orders its keys with `comparator`.
fn tree_map_init_comparator(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let mut map = oxjvm_vm::SortedMap::new();
    map.set_ordering(ref_arg(args, 1));
    vm.set_sorted_map(this, map);
    void()
}

/// `TreeMap.<init>(Map)`: a map holding the entries of `source`.
fn tree_map_init_map(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    vm.set_sorted_map(this, oxjvm_vm::SortedMap::new());
    tree_map_put_all(vm, this, ref_arg(args, 1))
}

/// `TreeMap.putAll(Map)`: copy the entries of `source` into the map.
///
/// The source is walked through its own `keySet` and `get`, so any `Map` implementation works.
fn tree_map_put_all(vm: &mut Vm<'_>, this: ObjectRef, source: ObjectRef) -> Result<Value, VmError> {
    if source.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let source_keys = call(vm, source, "keySet", "()Ljava/util/Set;", &[])?;
    for key in collection_elements(vm, source_keys.as_ref())? {
        let value = call(
            vm,
            source,
            "get",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            &[Value::Ref(key)],
        )?;
        put_value(vm, this, key, value.as_ref())?;
    }
    void()
}

/// `TreeMap.put(Object, Object)`.
fn tree_map_put(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let previous = put_value(vm, this, ref_arg(args, 1), ref_arg(args, 2))?;
    object(previous)
}

/// `TreeMap.get(Object)`.
fn tree_map_get(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let key = ref_arg(args, 1);
    let (index, found) = search_key(vm, this, key)?;
    if !found {
        return object(ObjectRef::NULL);
    }
    let value = vm
        .sorted_map(this)
        .and_then(|map| map.value_at(index))
        .unwrap_or(ObjectRef::NULL);
    object(value)
}

/// `TreeMap.remove(Object)`.
fn tree_map_remove(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let removed = remove_key(vm, this, ref_arg(args, 1))?;
    object(removed)
}

/// `TreeMap.containsKey(Object)`.
fn tree_map_contains_key(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let (_, found) = search_key(vm, receiver(args), ref_arg(args, 1))?;
    boolean(found)
}

/// `TreeMap.containsValue(Object)`.
fn tree_map_contains_value(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let wanted = ref_arg(args, 1);
    for value in values(vm, receiver(args)) {
        if keys_equal(vm, value, wanted)? {
            return boolean(true);
        }
    }
    boolean(false)
}

/// `TreeMap.getOrDefault(Object, Object)`.
fn tree_map_get_or_default(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let key = ref_arg(args, 1);
    let (index, found) = search_key(vm, this, key)?;
    if !found {
        return object(ref_arg(args, 2));
    }
    let value = vm
        .sorted_map(this)
        .and_then(|map| map.value_at(index))
        .unwrap_or(ObjectRef::NULL);
    object(value)
}

/// `TreeMap.putIfAbsent(Object, Object)`.
fn tree_map_put_if_absent(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let key = ref_arg(args, 1);
    let value = ref_arg(args, 2);
    let (index, found) = search_key(vm, this, key)?;
    let current = if found {
        vm.sorted_map(this)
            .and_then(|map| map.value_at(index))
            .unwrap_or(ObjectRef::NULL)
    } else {
        ObjectRef::NULL
    };
    if !current.is_null() {
        return object(current);
    }
    put_value(vm, this, key, value)?;
    object(ObjectRef::NULL)
}

/// `TreeMap.replace(Object, Object)`.
fn tree_map_replace(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let key = ref_arg(args, 1);
    let value = ref_arg(args, 2);
    let (index, found) = search_key(vm, this, key)?;
    if !found {
        return object(ObjectRef::NULL);
    }
    let previous = vm
        .sorted_map_mut(this)
        .and_then(|map| map.set_value_at(index, value))
        .unwrap_or(ObjectRef::NULL);
    object(previous)
}

/// `TreeMap.replace(Object, Object, Object)`.
fn tree_map_replace_if_equal(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let key = ref_arg(args, 1);
    let expected = ref_arg(args, 2);
    let value = ref_arg(args, 3);
    let (index, found) = search_key(vm, this, key)?;
    if !found {
        return boolean(false);
    }
    let current = vm
        .sorted_map(this)
        .and_then(|map| map.value_at(index))
        .unwrap_or(ObjectRef::NULL);
    if !keys_equal(vm, current, expected)? {
        return boolean(false);
    }
    if let Some(map) = vm.sorted_map_mut(this) {
        map.set_value_at(index, value);
    }
    boolean(true)
}

/// `TreeMap.remove(Object, Object)`.
fn tree_map_remove_if_equal(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let key = ref_arg(args, 1);
    let expected = ref_arg(args, 2);
    let (index, found) = search_key(vm, this, key)?;
    if !found {
        return boolean(false);
    }
    let current = vm
        .sorted_map(this)
        .and_then(|map| map.value_at(index))
        .unwrap_or(ObjectRef::NULL);
    if !keys_equal(vm, current, expected)? {
        return boolean(false);
    }
    if let Some(map) = vm.sorted_map_mut(this) {
        map.remove_at(index);
    }
    boolean(true)
}

/// `TreeMap.computeIfAbsent(Object, Function)`.
fn tree_map_compute_if_absent(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let key = ref_arg(args, 1);
    let function = ref_arg(args, 2);
    if function.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (index, found) = search_key(vm, this, key)?;
    let current = if found {
        vm.sorted_map(this)
            .and_then(|map| map.value_at(index))
            .unwrap_or(ObjectRef::NULL)
    } else {
        ObjectRef::NULL
    };
    if !current.is_null() {
        return object(current);
    }
    let value = call(
        vm,
        function,
        "apply",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        &[Value::Ref(key)],
    )?
    .as_ref();
    if !value.is_null() {
        put_value(vm, this, key, value)?;
    }
    object(value)
}

/// `TreeMap.computeIfPresent(Object, BiFunction)`.
fn tree_map_compute_if_present(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let key = ref_arg(args, 1);
    let function = ref_arg(args, 2);
    if function.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (index, found) = search_key(vm, this, key)?;
    if !found {
        return object(ObjectRef::NULL);
    }
    let current = vm
        .sorted_map(this)
        .and_then(|map| map.value_at(index))
        .unwrap_or(ObjectRef::NULL);
    if current.is_null() {
        return object(ObjectRef::NULL);
    }
    let value = call(
        vm,
        function,
        "apply",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[Value::Ref(key), Value::Ref(current)],
    )?
    .as_ref();
    if value.is_null() {
        if let Some(map) = vm.sorted_map_mut(this) {
            map.remove_at(index);
        }
    } else if let Some(map) = vm.sorted_map_mut(this) {
        map.set_value_at(index, value);
    }
    object(value)
}

/// `TreeMap.compute(Object, BiFunction)`.
fn tree_map_compute(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let key = ref_arg(args, 1);
    let function = ref_arg(args, 2);
    if function.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (index, found) = search_key(vm, this, key)?;
    let current = if found {
        vm.sorted_map(this)
            .and_then(|map| map.value_at(index))
            .unwrap_or(ObjectRef::NULL)
    } else {
        ObjectRef::NULL
    };
    let value = call(
        vm,
        function,
        "apply",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[Value::Ref(key), Value::Ref(current)],
    )?
    .as_ref();
    if value.is_null() {
        if found {
            if let Some(map) = vm.sorted_map_mut(this) {
                map.remove_at(index);
            }
        }
    } else {
        put_value(vm, this, key, value)?;
    }
    object(value)
}

/// `TreeMap.merge(Object, Object, BiFunction)`.
fn tree_map_merge(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let key = ref_arg(args, 1);
    let value = ref_arg(args, 2);
    let function = ref_arg(args, 3);
    let (index, found) = search_key(vm, this, key)?;
    let current = if found {
        vm.sorted_map(this)
            .and_then(|map| map.value_at(index))
            .unwrap_or(ObjectRef::NULL)
    } else {
        ObjectRef::NULL
    };
    let merged = if current.is_null() {
        value
    } else {
        call(
            vm,
            function,
            "apply",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            &[Value::Ref(current), Value::Ref(value)],
        )?
        .as_ref()
    };
    if merged.is_null() {
        if found {
            if let Some(map) = vm.sorted_map_mut(this) {
                map.remove_at(index);
            }
        }
    } else {
        put_value(vm, this, key, merged)?;
    }
    object(merged)
}

/// `TreeMap.forEach(BiConsumer)`: walk the entries in the map's order.
fn tree_map_for_each(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let action = ref_arg(args, 1);
    if action.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    for (key, value) in entries(vm, receiver(args)) {
        call(
            vm,
            action,
            "accept",
            "(Ljava/lang/Object;Ljava/lang/Object;)V",
            &[Value::Ref(key), Value::Ref(value)],
        )?;
    }
    void()
}

/// `TreeMap.replaceAll(BiFunction)`: replace every value with `function.apply(key, value)`.
fn tree_map_replace_all(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let function = ref_arg(args, 1);
    if function.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let this = receiver(args);
    let current = entries(vm, this);
    for (index, (key, value)) in current.into_iter().enumerate() {
        let replacement = call(
            vm,
            function,
            "apply",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            &[Value::Ref(key), Value::Ref(value)],
        )?
        .as_ref();
        if let Some(map) = vm.sorted_map_mut(this) {
            map.set_value_at(index, replacement);
        }
    }
    void()
}

/// `TreeMap.keySet()`: the keys, in the map's order.
fn tree_map_key_set(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let set = new_sorted_map(vm, "java/util/TreeSet", ordering_of(vm, this))?;
    let present = hash_set_present(vm)?;
    for key in keys(vm, this) {
        put_value(vm, set, key, present)?;
    }
    object(set)
}

/// `TreeMap.values()`: the values, in the map's order.
fn tree_map_values(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let list = array_list_from(vm, &values(vm, receiver(args)))?;
    object(list)
}

/// `TreeMap.entrySet()`: the entries, in the map's order.
fn tree_map_entry_set(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let order = new_entry_order(vm, this)?;
    let set = new_sorted_map(vm, "java/util/TreeSet", order)?;
    let present = hash_set_present(vm)?;
    for (key, value) in entries(vm, this) {
        let entry = new_entry(vm, key, value)?;
        put_value(vm, set, entry, present)?;
    }
    object(set)
}

/// `TreeMap.toString()`: `{key=value, ...}` in the map's order.
fn tree_map_to_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let mut text = String::from("{");
    for (index, (key, value)) in entries(vm, receiver(args)).into_iter().enumerate() {
        if index > 0 {
            text.push_str(", ");
        }
        text.push_str(&value_to_string(vm, key)?);
        text.push('=');
        text.push_str(&value_to_string(vm, value)?);
    }
    text.push('}');
    object(vm.make_string(&text)?)
}

/// `TreeMap.hashCode()`: the sum of the entries' hash codes.
fn tree_map_hash_code(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let mut hash: i32 = 0;
    for (key, value) in entries(vm, receiver(args)) {
        hash = hash.wrapping_add(object_hash(vm, key)? ^ object_hash(vm, value)?);
    }
    int(hash)
}

/// `TreeMap.equals(Object)`: the JDK's `Map` equality, entry by entry.
fn tree_map_equals(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let other = ref_arg(args, 1);
    if this == other {
        return boolean(true);
    }
    let map_class = vm.resolve_class("java/util/Map")?;
    if other.is_null() || !vm.is_instance(other, map_class) {
        return boolean(false);
    }
    let this_entries = entries(vm, this);
    let other_size = call(vm, other, "size", "()I", &[])?.as_int();
    if other_size != this_entries.len() as i32 {
        return boolean(false);
    }
    for (key, value) in this_entries {
        let other_value = call(
            vm,
            other,
            "get",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            &[Value::Ref(key)],
        )?
        .as_ref();
        if value.is_null() {
            // A null value only matches a key the other map knows about.
            let contains = call(
                vm,
                other,
                "containsKey",
                "(Ljava/lang/Object;)Z",
                &[Value::Ref(key)],
            )?
            .as_int();
            if contains == 0 {
                return boolean(false);
            }
        } else if other_value.is_null() || !keys_equal(vm, value, other_value)? {
            return boolean(false);
        }
    }
    boolean(true)
}

/// `TreeMap.firstKey()`.
fn tree_map_first_key(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let Some(key) = vm.sorted_map(receiver(args)).and_then(|map| map.key_at(0)) else {
        return Err(vm.throw_new("java/util/NoSuchElementException", None));
    };
    object(key)
}

/// `TreeMap.lastKey()`.
fn tree_map_last_key(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let size = size_of(vm, receiver(args)) as usize;
    let Some(key) = vm
        .sorted_map(receiver(args))
        .and_then(|map| map.key_at(size.saturating_sub(1)))
    else {
        return Err(vm.throw_new("java/util/NoSuchElementException", None));
    };
    object(key)
}

/// `TreeMap.firstEntry()`.
fn tree_map_first_entry(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let Some((key, value)) = vm
        .sorted_map(receiver(args))
        .and_then(|map| map.get_at(0))
        .map(|entry| (entry.key, entry.value))
    else {
        return object(ObjectRef::NULL);
    };
    object(new_entry(vm, key, value)?)
}

/// `TreeMap.lastEntry()`.
fn tree_map_last_entry(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let size = size_of(vm, receiver(args)) as usize;
    let Some((key, value)) = vm
        .sorted_map(receiver(args))
        .and_then(|map| map.get_at(size.saturating_sub(1)))
        .map(|entry| (entry.key, entry.value))
    else {
        return object(ObjectRef::NULL);
    };
    object(new_entry(vm, key, value)?)
}

/// Remove the first (`first`) or last entry of the map and return it.
fn tree_map_poll_entry(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    first: bool,
) -> Result<ObjectRef, VmError> {
    let size = size_of(vm, this) as usize;
    let index = if first { 0 } else { size.saturating_sub(1) };
    let Some(key) = vm.sorted_map(this).and_then(|map| map.key_at(index)) else {
        return Ok(ObjectRef::NULL);
    };
    let value = remove_key(vm, this, key)?;
    new_entry(vm, key, value)
}

/// `TreeMap.pollFirstEntry()`: remove and return the first entry.
fn tree_map_poll_first_entry(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let entry = tree_map_poll_entry(vm, receiver(args), true)?;
    object(entry)
}

/// `TreeMap.pollLastEntry()`: remove and return the last entry.
fn tree_map_poll_last_entry(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let entry = tree_map_poll_entry(vm, receiver(args), false)?;
    object(entry)
}

/// Which neighbour of a key a sorted collection should return.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Neighbour {
    /// The smallest key that is not below the given one.
    Ceiling,
    /// The largest key that is not above the given one.
    Floor,
    /// The smallest key that is strictly above the given one.
    Higher,
    /// The largest key that is strictly below the given one.
    Lower,
}

/// The neighbour of `key` the collection asks for, or null when there is none.
fn neighbour(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    key: ObjectRef,
    which: Neighbour,
) -> Result<ObjectRef, VmError> {
    let (index, found) = search_key(vm, this, key)?;
    let position = match which {
        // The key itself answers the question unless a strictly different one is wanted.
        Neighbour::Ceiling | Neighbour::Floor => {
            if found {
                return Ok(vm
                    .sorted_map(this)
                    .and_then(|map| map.key_at(index))
                    .unwrap_or(ObjectRef::NULL));
            }
            match which {
                Neighbour::Ceiling => index,
                _ => match index.checked_sub(1) {
                    Some(position) => position,
                    None => return Ok(ObjectRef::NULL),
                },
            }
        }
        Neighbour::Higher => {
            if found {
                index + 1
            } else {
                index
            }
        }
        Neighbour::Lower => match index.checked_sub(1) {
            Some(position) => position,
            None => return Ok(ObjectRef::NULL),
        },
    };
    Ok(vm
        .sorted_map(this)
        .and_then(|map| map.key_at(position))
        .unwrap_or(ObjectRef::NULL))
}

/// `TreeMap.ceilingKey(Object)`.
fn tree_map_ceiling_key(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let key = neighbour(vm, receiver(args), ref_arg(args, 1), Neighbour::Ceiling)?;
    object(key)
}

/// `TreeMap.floorKey(Object)`.
fn tree_map_floor_key(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let key = neighbour(vm, receiver(args), ref_arg(args, 1), Neighbour::Floor)?;
    object(key)
}

/// `TreeMap.higherKey(Object)`.
fn tree_map_higher_key(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let key = neighbour(vm, receiver(args), ref_arg(args, 1), Neighbour::Higher)?;
    object(key)
}

/// `TreeMap.lowerKey(Object)`.
fn tree_map_lower_key(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let key = neighbour(vm, receiver(args), ref_arg(args, 1), Neighbour::Lower)?;
    object(key)
}

/// The `Map.Entry` of the neighbour of `key` the map asks for, or null.
fn tree_map_neighbour_entry(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    key: ObjectRef,
    which: Neighbour,
) -> Result<ObjectRef, VmError> {
    let neighbour = neighbour(vm, this, key, which)?;
    if neighbour.is_null() {
        return Ok(ObjectRef::NULL);
    }
    let value = call(
        vm,
        this,
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        &[Value::Ref(neighbour)],
    )?
    .as_ref();
    new_entry(vm, neighbour, value)
}

/// `TreeMap.higherEntry(Object)`.
fn tree_map_higher_entry(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let entry = tree_map_neighbour_entry(vm, receiver(args), ref_arg(args, 1), Neighbour::Higher)?;
    object(entry)
}

/// `TreeMap.ceilingEntry(Object)`.
fn tree_map_ceiling_entry(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let entry = tree_map_neighbour_entry(vm, receiver(args), ref_arg(args, 1), Neighbour::Ceiling)?;
    object(entry)
}

/// `TreeMap.floorEntry(Object)`.
fn tree_map_floor_entry(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let entry = tree_map_neighbour_entry(vm, receiver(args), ref_arg(args, 1), Neighbour::Floor)?;
    object(entry)
}

/// `TreeMap.lowerEntry(Object)`.
fn tree_map_lower_entry(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let entry = tree_map_neighbour_entry(vm, receiver(args), ref_arg(args, 1), Neighbour::Lower)?;
    object(entry)
}

/// `TreeMap.pollFirst()`: remove and return the first entry.
fn tree_map_poll_first(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let entry = tree_map_poll_entry(vm, receiver(args), true)?;
    object(entry)
}

/// `TreeMap.pollLast()`: remove and return the last entry.
fn tree_map_poll_last(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let entry = tree_map_poll_entry(vm, receiver(args), false)?;
    object(entry)
}

/// A copy of the map holding the entries before `key` (`headMap`), from `key` on (`tailMap`),
/// or between two keys (`subMap`).
fn tree_map_range(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    from: Option<ObjectRef>,
    to: Option<ObjectRef>,
) -> Result<ObjectRef, VmError> {
    let ordering = ordering_of(vm, this);
    let range = new_sorted_map(vm, "java/util/TreeMap", ordering)?;
    for (key, value) in entries(vm, this) {
        if let Some(from) = from {
            if compare_keys(vm, ordering, key, from)? < 0 {
                continue;
            }
        }
        if let Some(to) = to {
            if compare_keys(vm, ordering, key, to)? >= 0 {
                continue;
            }
        }
        put_value(vm, range, key, value)?;
    }
    Ok(range)
}

/// `TreeMap.headMap(Object)`.
fn tree_map_head_map(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let range = tree_map_range(vm, receiver(args), None, Some(ref_arg(args, 1)))?;
    object(range)
}

/// `TreeMap.tailMap(Object)`.
fn tree_map_tail_map(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let range = tree_map_range(vm, receiver(args), Some(ref_arg(args, 1)), None)?;
    object(range)
}

/// `TreeMap.subMap(Object, Object)`.
fn tree_map_sub_map(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let range = tree_map_range(
        vm,
        receiver(args),
        Some(ref_arg(args, 1)),
        Some(ref_arg(args, 2)),
    )?;
    object(range)
}

/// `TreeMap.descendingMap()`: the map with the order turned around.
fn tree_map_descending_map(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let reversed = new_reversed_order(vm, this)?;
    let map = new_sorted_map(vm, "java/util/TreeMap", reversed)?;
    for (key, value) in entries(vm, this) {
        put_value(vm, map, key, value)?;
    }
    object(map)
}

/// `TreeMap.navigableKeySet()`: the keys, in the map's order.
fn tree_map_navigable_key_set(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let set = new_sorted_map(vm, "java/util/TreeSet", ordering_of(vm, this))?;
    let present = hash_set_present(vm)?;
    for key in keys(vm, this) {
        put_value(vm, set, key, present)?;
    }
    object(set)
}

/// `TreeMap.descendingKeySet()`: the keys, in the reverse order.
fn tree_map_descending_key_set(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let reversed = new_reversed_order(vm, this)?;
    let set = new_sorted_map(vm, "java/util/TreeSet", reversed)?;
    let present = hash_set_present(vm)?;
    for key in keys(vm, this) {
        put_value(vm, set, key, present)?;
    }
    object(set)
}

const TREE_MAP_METHODS: [oxjvm_vm::NativeMethodDef; 56] = [
    method("<init>", "()V", ACC_PUBLIC, tree_map_init),
    method(
        "<init>",
        "(Ljava/util/Comparator;)V",
        ACC_PUBLIC,
        tree_map_init_comparator,
    ),
    method(
        "<init>",
        "(Ljava/util/Map;)V",
        ACC_PUBLIC,
        tree_map_init_map,
    ),
    method(
        "<init>",
        "(Ljava/util/SortedMap;)V",
        ACC_PUBLIC,
        tree_map_init_map,
    ),
    method(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_map_put,
    ),
    method(
        "putIfAbsent",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_map_put_if_absent,
    ),
    method("putAll", "(Ljava/util/Map;)V", ACC_PUBLIC, |vm, _, args| {
        tree_map_put_all(vm, receiver(args), ref_arg(args, 1))
    }),
    method(
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_map_get,
    ),
    method(
        "getOrDefault",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_map_get_or_default,
    ),
    method(
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_map_remove,
    ),
    method(
        "remove",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        tree_map_remove_if_equal,
    ),
    method(
        "containsKey",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        tree_map_contains_key,
    ),
    method(
        "containsValue",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        tree_map_contains_value,
    ),
    method(
        "replace",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_map_replace,
    ),
    method(
        "replace",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        tree_map_replace_if_equal,
    ),
    method(
        "replaceAll",
        "(Ljava/util/function/BiFunction;)V",
        ACC_PUBLIC,
        tree_map_replace_all,
    ),
    method(
        "computeIfAbsent",
        "(Ljava/lang/Object;Ljava/util/function/Function;)Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_map_compute_if_absent,
    ),
    method(
        "computeIfPresent",
        "(Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_map_compute_if_present,
    ),
    method(
        "compute",
        "(Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_map_compute,
    ),
    method(
        "merge",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_map_merge,
    ),
    method(
        "forEach",
        "(Ljava/util/function/BiConsumer;)V",
        ACC_PUBLIC,
        tree_map_for_each,
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        int(size_of(vm, receiver(args)))
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(size_of(vm, receiver(args)) == 0)
    }),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        if let Some(map) = vm.sorted_map_mut(receiver(args)) {
            map.clear();
        }
        void()
    }),
    method("keySet", "()Ljava/util/Set;", ACC_PUBLIC, tree_map_key_set),
    method(
        "values",
        "()Ljava/util/Collection;",
        ACC_PUBLIC,
        tree_map_values,
    ),
    method(
        "entrySet",
        "()Ljava/util/Set;",
        ACC_PUBLIC,
        tree_map_entry_set,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        tree_map_to_string,
    ),
    method("hashCode", "()I", ACC_PUBLIC, tree_map_hash_code),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        tree_map_equals,
    ),
    method(
        "comparator",
        "()Ljava/util/Comparator;",
        ACC_PUBLIC,
        |vm, _, args| object(ordering_of(vm, receiver(args))),
    ),
    method(
        "firstKey",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_map_first_key,
    ),
    method(
        "lastKey",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_map_last_key,
    ),
    method(
        "headMap",
        "(Ljava/lang/Object;)Ljava/util/SortedMap;",
        ACC_PUBLIC,
        tree_map_head_map,
    ),
    method(
        "headMap",
        "(Ljava/lang/Object;)Ljava/util/NavigableMap;",
        ACC_PUBLIC,
        tree_map_head_map,
    ),
    method(
        "tailMap",
        "(Ljava/lang/Object;)Ljava/util/SortedMap;",
        ACC_PUBLIC,
        tree_map_tail_map,
    ),
    method(
        "tailMap",
        "(Ljava/lang/Object;)Ljava/util/NavigableMap;",
        ACC_PUBLIC,
        tree_map_tail_map,
    ),
    method(
        "subMap",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/SortedMap;",
        ACC_PUBLIC,
        tree_map_sub_map,
    ),
    method(
        "subMap",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/NavigableMap;",
        ACC_PUBLIC,
        tree_map_sub_map,
    ),
    method(
        "firstEntry",
        "()Ljava/util/Map$Entry;",
        ACC_PUBLIC,
        tree_map_first_entry,
    ),
    method(
        "lastEntry",
        "()Ljava/util/Map$Entry;",
        ACC_PUBLIC,
        tree_map_last_entry,
    ),
    method(
        "pollFirstEntry",
        "()Ljava/util/Map$Entry;",
        ACC_PUBLIC,
        tree_map_poll_first_entry,
    ),
    method(
        "pollLastEntry",
        "()Ljava/util/Map$Entry;",
        ACC_PUBLIC,
        tree_map_poll_last_entry,
    ),
    method(
        "pollFirst",
        "()Ljava/util/Map$Entry;",
        ACC_PUBLIC,
        tree_map_poll_first,
    ),
    method(
        "pollLast",
        "()Ljava/util/Map$Entry;",
        ACC_PUBLIC,
        tree_map_poll_last,
    ),
    method(
        "higherEntry",
        "(Ljava/lang/Object;)Ljava/util/Map$Entry;",
        ACC_PUBLIC,
        tree_map_higher_entry,
    ),
    method(
        "ceilingEntry",
        "(Ljava/lang/Object;)Ljava/util/Map$Entry;",
        ACC_PUBLIC,
        tree_map_ceiling_entry,
    ),
    method(
        "floorEntry",
        "(Ljava/lang/Object;)Ljava/util/Map$Entry;",
        ACC_PUBLIC,
        tree_map_floor_entry,
    ),
    method(
        "lowerEntry",
        "(Ljava/lang/Object;)Ljava/util/Map$Entry;",
        ACC_PUBLIC,
        tree_map_lower_entry,
    ),
    method(
        "ceilingKey",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_map_ceiling_key,
    ),
    method(
        "floorKey",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_map_floor_key,
    ),
    method(
        "higherKey",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_map_higher_key,
    ),
    method(
        "lowerKey",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_map_lower_key,
    ),
    method(
        "descendingMap",
        "()Ljava/util/NavigableMap;",
        ACC_PUBLIC,
        tree_map_descending_map,
    ),
    method(
        "navigableKeySet",
        "()Ljava/util/NavigableSet;",
        ACC_PUBLIC,
        tree_map_navigable_key_set,
    ),
    method(
        "descendingKeySet",
        "()Ljava/util/NavigableSet;",
        ACC_PUBLIC,
        tree_map_descending_key_set,
    ),
];

/// `java.util.TreeMap`: a map whose entries are kept in the keys' order.
///
/// The entries live in the VM's ordered-map payload, a Rust `BTreeMap`; the map keeps them in
/// the order the keys compare in.
pub(crate) const TREE_MAP: oxjvm_vm::NativeClass = class(
    "java/util/TreeMap",
    Some("java/lang/Object"),
    &[
        "java/util/NavigableMap",
        "java/util/Map",
        "java/lang/Cloneable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC,
    &[],
    &TREE_MAP_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// TreeSet
// -------------------------------------------------------------------------------------------

/// `TreeSet.<init>()`: an empty set that orders its elements by themselves.
fn tree_set_init(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    vm.set_sorted_map(this, oxjvm_vm::SortedMap::new());
    void()
}

/// `TreeSet.<init>(Comparator)`: an empty set that orders its elements with `comparator`.
fn tree_set_init_comparator(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let mut map = oxjvm_vm::SortedMap::new();
    map.set_ordering(ref_arg(args, 1));
    vm.set_sorted_map(this, map);
    void()
}

/// `TreeSet.<init>(Collection)`: a set holding the elements of `source`.
fn tree_set_init_collection(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    vm.set_sorted_map(this, oxjvm_vm::SortedMap::new());
    tree_set_add_all(vm, this, ref_arg(args, 1))?;
    void()
}

/// `TreeSet.add(Object)`.
fn tree_set_add(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let element = ref_arg(args, 1);
    let present = hash_set_present(vm)?;
    let previous = put_value(vm, this, element, present)?;
    boolean(previous.is_null())
}

/// `TreeSet.remove(Object)`.
fn tree_set_remove(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let removed = remove_key(vm, receiver(args), ref_arg(args, 1))?;
    boolean(!removed.is_null())
}

/// `TreeSet.contains(Object)`.
fn tree_set_contains(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let (_, found) = search_key(vm, receiver(args), ref_arg(args, 1))?;
    boolean(found)
}

/// `TreeSet.addAll(Collection)`.
fn tree_set_add_all(vm: &mut Vm<'_>, this: ObjectRef, source: ObjectRef) -> Result<Value, VmError> {
    if source.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let present = hash_set_present(vm)?;
    let mut changed = false;
    for element in collection_elements(vm, source)? {
        let previous = put_value(vm, this, element, present)?;
        changed |= previous.is_null();
    }
    boolean(changed)
}

/// `TreeSet.iterator()`: the elements, in the set's order.
fn tree_set_iterator(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let list = array_list_from(vm, &keys(vm, receiver(args)))?;
    call(vm, list, "iterator", "()Ljava/util/Iterator;", &[])
}

/// `TreeSet.toString()`: `[element, ...]` in the set's order.
fn tree_set_to_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let mut text = String::from("[");
    for (index, element) in keys(vm, receiver(args)).into_iter().enumerate() {
        if index > 0 {
            text.push_str(", ");
        }
        text.push_str(&value_to_string(vm, element)?);
    }
    text.push(']');
    object(vm.make_string(&text)?)
}

/// `TreeSet.equals(Object)`: the JDK's `Set` equality.
fn tree_set_equals(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let other = ref_arg(args, 1);
    if this == other {
        return boolean(true);
    }
    let set_class = vm.resolve_class("java/util/Set")?;
    if other.is_null() || !vm.is_instance(other, set_class) {
        return boolean(false);
    }
    let this_keys = keys(vm, this);
    let other_size = call(vm, other, "size", "()I", &[])?.as_int();
    if other_size != this_keys.len() as i32 {
        return boolean(false);
    }
    for element in this_keys {
        let found = call(
            vm,
            other,
            "contains",
            "(Ljava/lang/Object;)Z",
            &[Value::Ref(element)],
        )?
        .as_int();
        if found == 0 {
            return boolean(false);
        }
    }
    boolean(true)
}

/// `TreeSet.hashCode()`: the sum of the elements' hash codes.
fn tree_set_hash_code(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let mut hash: i32 = 0;
    for element in keys(vm, receiver(args)) {
        hash = hash.wrapping_add(object_hash(vm, element)?);
    }
    int(hash)
}

/// `TreeSet.forEach(Consumer)`: walk the elements in the set's order.
fn tree_set_for_each(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let action = ref_arg(args, 1);
    if action.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    for element in keys(vm, receiver(args)) {
        call(
            vm,
            action,
            "accept",
            "(Ljava/lang/Object;)V",
            &[Value::Ref(element)],
        )?;
    }
    void()
}

/// The element of a set that is not below `element` (`ceiling`), not above it (`floor`),
/// strictly above it (`higher`), or strictly below it (`lower`).
fn tree_set_neighbour(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    element: ObjectRef,
    which: Neighbour,
) -> Result<ObjectRef, VmError> {
    neighbour(vm, this, element, which)
}

/// `TreeSet.first()`.
fn tree_set_first(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let Some(element) = vm.sorted_map(receiver(args)).and_then(|map| map.key_at(0)) else {
        return Err(vm.throw_new("java/util/NoSuchElementException", None));
    };
    object(element)
}

/// `TreeSet.last()`.
fn tree_set_last(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let size = size_of(vm, receiver(args)) as usize;
    let Some(element) = vm
        .sorted_map(receiver(args))
        .and_then(|map| map.key_at(size.saturating_sub(1)))
    else {
        return Err(vm.throw_new("java/util/NoSuchElementException", None));
    };
    object(element)
}

/// `TreeSet.pollFirst()`: remove and return the first element.
fn tree_set_poll_first(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let Some(element) = vm.sorted_map(this).and_then(|map| map.key_at(0)) else {
        return object(ObjectRef::NULL);
    };
    remove_key(vm, this, element)?;
    object(element)
}

/// `TreeSet.pollLast()`: remove and return the last element.
fn tree_set_poll_last(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let size = size_of(vm, this) as usize;
    let Some(element) = vm
        .sorted_map(this)
        .and_then(|map| map.key_at(size.saturating_sub(1)))
    else {
        return object(ObjectRef::NULL);
    };
    remove_key(vm, this, element)?;
    object(element)
}

/// `TreeSet.ceiling(Object)`.
fn tree_set_ceiling(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let element = tree_set_neighbour(vm, receiver(args), ref_arg(args, 1), Neighbour::Ceiling)?;
    object(element)
}

/// `TreeSet.floor(Object)`.
fn tree_set_floor(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let element = tree_set_neighbour(vm, receiver(args), ref_arg(args, 1), Neighbour::Floor)?;
    object(element)
}

/// `TreeSet.higher(Object)`.
fn tree_set_higher(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let element = tree_set_neighbour(vm, receiver(args), ref_arg(args, 1), Neighbour::Higher)?;
    object(element)
}

/// `TreeSet.lower(Object)`.
fn tree_set_lower(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let element = tree_set_neighbour(vm, receiver(args), ref_arg(args, 1), Neighbour::Lower)?;
    object(element)
}

/// A copy of the set holding the elements before `element` (`headSet`), from `element` on
/// (`tailSet`), or between two elements (`subSet`).
fn tree_set_range(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    from: Option<ObjectRef>,
    to: Option<ObjectRef>,
) -> Result<ObjectRef, VmError> {
    let ordering = ordering_of(vm, this);
    let range = new_sorted_map(vm, "java/util/TreeSet", ordering)?;
    let present = hash_set_present(vm)?;
    for element in keys(vm, this) {
        if let Some(from) = from {
            if compare_keys(vm, ordering, element, from)? < 0 {
                continue;
            }
        }
        if let Some(to) = to {
            if compare_keys(vm, ordering, element, to)? >= 0 {
                continue;
            }
        }
        put_value(vm, range, element, present)?;
    }
    Ok(range)
}

/// `TreeSet.headSet(Object)`.
fn tree_set_head_set(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let range = tree_set_range(vm, receiver(args), None, Some(ref_arg(args, 1)))?;
    object(range)
}

/// `TreeSet.tailSet(Object)`.
fn tree_set_tail_set(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let range = tree_set_range(vm, receiver(args), Some(ref_arg(args, 1)), None)?;
    object(range)
}

/// `TreeSet.subSet(Object, Object)`.
fn tree_set_sub_set(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let range = tree_set_range(
        vm,
        receiver(args),
        Some(ref_arg(args, 1)),
        Some(ref_arg(args, 2)),
    )?;
    object(range)
}

/// `TreeSet.descendingSet()`: the set with the order turned around.
fn tree_set_descending_set(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let reversed = new_reversed_order(vm, this)?;
    let set = new_sorted_map(vm, "java/util/TreeSet", reversed)?;
    let present = hash_set_present(vm)?;
    for element in keys(vm, this) {
        put_value(vm, set, element, present)?;
    }
    object(set)
}

const TREE_SET_METHODS: [oxjvm_vm::NativeMethodDef; 33] = [
    method("<init>", "()V", ACC_PUBLIC, tree_set_init),
    method(
        "<init>",
        "(Ljava/util/Comparator;)V",
        ACC_PUBLIC,
        tree_set_init_comparator,
    ),
    method(
        "<init>",
        "(Ljava/util/Collection;)V",
        ACC_PUBLIC,
        tree_set_init_collection,
    ),
    method("add", "(Ljava/lang/Object;)Z", ACC_PUBLIC, tree_set_add),
    method(
        "remove",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        tree_set_remove,
    ),
    method(
        "contains",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        tree_set_contains,
    ),
    method("size", "()I", ACC_PUBLIC, |vm, _, args| {
        int(size_of(vm, receiver(args)))
    }),
    method("isEmpty", "()Z", ACC_PUBLIC, |vm, _, args| {
        boolean(size_of(vm, receiver(args)) == 0)
    }),
    method("clear", "()V", ACC_PUBLIC, |vm, _, args| {
        if let Some(map) = vm.sorted_map_mut(receiver(args)) {
            map.clear();
        }
        void()
    }),
    method(
        "iterator",
        "()Ljava/util/Iterator;",
        ACC_PUBLIC,
        tree_set_iterator,
    ),
    method(
        "addAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC,
        |vm, _, args| tree_set_add_all(vm, receiver(args), ref_arg(args, 1)),
    ),
    method(
        "containsAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let source = ref_arg(args, 1);
            for element in collection_elements(vm, source)? {
                let found = search_key(vm, receiver(args), element)?.1;
                if !found {
                    return boolean(false);
                }
            }
            boolean(true)
        },
    ),
    method(
        "removeAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC,
        collection_remove_all,
    ),
    method(
        "retainAll",
        "(Ljava/util/Collection;)Z",
        ACC_PUBLIC,
        collection_retain_all,
    ),
    method(
        "toArray",
        "()[Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let object_class = vm.resolve_class("java/lang/Object")?;
            let elements = keys(vm, receiver(args));
            let array = vm.allocate_object_array(object_class, elements.len())?;
            for (index, element) in elements.iter().enumerate() {
                vm.array_set_ref(array, index, *element)?;
            }
            object(array)
        },
    ),
    method(
        "toArray",
        "([Ljava/lang/Object;)[Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let elements = keys(vm, receiver(args));
            collection_to_array_into(vm, &elements, ref_arg(args, 1))
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        tree_set_to_string,
    ),
    method("hashCode", "()I", ACC_PUBLIC, tree_set_hash_code),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        tree_set_equals,
    ),
    method(
        "forEach",
        "(Ljava/util/function/Consumer;)V",
        ACC_PUBLIC,
        tree_set_for_each,
    ),
    method(
        "comparator",
        "()Ljava/util/Comparator;",
        ACC_PUBLIC,
        |vm, _, args| object(ordering_of(vm, receiver(args))),
    ),
    method("first", "()Ljava/lang/Object;", ACC_PUBLIC, tree_set_first),
    method("last", "()Ljava/lang/Object;", ACC_PUBLIC, tree_set_last),
    method(
        "pollFirst",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_set_poll_first,
    ),
    method(
        "pollLast",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_set_poll_last,
    ),
    method(
        "ceiling",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_set_ceiling,
    ),
    method(
        "floor",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_set_floor,
    ),
    method(
        "higher",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_set_higher,
    ),
    method(
        "lower",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        ACC_PUBLIC,
        tree_set_lower,
    ),
    method(
        "headSet",
        "(Ljava/lang/Object;)Ljava/util/SortedSet;",
        ACC_PUBLIC,
        tree_set_head_set,
    ),
    method(
        "tailSet",
        "(Ljava/lang/Object;)Ljava/util/SortedSet;",
        ACC_PUBLIC,
        tree_set_tail_set,
    ),
    method(
        "subSet",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/util/SortedSet;",
        ACC_PUBLIC,
        tree_set_sub_set,
    ),
    method(
        "descendingSet",
        "()Ljava/util/NavigableSet;",
        ACC_PUBLIC,
        tree_set_descending_set,
    ),
];

/// `java.util.TreeSet`: a set whose elements are kept in order.
pub(crate) const TREE_SET: oxjvm_vm::NativeClass = class(
    "java/util/TreeSet",
    Some("java/lang/Object"),
    &[
        "java/util/NavigableSet",
        "java/util/Set",
        "java/lang/Cloneable",
        "java/io/Serializable",
    ],
    ACC_PUBLIC,
    &[],
    &TREE_SET_METHODS,
    None,
);

// -------------------------------------------------------------------------------------------
// The orderings the sorted views need
// -------------------------------------------------------------------------------------------

/// A `Comparator` that orders the entries of a map by their keys.
fn new_entry_order(vm: &mut Vm<'_>, map: ObjectRef) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/OxjvmEntryOrder")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "source", "Ljava/lang/Object;")?;
    vm.set_instance_ref(instance, declaring, field, map);
    Ok(instance)
}

/// `OxjvmEntryOrder.compare(Object, Object)`: compare two entries by their keys.
fn entry_order_compare(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let left = entry_key(vm, ref_arg(args, 1))?;
    let right = entry_key(vm, ref_arg(args, 2))?;
    let source = vm
        .read_ref_field(this, vm.class_of(this), "source", "Ljava/lang/Object;")
        .unwrap_or(ObjectRef::NULL);
    let ordering = ordering_of(vm, source);
    int(compare_keys(vm, ordering, left, right)?)
}

const ENTRY_ORDER_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "compare",
    "(Ljava/lang/Object;Ljava/lang/Object;)I",
    ACC_PUBLIC,
    entry_order_compare,
)];

/// The `Comparator` that orders the entries of a map by their keys.
pub(crate) const OXJVM_ENTRY_ORDER: oxjvm_vm::NativeClass = class(
    "java/util/OxjvmEntryOrder",
    Some("java/lang/Object"),
    &["java/util/Comparator"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("source", "Ljava/lang/Object;", ACC_PRIVATE, None)],
    &ENTRY_ORDER_METHODS,
    None,
);

/// A `Comparator` that turns another ordering around.
fn new_reversed_order(vm: &mut Vm<'_>, source: ObjectRef) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/OxjvmReversedOrder")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "source", "Ljava/lang/Object;")?;
    vm.set_instance_ref(instance, declaring, field, source);
    Ok(instance)
}

/// `OxjvmReversedOrder.compare(Object, Object)`: the source's order, turned around.
fn reversed_order_compare(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let source = vm
        .read_ref_field(this, vm.class_of(this), "source", "Ljava/lang/Object;")
        .unwrap_or(ObjectRef::NULL);
    let ordering = ordering_of(vm, source);
    let result = compare_keys(vm, ordering, ref_arg(args, 1), ref_arg(args, 2))?;
    int(result.saturating_neg())
}

const REVERSED_ORDER_METHODS: [oxjvm_vm::NativeMethodDef; 1] = [method(
    "compare",
    "(Ljava/lang/Object;Ljava/lang/Object;)I",
    ACC_PUBLIC,
    reversed_order_compare,
)];

/// The `Comparator` that turns a map's order around.
pub(crate) const OXJVM_REVERSED_ORDER: oxjvm_vm::NativeClass = class(
    "java/util/OxjvmReversedOrder",
    Some("java/lang/Object"),
    &["java/util/Comparator"],
    ACC_PUBLIC | ACC_FINAL | ACC_SYNTHETIC,
    &[field("source", "Ljava/lang/Object;", ACC_PRIVATE, None)],
    &REVERSED_ORDER_METHODS,
    None,
);
