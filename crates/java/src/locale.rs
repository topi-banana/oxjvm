//! `java.util.Locale`: a minimal implementation with the common constants.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, int, method, object, receiver, ref_arg, string_arg, void};

const LOCALE_METHODS: [oxjvm_vm::NativeMethodDef; 41] = [
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
        |vm, _, _| locale_default(vm),
    ),
    method(
        "getLanguage",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| locale_string_field(vm, receiver(args), "language"),
    ),
    method(
        "getISO3Language",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let language_value = locale_string_field(vm, receiver(args), "language")?;
            let language = string_arg(vm, language_value)?;
            let iso3 = iso3_language(&language);
            if iso3.is_empty() {
                return Err(vm.throw_new("java/util/MissingResourceException", None));
            }
            object(vm.make_string(iso3)?)
        },
    ),
    method(
        "getISO3Country",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| crate::locale_iso::locale_get_iso3_country(vm, args),
    ),
    method(
        "getISOCountries",
        "()[Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| crate::locale_iso::locale_get_iso_countries(vm),
    ),
    method(
        "getISOCountries",
        "(Ljava/util/Locale$IsoCountryCode;)Ljava/util/Set;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| crate::locale_iso::locale_get_iso_countries_typed(vm, args),
    ),
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.resolve_class("java/util/Locale")?;
            let copy = vm.new_instance(class)?;
            for (name, descriptor) in [
                ("language", "Ljava/lang/String;"),
                ("country", "Ljava/lang/String;"),
                ("variant", "Ljava/lang/String;"),
                ("languageTag", "Ljava/lang/String;"),
            ] {
                if let Some(value) = vm.read_ref_field(this, vm.class_of(this), name, descriptor) {
                    if let Some((declaring, field)) = vm.find_field_opt(class, name, descriptor) {
                        vm.set_instance_ref(copy, declaring, field, value);
                    }
                }
            }
            object(copy)
        },
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
        |vm, _, args| crate::locale_display::locale_display_name_target(vm, args, false),
    ),
    method(
        "getDisplayName",
        "(Ljava/util/Locale;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| crate::locale_display::locale_display_name_target(vm, args, true),
    ),
    method(
        "getDisplayLanguage",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| crate::locale_display::locale_display_language(vm, args, false),
    ),
    method(
        "getDisplayLanguage",
        "(Ljava/util/Locale;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| crate::locale_display::locale_display_language(vm, args, true),
    ),
    method(
        "getDisplayCountry",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| crate::locale_display::locale_display_country(vm, args, false),
    ),
    method(
        "getDisplayCountry",
        "(Ljava/util/Locale;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| crate::locale_display::locale_display_country(vm, args, true),
    ),
    method(
        "getDisplayVariant",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| crate::locale_display::locale_display_variant(vm, args, false),
    ),
    method(
        "getDisplayVariant",
        "(Ljava/util/Locale;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| crate::locale_display::locale_display_variant(vm, args, true),
    ),
    method(
        "getDisplayScript",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| crate::locale_display::locale_display_script(vm, args),
    ),
    method(
        "getDisplayScript",
        "(Ljava/util/Locale;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| crate::locale_display::locale_display_script(vm, args),
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
        "filter",
        "(Ljava/util/List;Ljava/util/Collection;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| crate::locale_match::locale_filter(vm, args, false),
    ),
    method(
        "filter",
        "(Ljava/util/List;Ljava/util/Collection;Ljava/util/Locale$FilteringMode;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| crate::locale_match::locale_filter(vm, args, true),
    ),
    method(
        "filterTags",
        "(Ljava/util/List;Ljava/util/Collection;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| crate::locale_match::locale_filter_tags(vm, args, false),
    ),
    method(
        "filterTags",
        "(Ljava/util/List;Ljava/util/Collection;Ljava/util/Locale$FilteringMode;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| crate::locale_match::locale_filter_tags(vm, args, true),
    ),
    method(
        "lookup",
        "(Ljava/util/List;Ljava/util/Collection;)Ljava/util/Locale;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| crate::locale_match::locale_lookup(vm, args),
    ),
    method(
        "lookupTag",
        "(Ljava/util/List;Ljava/util/Collection;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| crate::locale_match::locale_lookup_tag(vm, args),
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
    method(
        "setDefault",
        "(Ljava/util/Locale;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let locale = ref_arg(args, 0);
            if locale.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let class = vm.resolve_class("java/util/Locale")?;
            vm.set_static_value(
                class,
                "defaultLocale",
                "Ljava/util/Locale;",
                Value::Ref(locale),
            );
            vm.set_static_value(
                class,
                "defaultDisplayLocale",
                "Ljava/util/Locale;",
                Value::Ref(locale),
            );
            vm.set_static_value(
                class,
                "defaultFormatLocale",
                "Ljava/util/Locale;",
                Value::Ref(locale),
            );
            void()
        },
    ),
    method(
        "getDefault",
        "(Ljava/util/Locale$Category;)Ljava/util/Locale;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let category = ref_arg(args, 0);
            if category.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let ordinal = crate::lang::read_int_field(vm, category, "ordinal");
            let class = vm.resolve_class("java/util/Locale")?;
            // Each category keeps its own default, falling back to the user default.
            let field_name = if ordinal == 0 {
                "defaultDisplayLocale"
            } else {
                "defaultFormatLocale"
            };
            if let Some(value) = vm.read_static_ref_if_set(class, field_name, "Ljava/util/Locale;")
            {
                return object(value);
            }
            locale_default(vm)
        },
    ),
    method(
        "setDefault",
        "(Ljava/util/Locale$Category;Ljava/util/Locale;)V",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let category = ref_arg(args, 0);
            let locale = ref_arg(args, 1);
            if category.is_null() || locale.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let ordinal = crate::lang::read_int_field(vm, category, "ordinal");
            let class = vm.resolve_class("java/util/Locale")?;
            let field_name = if ordinal == 0 {
                "defaultDisplayLocale"
            } else {
                "defaultFormatLocale"
            };
            vm.set_static_value(class, field_name, "Ljava/util/Locale;", Value::Ref(locale));
            void()
        },
    ),
    method(
        "of",
        "(Ljava/lang/String;)Ljava/util/Locale;",
        ACC_PUBLIC | ACC_STATIC,
        locale_of,
    ),
    method(
        "of",
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/util/Locale;",
        ACC_PUBLIC | ACC_STATIC,
        locale_of,
    ),
    method(
        "of",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/util/Locale;",
        ACC_PUBLIC | ACC_STATIC,
        locale_of,
    ),
    method(
        "getAvailableLocales",
        "()[Ljava/util/Locale;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/Locale")?;
            vm.ensure_initialized(class)?;
            let names = [
                "ROOT", "ENGLISH", "US", "UK", "CANADA", "FRANCE", "GERMANY", "ITALY", "JAPAN",
                "KOREA", "CHINA", "PRC", "TAIWAN",
            ];
            let mut locales = Vec::new();
            for name in names {
                if let Some(value) = vm.read_static_ref_if_set(class, name, "Ljava/util/Locale;") {
                    locales.push(value);
                }
            }
            let array = vm.allocate_object_array(class, locales.len())?;
            for (index, locale) in locales.iter().enumerate() {
                vm.array_set_ref(array, index, *locale)?;
            }
            object(array)
        },
    ),
];

