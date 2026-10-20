//! `java.util.TreeMap` and `java.util.TreeSet`: the sorted collections, backed by a Rust
//! `BTreeMap` in the VM's ordered-map payload.

use oxjvm_platform::{Host, HostError, MemoryClasses, Stream};
use oxjvm_vm::{ObjectRef, Value, Vm};

/// A host that knows no classes and swallows the output.
#[derive(Default)]
struct TestHost {
    classes: MemoryClasses,
}

impl Host for TestHost {
    fn load_class(&mut self, internal_name: &str) -> Option<Vec<u8>> {
        self.classes.get(internal_name).map(<[u8]>::to_vec)
    }

    fn write(&mut self, _stream: Stream, _bytes: &[u8]) -> Result<(), HostError> {
        Ok(())
    }
}

/// Call a method on a guest object.
fn call(
    vm: &mut Vm<'_>,
    target: ObjectRef,
    name: &str,
    descriptor: &str,
    arguments: Vec<Value>,
) -> Value {
    let (class, method) = vm
        .resolve_virtual_method(target, name, descriptor)
        .expect(name);
    let mut args = vec![Value::Ref(target)];
    args.extend(arguments);
    vm.invoke_method(class, method, args).expect(name)
}

/// Call a static method.
fn call_static(
    vm: &mut Vm<'_>,
    class_name: &str,
    name: &str,
    descriptor: &str,
    arguments: Vec<Value>,
) -> Value {
    let class = vm.resolve_class(class_name).expect(class_name);
    let (declaring, method) = vm.find_method(class, name, descriptor).expect(name);
    vm.invoke_method(declaring, method, arguments).expect(name)
}

/// A guest string.
fn string(vm: &mut Vm<'_>, text: &str) -> ObjectRef {
    vm.make_string(text).expect("string")
}

/// The text of a guest string.
fn text(vm: &mut Vm<'_>, reference: ObjectRef) -> String {
    vm.string_or_empty(reference)
}

/// A boxed `Integer`.
fn integer(vm: &mut Vm<'_>, value: i32) -> ObjectRef {
    call_static(
        vm,
        "java/lang/Integer",
        "valueOf",
        "(I)Ljava/lang/Integer;",
        vec![Value::Int(value)],
    )
    .as_ref()
}

/// Every element of a guest collection, in the collection's own order.
fn elements(vm: &mut Vm<'_>, collection: ObjectRef) -> Vec<ObjectRef> {
    let iterator = call(vm, collection, "iterator", "()Ljava/util/Iterator;", vec![]).as_ref();
    let mut found = Vec::new();
    loop {
        if call(vm, iterator, "hasNext", "()Z", vec![]).as_int() == 0 {
            return found;
        }
        found.push(call(vm, iterator, "next", "()Ljava/lang/Object;", vec![]).as_ref());
    }
}

/// The text of every element of a guest collection.
fn element_texts(vm: &mut Vm<'_>, collection: ObjectRef) -> Vec<String> {
    let found = elements(vm, collection);
    found.into_iter().map(|element| text(vm, element)).collect()
}

/// A fresh `TreeMap`.
fn tree_map(vm: &mut Vm<'_>) -> ObjectRef {
    let class = vm.resolve_class("java/util/TreeMap").expect("TreeMap");
    let instance = vm.new_instance(class).expect("instance");
    let (declaring, constructor) = vm.find_method(class, "<init>", "()V").expect("<init>");
    vm.invoke_method(declaring, constructor, vec![Value::Ref(instance)])
        .expect("init");
    instance
}

/// A `TreeMap` that orders its keys with `comparator`.
fn tree_map_with(vm: &mut Vm<'_>, comparator: ObjectRef) -> ObjectRef {
    let class = vm.resolve_class("java/util/TreeMap").expect("TreeMap");
    let instance = vm.new_instance(class).expect("instance");
    let (declaring, constructor) = vm
        .find_method(class, "<init>", "(Ljava/util/Comparator;)V")
        .expect("<init>");
    vm.invoke_method(
        declaring,
        constructor,
        vec![Value::Ref(instance), Value::Ref(comparator)],
    )
    .expect("init");
    instance
}

/// A fresh `TreeSet`.
fn tree_set(vm: &mut Vm<'_>) -> ObjectRef {
    let class = vm.resolve_class("java/util/TreeSet").expect("TreeSet");
    let instance = vm.new_instance(class).expect("instance");
    let (declaring, constructor) = vm.find_method(class, "<init>", "()V").expect("<init>");
    vm.invoke_method(declaring, constructor, vec![Value::Ref(instance)])
        .expect("init");
    instance
}

