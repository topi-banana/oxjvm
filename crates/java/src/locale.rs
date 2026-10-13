//! `java.util.Locale`: a minimal implementation with the common constants.

use alloc::string::{String, ToString};
use alloc::vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, int, method, object, receiver, ref_arg, string_arg, void};

const LOCALE_METHODS: [oxjvm_vm::NativeMethodDef; 14] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| locale_init(vm, args, 1, 1),
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| locale_init(vm, args, 1, 2),
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| locale_init(vm, args, 1, 3),
    ),
    method(
        "getDefault",
        "()Ljava/util/Locale;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/Locale")?;
            let us = vm
                .read_static_ref_if_set(class, "US", "Ljava/util/Locale;")
                .unwrap_or(ObjectRef::NULL);
            if us.is_null() {
                // Fall back to constructing en_US on the spot.
                let instance = vm.new_instance(class)?;
                let (declaring, ctor) = vm
                    .find_method(class, "<init>", "(Ljava/lang/String;Ljava/lang/String;)V")
                    .ok_or_else(|| VmError::internal("Locale lacks its constructor"))?;
                let language = vm.make_string("en")?;
                let country = vm.make_string("US")?;
                vm.invoke_method(
                    declaring,
                    ctor,
                    vec![
                        Value::Ref(instance),
                        Value::Ref(language),
                        Value::Ref(country),
                    ],
                )?;
                return object(instance);
            }
            object(us)
        },
    ),
    method(
        "getLanguage",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| locale_string_field(vm, receiver(args), "language"),
    ),
    method(
        "getCountry",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| locale_string_field(vm, receiver(args), "country"),
    ),
    method(
        "getVariant",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| locale_string_field(vm, receiver(args), "variant"),
    ),
    method(
        "getScript",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, _| object(vm.make_string("")?),
    ),
    method(
        "getDisplayName",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        locale_to_string,
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        locale_to_string,
    ),
    method(
        "toLanguageTag",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        locale_to_language_tag,
    ),
    method(
        "forLanguageTag",
        "(Ljava/lang/String;)Ljava/util/Locale;",
        ACC_PUBLIC | ACC_STATIC,
        locale_for_language_tag,
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(other) != vm.class_of(receiver(args)) {
                return boolean(false);
            }
            let a = locale_string_field(vm, receiver(args), "language")?;
            let b = locale_string_field(vm, other, "language")?;
            let same_language = vm.string_value(a.as_ref()) == vm.string_value(b.as_ref());
            let a = locale_string_field(vm, receiver(args), "country")?;
            let b = locale_string_field(vm, other, "country")?;
            boolean(same_language && vm.string_value(a.as_ref()) == vm.string_value(b.as_ref()))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let language = vm
            .read_string_field(this, vm.class_of(this), "language")
            .unwrap_or_default();
        let country = vm
            .read_string_field(this, vm.class_of(this), "country")
            .unwrap_or_default();
        let mut hash = 0i32;
        for ch in language.encode_utf16().chain(country.encode_utf16()) {
            hash = hash.wrapping_mul(31).wrapping_add(i32::from(ch));
        }
        int(hash)
    }),
];