/// `Locale.of(...)`: construct through the matching constructor (the `of`
/// descriptor with its return type replaced by `V`).
fn locale_of(
    vm: &mut Vm<'_>,
    ctx: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let of_descriptor = vm.classes.get(ctx.class).methods[ctx.method as usize]
        .descriptor
        .clone();
    let params = of_descriptor
        .rsplit_once(')')
        .map_or("()".to_string(), |(params, _)| params.to_string());
    let ctor_descriptor = alloc::format!("{params})V");
    let class = vm.resolve_class("java/util/Locale")?;
    let (declaring, constructor) = vm
        .find_method(class, "<init>", &ctor_descriptor)
        .ok_or_else(|| VmError::internal("Locale constructor missing"))?;
    let instance = vm.new_instance(class)?;
    let mut call_args = vec![Value::Ref(instance)];
    call_args.extend_from_slice(args);
    vm.invoke_method(declaring, constructor, call_args)?;
    object(instance)
}

pub(crate) const LOCALE: oxjvm_vm::NativeClass = class(
    "java/util/Locale",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("language", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("country", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("variant", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("languageTag", "Ljava/lang/String;", ACC_PRIVATE, None),
        field(
            "defaultLocale",
            "Ljava/util/Locale;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
        field(
            "defaultDisplayLocale",
            "Ljava/util/Locale;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
        field(
            "defaultFormatLocale",
            "Ljava/util/Locale;",
            ACC_PRIVATE | ACC_STATIC,
            None,
        ),
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
        field(
            "CANADA_FRENCH",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "SIMPLIFIED_CHINESE",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "TRADITIONAL_CHINESE",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "PRC",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
        field(
            "TAIWAN",
            "Ljava/util/Locale;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            None,
        ),
    ],
    &LOCALE_METHODS,
    Some(locale_clinit),
);

/// The default `Locale` (the FORMAT category default), falling back to `en_US`.
fn locale_default(vm: &mut Vm<'_>) -> Result<Value, VmError> {
    let class = vm.resolve_class("java/util/Locale")?;
    if let Some(current) = vm.read_static_ref_if_set(class, "defaultLocale", "Ljava/util/Locale;") {
        return object(current);
    }
    let us = vm
        .read_static_ref_if_set(class, "US", "Ljava/util/Locale;")
        .unwrap_or(ObjectRef::NULL);
    if us.is_null() {
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
}

/// The language of the default (FORMAT category) `Locale`, or "" when unavailable.
pub(crate) fn locale_default_language(vm: &mut Vm<'_>) -> String {
    // Read the cached default (or the US constant) directly; constructing a
    // fresh Locale on every case conversion would be prohibitively slow.
    let Some(class) = vm.resolve_class_lenient_pub("java/util/Locale") else {
        return String::new();
    };
    let locale = vm
        .read_static_ref_if_set(class, "defaultLocale", "Ljava/util/Locale;")
        .or_else(|| vm.read_static_ref_if_set(class, "US", "Ljava/util/Locale;"))
        .unwrap_or(ObjectRef::NULL);
    if locale.is_null() {
        return String::new();
    }
    let language = vm
        .read_ref_field(
            locale,
            vm.class_of(locale),
            "language",
            "Ljava/lang/String;",
        )
        .unwrap_or(ObjectRef::NULL);
    vm.string_or_empty(language)
}

/// The language tag of a `Locale` instance, or "" when null/unavailable.
pub(crate) fn locale_language(vm: &Vm<'_>, locale: ObjectRef) -> String {
    if locale.is_null() {
        return String::new();
    }
    let language = vm
        .read_ref_field(
            locale,
            vm.class_of(locale),
            "language",
            "Ljava/lang/String;",
        )
        .unwrap_or(ObjectRef::NULL);
    vm.string_or_empty(language)
}

const CATEGORY_NAMES: [&str; 2] = ["DISPLAY", "FORMAT"];

/// `java.util.Locale.Category`: the DISPLAY / FORMAT locale selector.
pub(crate) const CATEGORY: oxjvm_vm::NativeClass = class(
    "java/util/Locale$Category",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "DISPLAY",
            "Ljava/util/Locale$Category;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "FORMAT",
            "Ljava/util/Locale$Category;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
    ],
    &CATEGORY_METHODS,
    Some(category_clinit),
);

fn category_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/util/Locale$Category")?;
    for (ordinal, name) in CATEGORY_NAMES.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        if let Some((declaring, field)) = vm.find_field_opt(class, "ordinal", "I") {
            vm.set_instance_int(instance, declaring, field, ordinal as i32);
        }
        vm.set_static_value(
            class,
            name,
            "Ljava/util/Locale$Category;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

const CATEGORY_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "ordinal"))
    }),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let index = crate::lang::read_int_field(vm, receiver(args), "ordinal").clamp(0, 1) as usize;
        object(vm.make_string(CATEGORY_NAMES[index])?)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let index =
                crate::lang::read_int_field(vm, receiver(args), "ordinal").clamp(0, 1) as usize;
            object(vm.make_string(CATEGORY_NAMES[index])?)
        },
    ),
    method(
        "values",
        "()[Ljava/util/Locale$Category;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/Locale$Category")?;
            vm.ensure_initialized(class)?;
            let array = vm.allocate_object_array(class, CATEGORY_NAMES.len())?;
            for (index, name) in CATEGORY_NAMES.iter().enumerate() {
                if let Some(value) =
                    vm.read_static_ref_if_set(class, name, "Ljava/util/Locale$Category;")
                {
                    vm.array_set_ref(array, index, value)?;
                }
            }
            object(array)
        },
    ),
    method(
        "valueOf",
        "(Ljava/lang/String;)Ljava/util/Locale$Category;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let name = vm.string_or_empty(ref_arg(args, 0));
            let class = vm.resolve_class("java/util/Locale$Category")?;
            vm.ensure_initialized(class)?;
            for constant in CATEGORY_NAMES {
                if constant == name {
                    if let Some(value) =
                        vm.read_static_ref_if_set(class, constant, "Ljava/util/Locale$Category;")
                    {
                        return object(value);
                    }
                }
            }
            Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("No enum constant java.util.Locale.Category."),
            ))
        },
    ),
];

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
        ("variant", "Ljava/lang/String;"),
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
        // Normalize case and convert deprecated ISO 639 codes the way the JDK
        // constructor does (java.locale.useOldISOCodes defaults to false).
        let text = vm.string_value(value).unwrap_or_default();
        let normalized = match offset {
            0 => convert_old_language(&text.to_lowercase()),
            1 => text.to_uppercase(),
            _ => text,
        };
        let normalized_ref = vm.make_string(&normalized)?;
        vm.set_instance_ref(this, declaring, field, normalized_ref);
    }
    void()
}