/// Put a key/value pair into a map.
fn put(vm: &mut Vm<'_>, map: ObjectRef, key: ObjectRef, value: ObjectRef) -> Value {
    call(
        vm,
        map,
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        vec![Value::Ref(key), Value::Ref(value)],
    )
}

/// The key a map's `firstKey`/`lastKey`/`floorKey`/... returns.
fn map_key(vm: &mut Vm<'_>, map: ObjectRef, name: &str, key: Option<ObjectRef>) -> ObjectRef {
    let (descriptor, arguments) = match key {
        Some(key) => (
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            vec![Value::Ref(key)],
        ),
        None => ("()Ljava/lang/Object;", vec![]),
    };
    call(vm, map, name, descriptor, arguments).as_ref()
}

#[test]
fn tree_map_keeps_its_keys_in_order() {
    let mut host = TestHost::default();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let map = tree_map(&mut vm);

    for word in ["delta", "alpha", "charlie", "bravo"] {
        let key = string(&mut vm, word);
        let value = string(&mut vm, word);
        put(&mut vm, map, key, value);
    }

    assert_eq!(call(&mut vm, map, "size", "()I", vec![]).as_int(), 4);
    let first = map_key(&mut vm, map, "firstKey", None);
    assert_eq!(text(&mut vm, first), "alpha");
    let last = map_key(&mut vm, map, "lastKey", None);
    assert_eq!(text(&mut vm, last), "delta");

    // The key set walks the map in the keys' order, not the insertion order.
    let keys = call(&mut vm, map, "keySet", "()Ljava/util/Set;", vec![]).as_ref();
    assert_eq!(
        element_texts(&mut vm, keys),
        ["alpha", "bravo", "charlie", "delta"]
    );
}

#[test]
fn tree_map_reads_and_writes_entries() {
    let mut host = TestHost::default();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let map = tree_map(&mut vm);

    let key = string(&mut vm, "key");
    let value = string(&mut vm, "value");
    let previous = put(&mut vm, map, key, value);
    assert!(
        previous.as_ref().is_null(),
        "the first put has no previous value"
    );

    let found = call(
        &mut vm,
        map,
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        vec![Value::Ref(key)],
    )
    .as_ref();
    assert_eq!(text(&mut vm, found), "value");

    let missing = string(&mut vm, "missing");
    let contains = call(
        &mut vm,
        map,
        "containsKey",
        "(Ljava/lang/Object;)Z",
        vec![Value::Ref(missing)],
    );
    assert_eq!(contains.as_int(), 0);

    // Putting the same key again replaces the value.
    let replacement = string(&mut vm, "other");
    let previous = put(&mut vm, map, key, replacement);
    assert_eq!(text(&mut vm, previous.as_ref()), "value");
    assert_eq!(
        call(&mut vm, map, "size", "()I", vec![]).as_int(),
        1,
        "the key was replaced, not added"
    );

    let removed = call(
        &mut vm,
        map,
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        vec![Value::Ref(key)],
    )
    .as_ref();
    assert_eq!(text(&mut vm, removed), "other");
    assert_eq!(
        call(&mut vm, map, "isEmpty", "()Z", vec![]).as_int(),
        1,
        "the map is empty again"
    );
}

#[test]
fn tree_map_orders_by_the_keys_own_comparison() {
    let mut host = TestHost::default();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let map = tree_map(&mut vm);

    for number in [30, 10, 20] {
        let key = integer(&mut vm, number);
        put(&mut vm, map, key, key);
    }

    let first = map_key(&mut vm, map, "firstKey", None);
    assert_eq!(call(&mut vm, first, "intValue", "()I", vec![]).as_int(), 10);
    let last = map_key(&mut vm, map, "lastKey", None);
    assert_eq!(call(&mut vm, last, "intValue", "()I", vec![]).as_int(), 30);
}

#[test]
fn tree_map_honours_a_comparator() {
    let mut host = TestHost::default();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let comparator = call_static(
        &mut vm,
        "java/util/Collections",
        "reverseOrder",
        "()Ljava/util/Comparator;",
        vec![],
    )
    .as_ref();
    let map = tree_map_with(&mut vm, comparator);

    for word in ["alpha", "charlie", "bravo"] {
        let key = string(&mut vm, word);
        put(&mut vm, map, key, key);
    }

    let first = map_key(&mut vm, map, "firstKey", None);
    assert_eq!(text(&mut vm, first), "charlie");
    let last = map_key(&mut vm, map, "lastKey", None);
    assert_eq!(text(&mut vm, last), "alpha");
}