pub(crate) const LOCALE: oxjvm_vm::NativeClass = class(
    "java/util/Locale",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("language", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("country", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("variant", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "ROOT",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ENGLISH",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "FRENCH",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "GERMAN",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ITALIAN",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "JAPANESE",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "KOREAN",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "CHINESE",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "US",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "UK",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "CANADA",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "FRANCE",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "GERMANY",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "ITALY",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "JAPAN",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "KOREA",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "CHINA",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &LOCALE_METHODS,
    Some(locale_clinit),
);

fn locale_init(
    vm: &mut Vm<'_>,
    args: &[Value],
    language_index: usize,
    field_count: usize,
) -> Result<Value, VmError> {
    let this = receiver(args);
    let class = vm.class_of(this);
    for (offset, (name, descriptor)) in [
        ("language", "Ljava/lang/String;"),
        ("country", "Ljava/lang/String;"),
    ]
    .iter()
    .enumerate()
    {
        if offset >= field_count {
            break;
        }
        let (declaring, field) = vm.find_field(class, name, descriptor)?;
        let value = if offset + language_index < args.len() {
            ref_arg(args, offset + language_index)
        } else {
            ObjectRef::NULL
        };
        vm.set_instance_ref(this, declaring, field, value);
    }
    void()
}

fn locale_string_field(vm: &mut Vm<'_>, this: ObjectRef, name: &str) -> Result<Value, VmError> {
    let class = vm.class_of(this);
    match vm.read_ref_field(this, class, name, "Ljava/lang/String;") {
        Some(reference) if !reference.is_null() => object(reference),
        _ => object(vm.make_string("")?),
    }
}

fn locale_to_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let language = vm
        .read_string_field(this, vm.class_of(this), "language")
        .unwrap_or_default();
    let country = vm
        .read_string_field(this, vm.class_of(this), "country")
        .unwrap_or_default();
    let variant = vm
        .read_string_field(this, vm.class_of(this), "variant")
        .unwrap_or_default();
    let mut text = language;
    if !country.is_empty() {
        text.push('_');
        text.push_str(&country);
    }
    if !variant.is_empty() {
        text.push('_');
        text.push_str(&variant);
    }
    object(vm.make_string(&text)?)
}

fn locale_to_language_tag(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let language = vm
        .read_string_field(this, vm.class_of(this), "language")
        .unwrap_or_default();
    let country = vm
        .read_string_field(this, vm.class_of(this), "country")
        .unwrap_or_default();
    let text = if language.is_empty() {
        String::from("und")
    } else if country.is_empty() {
        language
    } else {
        alloc::format!("{language}-{country}")
    };
    object(vm.make_string(&text)?)
}

fn locale_for_language_tag(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let tag = string_arg(vm, args[0])?;
    let (language, rest) = match tag.split_once('-') {
        Some((language, rest)) => (language.to_string(), rest.to_string()),
        None => (tag.clone(), String::new()),
    };
    let country = if rest.is_empty() {
        String::new()
    } else {
        rest.split('-').next().unwrap_or("").to_string()
    };
    let class = vm.resolve_class("java/util/Locale")?;
    let instance = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "(Ljava/lang/String;Ljava/lang/String;)V")
        .ok_or_else(|| VmError::internal("Locale lacks its constructor"))?;
    let language = vm.make_string(&language)?;
    let country = vm.make_string(&country)?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![
            Value::Ref(instance),
            Value::Ref(language),
            Value::Ref(country),
        ],
    )?;
    object(instance)
}

fn locale_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/util/Locale")?;
    let constants: [(&str, &str, &str); 17] = [
        ("ROOT", "", ""),
        ("ENGLISH", "en", ""),
        ("FRENCH", "fr", ""),
        ("GERMAN", "de", ""),
        ("ITALIAN", "it", ""),
        ("JAPANESE", "ja", ""),
        ("KOREAN", "ko", ""),
        ("CHINESE", "zh", ""),
        ("US", "en", "US"),
        ("UK", "en", "GB"),
        ("CANADA", "en", "CA"),
        ("FRANCE", "fr", "FR"),
        ("GERMANY", "de", "DE"),
        ("ITALY", "it", "IT"),
        ("JAPAN", "ja", "JP"),
        ("KOREA", "ko", "KR"),
        ("CHINA", "zh", "CN"),
    ];
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "(Ljava/lang/String;Ljava/lang/String;)V")
        .ok_or_else(|| VmError::internal("Locale lacks its constructor"))?;
    for (name, language, country) in constants.iter() {
        let instance = vm.new_instance(class)?;
        let language = vm.make_string(language)?;
        let country = vm.make_string(country)?;
        vm.invoke_method(
            declaring,
            ctor,
            vec![
                Value::Ref(instance),
                Value::Ref(language),
                Value::Ref(country),
            ],
        )?;
        vm.set_static_value(class, name, "Ljava/util/Locale;", Value::Ref(instance));
    }
    Ok(())
}