/// Map the deprecated ISO 639-1 codes to their modern replacements.
fn convert_old_language(language: &str) -> String {
    match language {
        "iw" => String::from("he"),
        "ji" => String::from("yi"),
        "in" => String::from("id"),
        other => other.to_string(),
    }
}

/// The ISO 639-2 three-letter code for a two-letter language code.
fn iso3_language(language: &str) -> &'static str {
    match language {
        "aa" => "aar",
        "ab" => "abk",
        "af" => "afr",
        "ak" => "aka",
        "sq" => "sqi",
        "am" => "amh",
        "ar" => "ara",
        "an" => "arg",
        "hy" => "hye",
        "as" => "asm",
        "av" => "ava",
        "ae" => "ave",
        "ay" => "aym",
        "az" => "aze",
        "ba" => "bak",
        "bm" => "bam",
        "eu" => "eus",
        "be" => "bel",
        "bn" => "ben",
        "bh" => "bih",
        "bi" => "bis",
        "bs" => "bos",
        "br" => "bre",
        "bg" => "bul",
        "my" => "mya",
        "ca" => "cat",
        "ch" => "cha",
        "ce" => "che",
        "zh" => "zho",
        "cu" => "chu",
        "cv" => "chv",
        "kw" => "cor",
        "co" => "cos",
        "cr" => "cre",
        "cs" => "ces",
        "da" => "dan",
        "dv" => "div",
        "nl" => "nld",
        "dz" => "dzo",
        "en" => "eng",
        "eo" => "epo",
        "et" => "est",
        "ee" => "ewe",
        "fo" => "fao",
        "fj" => "fij",
        "fi" => "fin",
        "fr" => "fra",
        "fy" => "fry",
        "ff" => "ful",
        "ka" => "kat",
        "de" => "deu",
        "gd" => "gla",
        "ga" => "gle",
        "gl" => "glg",
        "gv" => "glv",
        "el" => "ell",
        "gn" => "grn",
        "gu" => "guj",
        "ht" => "hat",
        "ha" => "hau",
        "he" => "heb",
        "hz" => "her",
        "hi" => "hin",
        "ho" => "hmo",
        "hr" => "hrv",
        "hu" => "hun",
        "ig" => "ibo",
        "is" => "isl",
        "io" => "ido",
        "ii" => "iii",
        "iu" => "iku",
        "ie" => "ile",
        "ia" => "ina",
        "id" => "ind",
        "ik" => "ipk",
        "it" => "ita",
        "jv" => "jav",
        "ja" => "jpn",
        "kl" => "kal",
        "kn" => "kan",
        "ks" => "kas",
        "kr" => "kau",
        "kk" => "kaz",
        "km" => "khm",
        "ki" => "kik",
        "rw" => "kin",
        "ky" => "kir",
        "kv" => "kom",
        "kg" => "kon",
        "ko" => "kor",
        "kj" => "kua",
        "ku" => "kur",
        "lo" => "lao",
        "la" => "lat",
        "lv" => "lav",
        "li" => "lim",
        "ln" => "lin",
        "lt" => "lit",
        "lb" => "ltz",
        "lu" => "lub",
        "lg" => "lug",
        "mk" => "mkd",
        "mh" => "mah",
        "ml" => "mal",
        "mi" => "mri",
        "mr" => "mar",
        "ms" => "msa",
        "mg" => "mlg",
        "mt" => "mlt",
        "mn" => "mon",
        "na" => "nau",
        "nv" => "nav",
        "nr" => "nbl",
        "nd" => "nde",
        "ng" => "ndo",
        "ne" => "nep",
        "nn" => "nno",
        "nb" => "nob",
        "no" => "nor",
        "ny" => "nya",
        "oc" => "oci",
        "oj" => "oji",
        "or" => "ori",
        "om" => "orm",
        "os" => "oss",
        "pa" => "pan",
        "fa" => "fas",
        "pi" => "pli",
        "pl" => "pol",
        "pt" => "por",
        "ps" => "pus",
        "qu" => "que",
        "rm" => "roh",
        "ro" => "ron",
        "rn" => "run",
        "ru" => "rus",
        "sg" => "sag",
        "sa" => "san",
        "si" => "sin",
        "sk" => "slk",
        "sl" => "slv",
        "se" => "sme",
        "sm" => "smo",
        "sn" => "sna",
        "sd" => "snd",
        "so" => "som",
        "st" => "sot",
        "es" => "spa",
        "sc" => "srd",
        "sr" => "srp",
        "ss" => "ssw",
        "su" => "sun",
        "sw" => "swa",
        "sv" => "swe",
        "ty" => "tah",
        "ta" => "tam",
        "tt" => "tat",
        "te" => "tel",
        "tg" => "tgk",
        "tl" => "tgl",
        "th" => "tha",
        "bo" => "bod",
        "ti" => "tir",
        "to" => "ton",
        "tn" => "tsn",
        "ts" => "tso",
        "tk" => "tuk",
        "tr" => "tur",
        "tw" => "twi",
        "ug" => "uig",
        "uk" => "ukr",
        "ur" => "urd",
        "uz" => "uzb",
        "ve" => "ven",
        "vi" => "vie",
        "vo" => "vol",
        "cy" => "cym",
        "wa" => "wln",
        "wo" => "wol",
        "xh" => "xho",
        "yi" => "yid",
        "yo" => "yor",
        "za" => "zha",
        "zu" => "zul",
        "iw" => "heb",
        "in" => "ind",
        "ji" => "yid",
        _ => "",
    }
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
    let class = vm.class_of(this);
    // A locale built by `forLanguageTag` keeps its canonical BCP 47 tag.
    if let Some(tag) = vm.read_string_field(this, class, "languageTag") {
        if !tag.is_empty() {
            return object(vm.make_string(&tag)?);
        }
    }
    let language = vm
        .read_string_field(this, class, "language")
        .unwrap_or_default()
        .to_lowercase();
    let country = vm
        .read_string_field(this, class, "country")
        .unwrap_or_default()
        .to_uppercase();
    let variant = vm
        .read_string_field(this, class, "variant")
        .unwrap_or_default();
    let mut text = if language.is_empty() {
        String::from("und")
    } else {
        language
    };
    if !country.is_empty() {
        text.push('-');
        text.push_str(&country);
    }
    if !variant.is_empty() {
        for part in variant.split('_') {
            text.push('-');
            text.push_str(&part.to_lowercase());
        }
    }
    object(vm.make_string(&text)?)
}