#[test]
fn tree_map_views_follow_the_order() {
    let mut host = TestHost::default();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let map = tree_map(&mut vm);

    for word in ["a", "b", "c", "d"] {
        let key = string(&mut vm, word);
        put(&mut vm, map, key, key);
    }

    let values = call(&mut vm, map, "values", "()Ljava/util/Collection;", vec![]).as_ref();
    assert_eq!(element_texts(&mut vm, values), ["a", "b", "c", "d"]);

    // The entries are `Map.Entry` objects, in the map's order.
    let entries = call(&mut vm, map, "entrySet", "()Ljava/util/Set;", vec![]).as_ref();
    let found = elements(&mut vm, entries);
    let mut keys = Vec::new();
    for entry in found {
        let key = call(&mut vm, entry, "getKey", "()Ljava/lang/Object;", vec![]).as_ref();
        keys.push(text(&mut vm, key));
    }
    assert_eq!(keys, ["a", "b", "c", "d"]);

    // The neighbours of a key.
    let middle = string(&mut vm, "b");
    let floor = map_key(&mut vm, map, "floorKey", Some(middle));
    assert_eq!(text(&mut vm, floor), "b");
    let higher = map_key(&mut vm, map, "higherKey", Some(middle));
    assert_eq!(text(&mut vm, higher), "c");
    let lower = map_key(&mut vm, map, "lowerKey", Some(middle));
    assert_eq!(text(&mut vm, lower), "a");
}

#[test]
fn tree_map_ranges_carry_the_order() {
    let mut host = TestHost::default();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let map = tree_map(&mut vm);

    for word in ["a", "b", "c", "d"] {
        let key = string(&mut vm, word);
        put(&mut vm, map, key, key);
    }

    let bound = string(&mut vm, "c");
    let head = call(
        &mut vm,
        map,
        "headMap",
        "(Ljava/lang/Object;)Ljava/util/SortedMap;",
        vec![Value::Ref(bound)],
    )
    .as_ref();
    let keys = call(&mut vm, head, "keySet", "()Ljava/util/Set;", vec![]).as_ref();
    assert_eq!(element_texts(&mut vm, keys), ["a", "b"]);
}

#[test]
fn tree_set_keeps_its_elements_in_order() {
    let mut host = TestHost::default();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let set = tree_set(&mut vm);

    for word in ["delta", "alpha", "charlie", "bravo"] {
        let element = string(&mut vm, word);
        let added = call(
            &mut vm,
            set,
            "add",
            "(Ljava/lang/Object;)Z",
            vec![Value::Ref(element)],
        );
        assert_eq!(added.as_int(), 1, "{word} is new");
    }

    // Adding the same element again changes nothing.
    let again = string(&mut vm, "alpha");
    let repeated = call(
        &mut vm,
        set,
        "add",
        "(Ljava/lang/Object;)Z",
        vec![Value::Ref(again)],
    );
    assert_eq!(repeated.as_int(), 0);
    assert_eq!(call(&mut vm, set, "size", "()I", vec![]).as_int(), 4);

    assert_eq!(
        element_texts(&mut vm, set),
        ["alpha", "bravo", "charlie", "delta"]
    );

    let first = call(&mut vm, set, "first", "()Ljava/lang/Object;", vec![]).as_ref();
    assert_eq!(text(&mut vm, first), "alpha");
    let last = call(&mut vm, set, "last", "()Ljava/lang/Object;", vec![]).as_ref();
    assert_eq!(text(&mut vm, last), "delta");

    // Removing an element takes it out of the set.
    let removed = string(&mut vm, "charlie");
    let gone = call(
        &mut vm,
        set,
        "remove",
        "(Ljava/lang/Object;)Z",
        vec![Value::Ref(removed)],
    );
    assert_eq!(gone.as_int(), 1);
    assert_eq!(element_texts(&mut vm, set), ["alpha", "bravo", "delta"]);
}

#[test]
fn the_map_survives_a_collection() {
    let mut host = TestHost::default();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let map = tree_map(&mut vm);

    // Enough entries to make the map re-space its ranks a few times.
    for number in 0..64 {
        let key = integer(&mut vm, number);
        put(&mut vm, map, key, key);
    }
    assert_eq!(call(&mut vm, map, "size", "()I", vec![]).as_int(), 64);

    // The entries come out in the keys' order.
    let keys = call(&mut vm, map, "keySet", "()Ljava/util/Set;", vec![]).as_ref();
    let found = elements(&mut vm, keys);
    let mut ordered = Vec::new();
    for key in found {
        ordered.push(call(&mut vm, key, "intValue", "()I", vec![]).as_int());
    }
    let expected: Vec<i32> = (0..64).collect();
    assert_eq!(ordered, expected);

    // Every key is still found.
    for number in 0..64 {
        let key = integer(&mut vm, number);
        let contains = call(
            &mut vm,
            map,
            "containsKey",
            "(Ljava/lang/Object;)Z",
            vec![Value::Ref(key)],
        );
        assert_eq!(contains.as_int(), 1, "{number} is in the map");
    }
}