/// Whether every character is an ASCII letter.
fn tag_alpha(subtag: &str) -> bool {
    !subtag.is_empty() && subtag.chars().all(|c| c.is_ascii_alphabetic())
}

/// Whether every character is an ASCII digit.
fn tag_digit(subtag: &str) -> bool {
    !subtag.is_empty() && subtag.chars().all(|c| c.is_ascii_digit())
}

/// Whether every character is an ASCII letter or digit.
fn tag_alphanum(subtag: &str) -> bool {
    !subtag.is_empty() && subtag.chars().all(|c| c.is_ascii_alphanumeric())
}

fn tag_titlecase(subtag: &str) -> String {
    let mut chars = subtag.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
        None => String::new(),
    }
}

/// A BCP 47 variant subtag: 5-8 alphanumerics, or 4 characters starting with a digit.
fn tag_variant(subtag: &str) -> bool {
    (subtag.len() >= 5 && subtag.len() <= 8 && tag_alphanum(subtag))
        || (subtag.len() == 4 && subtag.chars().next().is_some_and(|c| c.is_ascii_digit()))
}

/// Parse a BCP 47 language tag into its canonical form plus the legacy
/// language/country/variant fields, normalizing case the way the JDK does.
fn parse_language_tag(tag: &str) -> (String, String, String, String) {
    let subtags: Vec<&str> = tag.split('-').collect();
    let mut canonical: Vec<String> = Vec::new();
    let mut language = String::new();
    let mut country = String::new();
    let mut variant = String::new();
    let mut i = 0;
    if i < subtags.len() && tag_alpha(subtags[i]) && (2..=8).contains(&subtags[i].len()) {
        language = subtags[i].to_lowercase();
        canonical.push(language.clone());
        i += 1;
    }
    if i < subtags.len() && subtags[i].len() == 4 && tag_alpha(subtags[i]) {
        canonical.push(tag_titlecase(subtags[i]));
        i += 1;
    }
    if i < subtags.len()
        && ((subtags[i].len() == 2 && tag_alpha(subtags[i]))
            || (subtags[i].len() == 3 && tag_digit(subtags[i])))
    {
        country = subtags[i].to_uppercase();
        canonical.push(country.clone());
        i += 1;
    }
    while i < subtags.len() && tag_variant(subtags[i]) {
        let part = subtags[i].to_lowercase();
        if variant.is_empty() {
            variant = part.clone();
        } else {
            variant.push('_');
            variant.push_str(&part);
        }
        canonical.push(part);
        i += 1;
    }
    while i < subtags.len() {
        canonical.push(subtags[i].to_lowercase());
        i += 1;
    }
    (canonical.join("-"), language, country, variant)
}

fn locale_for_language_tag(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let tag = string_arg(vm, args[0])?;
    let (mut canonical, language, country, variant) = parse_language_tag(&tag);
    let language = convert_old_language(&language);
    // Reflect the converted language in the canonical tag's primary subtag.
    if let Some(rest) = canonical.split_once('-') {
        canonical = alloc::format!("{language}-{}", rest.1);
    } else if !canonical.is_empty() {
        canonical = language.clone();
    }
    let class = vm.resolve_class("java/util/Locale")?;
    let instance = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(
            class,
            "<init>",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
        )
        .ok_or_else(|| VmError::internal("Locale lacks its constructor"))?;
    let language_ref = vm.make_string(&language)?;
    let country_ref = vm.make_string(&country)?;
    let variant_ref = vm.make_string(&variant)?;
    vm.invoke_method(
        declaring,
        ctor,
        vec![
            Value::Ref(instance),
            Value::Ref(language_ref),
            Value::Ref(country_ref),
            Value::Ref(variant_ref),
        ],
    )?;
    let tag_ref = vm.make_string(&canonical)?;
    if let Some((declaring, field)) = vm.find_field_opt(class, "languageTag", "Ljava/lang/String;")
    {
        vm.set_instance_ref(instance, declaring, field, tag_ref);
    }
    object(instance)
}