#[test]
fn the_map_survives_the_collector() {
    let mut host = TestHost::default();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let map = tree_map(&mut vm);
    // The map is only reachable from this test, so it needs a root of its own.
    vm.protect(map);

    for word in ["a", "b", "c"] {
        let key = string(&mut vm, word);
        let value = string(&mut vm, word);
        put(&mut vm, map, key, value);
    }

    // The collector has to see the keys and the values the payload holds.
    call_static(&mut vm, "java/lang/System", "gc", "()V", vec![]);

    assert_eq!(call(&mut vm, map, "size", "()I", vec![]).as_int(), 3);
    let first = map_key(&mut vm, map, "firstKey", None);
    assert_eq!(text(&mut vm, first), "a");
    let last = map_key(&mut vm, map, "lastKey", None);
    assert_eq!(text(&mut vm, last), "c");

    // The values are there as well.
    let key = string(&mut vm, "b");
    let value = call(
        &mut vm,
        map,
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        vec![Value::Ref(key)],
    )
    .as_ref();
    assert_eq!(text(&mut vm, value), "b");

    vm.unprotect(map);
}

#[test]
fn the_descending_map_turns_the_order_around() {
    let mut host = TestHost::default();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let map = tree_map(&mut vm);

    for word in ["alpha", "bravo", "charlie"] {
        let key = string(&mut vm, word);
        put(&mut vm, map, key, key);
    }

    let descending = call(
        &mut vm,
        map,
        "descendingMap",
        "()Ljava/util/NavigableMap;",
        vec![],
    )
    .as_ref();
    let keys = call(&mut vm, descending, "keySet", "()Ljava/util/Set;", vec![]).as_ref();
    assert_eq!(element_texts(&mut vm, keys), ["charlie", "bravo", "alpha"]);
}

#[test]
fn a_map_copies_another_one() {
    let mut host = TestHost::default();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let source = tree_map(&mut vm);

    for word in ["delta", "alpha", "charlie"] {
        let key = string(&mut vm, word);
        put(&mut vm, source, key, key);
    }

    // `new TreeMap(source)` and `putAll` walk the source in its own order.
    let class = vm.resolve_class("java/util/TreeMap").expect("TreeMap");
    let copy = vm.new_instance(class).expect("instance");
    let (declaring, constructor) = vm
        .find_method(class, "<init>", "(Ljava/util/Map;)V")
        .expect("<init>");
    vm.invoke_method(
        declaring,
        constructor,
        vec![Value::Ref(copy), Value::Ref(source)],
    )
    .expect("init");
    assert_eq!(call(&mut vm, copy, "size", "()I", vec![]).as_int(), 3);
    let keys = call(&mut vm, copy, "keySet", "()Ljava/util/Set;", vec![]).as_ref();
    assert_eq!(element_texts(&mut vm, keys), ["alpha", "charlie", "delta"]);

    let other = tree_map(&mut vm);
    call(
        &mut vm,
        other,
        "putAll",
        "(Ljava/util/Map;)V",
        vec![Value::Ref(source)],
    );
    assert_eq!(call(&mut vm, other, "size", "()I", vec![]).as_int(), 3);
}

#[test]
fn the_map_computes_values() {
    let mut host = TestHost::default();
    let mut vm = Vm::new(&mut host, oxjvm_java::natives());
    let map = tree_map(&mut vm);

    // `putIfAbsent` only fills in a key the map does not have.
    let key = string(&mut vm, "key");
    let value = string(&mut vm, "value");
    let first = call(
        &mut vm,
        map,
        "putIfAbsent",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        vec![Value::Ref(key), Value::Ref(value)],
    )
    .as_ref();
    assert!(first.is_null(), "the key was new");
    let second = call(
        &mut vm,
        map,
        "putIfAbsent",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        vec![Value::Ref(key), Value::Ref(value)],
    )
    .as_ref();
    assert_eq!(text(&mut vm, second), "value");
    assert_eq!(call(&mut vm, map, "size", "()I", vec![]).as_int(), 1);

    // `merge` fills in a key the map does not have without asking the function.
    let other = string(&mut vm, "other");
    let merged = call(
        &mut vm,
        map,
        "merge",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;",
        vec![
            Value::Ref(other),
            Value::Ref(value),
            Value::Ref(ObjectRef::NULL),
        ],
    )
    .as_ref();
    assert_eq!(text(&mut vm, merged), "value");
    assert_eq!(call(&mut vm, map, "size", "()I", vec![]).as_int(), 2);
}