fn locale_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/util/Locale")?;
    let constants: [(&str, &str, &str); 22] = [
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
        ("CANADA_FRENCH", "fr", "CA"),
        ("SIMPLIFIED_CHINESE", "zh", "CN"),
        ("TRADITIONAL_CHINESE", "zh", "TW"),
        ("PRC", "zh", "CN"),
        ("TAIWAN", "zh", "TW"),
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

/// `java.util.Locale$Builder`: builds a locale from its parts.
pub(crate) const LOCALE_BUILDER: oxjvm_vm::NativeClass = class(
    "java/util/Locale$Builder",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("language", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("country", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("variant", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("script", "Ljava/lang/String;", ACC_PRIVATE, None),
    ],
    &LOCALE_BUILDER_METHODS,
    None,
);

fn locale_builder_part(vm: &Vm<'_>, this: ObjectRef, name: &str) -> String {
    vm.read_string_field(this, vm.class_of(this), name)
        .unwrap_or_default()
}

fn locale_builder_set(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    name: &str,
    value: ObjectRef,
) -> Result<Value, VmError> {
    let class = vm.class_of(this);
    if let Some((declaring, field)) = vm.find_field_opt(class, name, "Ljava/lang/String;") {
        vm.set_instance_ref(this, declaring, field, value);
    }
    object(this)
}

const LOCALE_BUILDER_METHODS: [oxjvm_vm::NativeMethodDef; 11] = [
    method("<init>", "()V", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let class = vm.class_of(this);
        for name in ["language", "country", "variant", "script"] {
            let empty = vm.make_string("")?;
            let (declaring, field) = vm.find_field(class, name, "Ljava/lang/String;")?;
            vm.set_instance_ref(this, declaring, field, empty);
        }
        void()
    }),
    method(
        "setLanguage",
        "(Ljava/lang/String;)Ljava/util/Locale$Builder;",
        ACC_PUBLIC,
        |vm, _, args| locale_builder_set(vm, receiver(args), "language", ref_arg(args, 1)),
    ),
    method(
        "setRegion",
        "(Ljava/lang/String;)Ljava/util/Locale$Builder;",
        ACC_PUBLIC,
        |vm, _, args| locale_builder_set(vm, receiver(args), "country", ref_arg(args, 1)),
    ),
    method(
        "setVariant",
        "(Ljava/lang/String;)Ljava/util/Locale$Builder;",
        ACC_PUBLIC,
        |vm, _, args| locale_builder_set(vm, receiver(args), "variant", ref_arg(args, 1)),
    ),
    method(
        "setScript",
        "(Ljava/lang/String;)Ljava/util/Locale$Builder;",
        ACC_PUBLIC,
        |vm, _, args| locale_builder_set(vm, receiver(args), "script", ref_arg(args, 1)),
    ),
    method(
        "setLanguageTag",
        "(Ljava/lang/String;)Ljava/util/Locale$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let tag = vm.string_or_empty(ref_arg(args, 1));
            let (_canonical, language, country, variant) = parse_language_tag(&tag);
            let language = convert_old_language(&language);
            let lang_ref = vm.make_string(&language)?;
            locale_builder_set(vm, this, "language", lang_ref)?;
            let country_ref = vm.make_string(&country)?;
            locale_builder_set(vm, this, "country", country_ref)?;
            let variant_ref = vm.make_string(&variant)?;
            locale_builder_set(vm, this, "variant", variant_ref)?;
            object(this)
        },
    ),
    method(
        "clear",
        "()Ljava/util/Locale$Builder;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            for name in ["language", "country", "variant", "script"] {
                let empty = vm.make_string("")?;
                locale_builder_set(vm, this, name, empty)?;
            }
            object(this)
        },
    ),
    method(
        "clearExtensions",
        "()Ljava/util/Locale$Builder;",
        ACC_PUBLIC,
        |_vm, _, args| object(receiver(args)),
    ),
    method(
        "setExtension",
        "(CLjava/lang/String;)Ljava/util/Locale$Builder;",
        ACC_PUBLIC,
        |_vm, _, args| object(receiver(args)),
    ),
    method(
        "build",
        "()Ljava/util/Locale;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let language = locale_builder_part(vm, this, "language");
            let country = locale_builder_part(vm, this, "country");
            let variant = locale_builder_part(vm, this, "variant");
            let class = vm.resolve_class("java/util/Locale")?;
            let instance = vm.new_instance(class)?;
            let (declaring, ctor) = vm
                .find_method(
                    class,
                    "<init>",
                    "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
                )
                .ok_or_else(|| VmError::internal("Locale lacks its constructor"))?;
            let language_ref = vm.make_string(&language)?;
            let country_ref = vm.make_string(&country)?;
            let variant_ref = vm.make_string(&variant)?;
            vm.invoke_method(
                declaring,
                ctor,
                vec![
                    Value::Ref(instance),
                    Value::Ref(language_ref),
                    Value::Ref(country_ref),
                    Value::Ref(variant_ref),
                ],
            )?;
            object(instance)
        },
    ),
    method(
        "toLanguageTag",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let language = locale_builder_part(vm, this, "language");
            let country = locale_builder_part(vm, this, "country");
            let variant = locale_builder_part(vm, this, "variant");
            let mut tag = language.clone();
            if !country.is_empty() {
                tag = alloc::format!("{tag}-{country}");
            }
            if !variant.is_empty() {
                tag = alloc::format!("{tag}-{variant}");
            }
            if tag.is_empty() {
                tag = String::from("und");
            }
            object(vm.make_string(&tag)?)
        },
    ),
];
