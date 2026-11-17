//! `java.util.Locale.LanguageRange`, `Locale.FilteringMode`, and the RFC 4647
//! filtering/lookup matching from `sun.util.locale.LocaleMatcher`.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{
    boolean, class, double, double_arg, field, int, method, object, receiver, ref_arg, string_arg,
    void,
};

// -------------------------------------------------------------------------------------------
// LanguageRange
// -------------------------------------------------------------------------------------------

/// `java.util.Locale.LanguageRange`: an RFC 4647 language range with a quality weight.
pub(crate) const LANGUAGE_RANGE: oxjvm_vm::NativeClass = class(
    "java/util/Locale$LanguageRange",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("range", "Ljava/lang/String;", ACC_PRIVATE | ACC_FINAL, None),
        field("weight", "D", ACC_PRIVATE | ACC_FINAL, None),
        field("MAX_WEIGHT", "D", ACC_PUBLIC | ACC_STATIC | ACC_FINAL, None),
        field("MIN_WEIGHT", "D", ACC_PUBLIC | ACC_STATIC | ACC_FINAL, None),
    ],
    &LANGUAGE_RANGE_METHODS,
    Some(language_range_clinit),
);

fn language_range_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/util/Locale$LanguageRange")?;
    vm.set_static_value(class, "MAX_WEIGHT", "D", Value::Double(1.0));
    vm.set_static_value(class, "MIN_WEIGHT", "D", Value::Double(0.0));
    Ok(())
}

const LANGUAGE_RANGE_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let range = string_arg(vm, args[1])?;
            language_range_init(vm, receiver(args), &range, 1.0)
        },
    ),
    method(
        "<init>",
        "(Ljava/lang/String;D)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let range = string_arg(vm, args[1])?;
            language_range_init(vm, receiver(args), &range, double_arg(args, 2))
        },
    ),
    method(
        "parse",
        "(Ljava/lang/String;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let ranges = string_arg(vm, args[0])?;
            let list = parse_ranges(vm, &ranges)?;
            object(crate::util::array_list_from(vm, &list)?)
        },
    ),
    method(
        "parse",
        "(Ljava/lang/String;Ljava/util/Map;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let ranges = string_arg(vm, args[0])?;
            let list = parse_ranges(vm, &ranges)?;
            let mapped = map_equivalents(vm, &list, ref_arg(args, 1))?;
            object(crate::util::array_list_from(vm, &mapped)?)
        },
    ),
    method(
        "mapEquivalents",
        "(Ljava/util/List;Ljava/util/Map;)Ljava/util/List;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let list = crate::util::collection_elements(vm, ref_arg(args, 0))?;
            let mapped = map_equivalents(vm, &list, ref_arg(args, 1))?;
            object(crate::util::array_list_from(vm, &mapped)?)
        },
    ),
    method(
        "getRange",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(vm.make_string(&lr_range(vm, receiver(args)))?),
    ),
    method("getWeight", "()D", ACC_PUBLIC, |vm, _, args| {
        double(lr_weight(vm, receiver(args)))
    }),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let other = ref_arg(args, 1);
            if this == other {
                return boolean(true);
            }
            let lr_class = vm.resolve_class("java/util/Locale$LanguageRange")?;
            if other.is_null() || !vm.is_instance(other, lr_class) {
                return boolean(false);
            }
            boolean(
                lr_range(vm, this) == lr_range(vm, other)
                    && lr_weight(vm, this) == lr_weight(vm, other),
            )
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let range = lr_range(vm, this);
        let weight = lr_weight(vm, this);
        let mut h = 17i32;
        h = h.wrapping_mul(37).wrapping_add(java_string_hash(&range));
        h = h.wrapping_mul(37).wrapping_add(java_double_hash(weight));
        int(h)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let range = lr_range(vm, this);
            let weight = lr_weight(vm, this);
            let text = if weight == 1.0 {
                range
            } else {
                alloc::format!("{range};q={}", java_double_string(weight))
            };
            object(vm.make_string(&text)?)
        },
    ),
];

fn java_string_hash(text: &str) -> i32 {
    let mut hash = 0i32;
    for unit in text.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
    }
    hash
}

fn java_double_hash(value: f64) -> i32 {
    let bits = value.to_bits();
    (bits ^ (bits >> 32)) as i32
}

/// `Double.toString` for the quality weights in `[0, 1]`: like Rust's display but
/// always keeps a fractional digit (`0.0`, `1.0`), as the JDK does.
fn java_double_string(value: f64) -> String {
    let text = alloc::format!("{value}");
    if text.contains('.') || text.contains('e') || text.contains('E') {
        text
    } else {
        alloc::format!("{text}.0")
    }
}

fn lr_range(vm: &Vm<'_>, this: ObjectRef) -> String {
    vm.read_string_field(this, vm.class_of(this), "range")
        .unwrap_or_default()
}

fn lr_weight(vm: &Vm<'_>, this: ObjectRef) -> f64 {
    vm.read_instance_double_named(this, vm.class_of(this), "weight", "D")
        .unwrap_or(1.0)
}

/// Construct and validate a `LanguageRange`, mirroring the JDK constructor.
fn language_range_init(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    range: &str,
    weight: f64,
) -> Result<Value, VmError> {
    if !(0.0..=1.0).contains(&weight) {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!("weight={weight}")),
        ));
    }
    let lower = range.to_lowercase();
    if ill_formed_range(&lower) {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!("range={lower}")),
        ));
    }
    let class = vm.class_of(this);
    let text = vm.make_string(&lower)?;
    if let Some((declaring, field)) = vm.find_field_opt(class, "range", "Ljava/lang/String;") {
        vm.set_instance_ref(this, declaring, field, text);
    }
    if let Some((declaring, field)) = vm.find_field_opt(class, "weight", "D") {
        vm.set_instance_double(this, declaring, field, weight);
    }
    void()
}

fn ill_formed_range(range: &str) -> bool {
    if range.ends_with('-') {
        return true;
    }
    let subtags: Vec<&str> = range.split('-').collect();
    if subtag_ill_formed(subtags[0], true) {
        return true;
    }
    subtags[1..].iter().any(|s| subtag_ill_formed(s, false))
}

fn subtag_ill_formed(subtag: &str, first: bool) -> bool {
    if subtag.is_empty() || subtag.len() > 8 {
        return true;
    }
    if subtag == "*" {
        return false;
    }
    subtag.chars().any(|c| {
        if first {
            !c.is_ascii_lowercase()
        } else {
            !c.is_ascii_lowercase() && !c.is_ascii_digit()
        }
    })
}

/// Build a `LanguageRange` object from an already-lowercased, validated range.
fn new_language_range(vm: &mut Vm<'_>, range: &str, weight: f64) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/Locale$LanguageRange")?;
    let instance = vm.new_instance(class)?;
    language_range_init(vm, instance, range, weight)?;
    Ok(instance)
}

// -------------------------------------------------------------------------------------------
// FilteringMode
// -------------------------------------------------------------------------------------------

const FILTERING_MODE_NAMES: [&str; 5] = [
    "AUTOSELECT_FILTERING",
    "EXTENDED_FILTERING",
    "IGNORE_EXTENDED_RANGES",
    "MAP_EXTENDED_RANGES",
    "REJECT_EXTENDED_RANGES",
];

/// `java.util.Locale.FilteringMode`: the RFC 4647 filtering mode selector.
pub(crate) const FILTERING_MODE: oxjvm_vm::NativeClass = class(
    "java/util/Locale$FilteringMode",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "AUTOSELECT_FILTERING",
            "Ljava/util/Locale$FilteringMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "EXTENDED_FILTERING",
            "Ljava/util/Locale$FilteringMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "IGNORE_EXTENDED_RANGES",
            "Ljava/util/Locale$FilteringMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "MAP_EXTENDED_RANGES",
            "Ljava/util/Locale$FilteringMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "REJECT_EXTENDED_RANGES",
            "Ljava/util/Locale$FilteringMode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
    ],
    &FILTERING_MODE_METHODS,
    Some(filtering_mode_clinit),
);

fn filtering_mode_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/util/Locale$FilteringMode")?;
    for (ordinal, name) in FILTERING_MODE_NAMES.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        if let Some((declaring, field)) = vm.find_field_opt(class, "ordinal", "I") {
            vm.set_instance_int(instance, declaring, field, ordinal as i32);
        }
        vm.set_static_value(
            class,
            name,
            "Ljava/util/Locale$FilteringMode;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

const FILTERING_MODE_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "ordinal"))
    }),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let index = crate::lang::read_int_field(vm, receiver(args), "ordinal").clamp(0, 4) as usize;
        object(vm.make_string(FILTERING_MODE_NAMES[index])?)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let index =
                crate::lang::read_int_field(vm, receiver(args), "ordinal").clamp(0, 4) as usize;
            object(vm.make_string(FILTERING_MODE_NAMES[index])?)
        },
    ),
    method(
        "values",
        "()[Ljava/util/Locale$FilteringMode;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/Locale$FilteringMode")?;
            vm.ensure_initialized(class)?;
            let array = vm.allocate_object_array(class, FILTERING_MODE_NAMES.len())?;
            for (index, name) in FILTERING_MODE_NAMES.iter().enumerate() {
                if let Some(value) =
                    vm.read_static_ref_if_set(class, name, "Ljava/util/Locale$FilteringMode;")
                {
                    vm.array_set_ref(array, index, value)?;
                }
            }
            object(array)
        },
    ),
];

// -------------------------------------------------------------------------------------------
// Locale.filter / filterTags / lookup / lookupTag (added to LOCALE_METHODS)
// -------------------------------------------------------------------------------------------

pub(crate) fn locale_filter(
    vm: &mut Vm<'_>,
    args: &[Value],
    with_mode: bool,
) -> Result<Value, VmError> {
    let priority = crate::util::collection_elements(vm, ref_arg(args, 0))?;
    let locales = crate::util::collection_elements(vm, ref_arg(args, 1))?;
    let mode = if with_mode {
        filtering_mode_ordinal(vm, ref_arg(args, 2))?
    } else {
        0
    };
    if priority.is_empty() || locales.is_empty() {
        return object(crate::util::array_list_from(vm, &[])?);
    }
    let tags = locale_tags(vm, &locales)?;
    let filtered = filter_tags(vm, &priority, &tags, mode)?;
    let mut result: Vec<ObjectRef> = Vec::new();
    for tag in filtered {
        result.push(locale_for_tag(vm, &tag)?);
    }
    object(crate::util::array_list_from(vm, &result)?)
}

pub(crate) fn locale_filter_tags(
    vm: &mut Vm<'_>,
    args: &[Value],
    with_mode: bool,
) -> Result<Value, VmError> {
    let priority = crate::util::collection_elements(vm, ref_arg(args, 0))?;
    let tag_refs = crate::util::collection_elements(vm, ref_arg(args, 1))?;
    let mode = if with_mode {
        filtering_mode_ordinal(vm, ref_arg(args, 2))?
    } else {
        0
    };
    let mut tags: Vec<String> = Vec::new();
    for reference in tag_refs {
        tags.push(string_arg(vm, Value::Ref(reference))?);
    }
    if priority.is_empty() || tags.is_empty() {
        return object(crate::util::array_list_from(vm, &[])?);
    }
    let filtered = filter_tags(vm, &priority, &tags, mode)?;
    let mut result: Vec<ObjectRef> = Vec::new();
    for tag in filtered {
        result.push(vm.make_string(&tag)?);
    }
    object(crate::util::array_list_from(vm, &result)?)
}

pub(crate) fn locale_lookup(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let priority = crate::util::collection_elements(vm, ref_arg(args, 0))?;
    let locales = crate::util::collection_elements(vm, ref_arg(args, 1))?;
    if priority.is_empty() || locales.is_empty() {
        return object(ObjectRef::NULL);
    }
    let tags = locale_tags(vm, &locales)?;
    match lookup_tag(vm, &priority, &tags) {
        Some(tag) => object(locale_for_tag(vm, &tag)?),
        None => object(ObjectRef::NULL),
    }
}

pub(crate) fn locale_lookup_tag(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let priority = crate::util::collection_elements(vm, ref_arg(args, 0))?;
    let tag_refs = crate::util::collection_elements(vm, ref_arg(args, 1))?;
    let mut tags: Vec<String> = Vec::new();
    for reference in tag_refs {
        tags.push(string_arg(vm, Value::Ref(reference))?);
    }
    if priority.is_empty() || tags.is_empty() {
        return object(ObjectRef::NULL);
    }
    match lookup_tag(vm, &priority, &tags) {
        Some(tag) => object(vm.make_string(&tag)?),
        None => object(ObjectRef::NULL),
    }
}

fn filtering_mode_ordinal(vm: &mut Vm<'_>, mode: ObjectRef) -> Result<i32, VmError> {
    if mode.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    Ok(crate::lang::read_int_field(vm, mode, "ordinal"))
}

fn locale_tags(vm: &mut Vm<'_>, locales: &[ObjectRef]) -> Result<Vec<String>, VmError> {
    let mut tags = Vec::new();
    for locale in locales {
        let (class, method) =
            vm.resolve_virtual_method(*locale, "toLanguageTag", "()Ljava/lang/String;")?;
        let tag = vm
            .invoke_method(class, method, vec![Value::Ref(*locale)])?
            .as_ref();
        tags.push(vm.string_value(tag).unwrap_or_default());
    }
    Ok(tags)
}

fn locale_for_tag(vm: &mut Vm<'_>, tag: &str) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("java/util/Locale")?;
    let (declaring, method) = vm
        .find_method(
            class,
            "forLanguageTag",
            "(Ljava/lang/String;)Ljava/util/Locale;",
        )
        .ok_or_else(|| VmError::internal("Locale.forLanguageTag"))?;
    let text = vm.make_string(tag)?;
    Ok(vm
        .invoke_method(declaring, method, vec![Value::Ref(text)])?
        .as_ref())
}

// -------------------------------------------------------------------------------------------
// The matching algorithms (sun.util.locale.LocaleMatcher)
// -------------------------------------------------------------------------------------------

fn split_ranges(vm: &Vm<'_>, priority: &[ObjectRef]) -> Option<usize> {
    priority.iter().position(|lr| lr_weight(vm, *lr) == 0.0)
}

fn filter_tags(
    vm: &mut Vm<'_>,
    priority: &[ObjectRef],
    tags: &[String],
    mode: i32,
) -> Result<Vec<String>, VmError> {
    if mode == 1 {
        return Ok(filter_extended(vm, priority, tags));
    }
    let mut list: Vec<ObjectRef> = Vec::new();
    for lr in priority {
        let range = lr_range(vm, *lr);
        let weight = lr_weight(vm, *lr);
        if range.starts_with("*-") || range.contains("-*") {
            match mode {
                0 => return Ok(filter_extended(vm, priority, tags)),
                3 => {
                    let mapped = if range.starts_with('*') {
                        String::from("*")
                    } else {
                        range.replace("-*", "")
                    };
                    list.push(new_language_range(vm, &mapped, weight)?);
                }
                4 => {
                    return Err(vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some(&alloc::format!(
                            "An extended range \"{range}\" found in REJECT_EXTENDED_RANGES mode."
                        )),
                    ));
                }
                _ => list.push(*lr),
            }
        } else {
            list.push(*lr);
        }
    }
    Ok(filter_basic(vm, &list, tags))
}

fn filter_basic(vm: &Vm<'_>, priority: &[ObjectRef], tags: &[String]) -> Vec<String> {
    let split = split_ranges(vm, priority);
    let (non_zero, zero) = match split {
        Some(index) => (&priority[..index], &priority[index..]),
        None => (priority, &[][..]),
    };
    let mut list: Vec<String> = Vec::new();
    for lr in non_zero {
        let range = lr_range(vm, *lr);
        if range == "*" {
            for tag in tags {
                let lower = tag.to_lowercase();
                if !case_insensitive_match(&list, &lower)
                    && !should_ignore_filter_basic(vm, zero, &lower)
                {
                    list.push(tag.clone());
                }
            }
            break;
        }
        for tag in tags {
            let lower = tag.to_lowercase();
            if lower.starts_with(&range) {
                let len = range.len();
                if (lower.len() == len || lower.as_bytes()[len] == b'-')
                    && !case_insensitive_match(&list, &lower)
                    && !should_ignore_filter_basic(vm, zero, &lower)
                {
                    list.push(tag.clone());
                }
            }
        }
    }
    list
}

fn case_insensitive_match(list: &[String], tag: &str) -> bool {
    list.iter().any(|element| element.eq_ignore_ascii_case(tag))
}

fn should_ignore_filter_basic(vm: &Vm<'_>, zero: &[ObjectRef], tag: &str) -> bool {
    for lr in zero {
        let range = lr_range(vm, *lr);
        if range == "*" {
            return true;
        }
        if tag.starts_with(&range) {
            let len = range.len();
            if tag.len() == len || tag.as_bytes()[len] == b'-' {
                return true;
            }
        }
    }
    false
}

fn filter_extended(vm: &Vm<'_>, priority: &[ObjectRef], tags: &[String]) -> Vec<String> {
    let split = split_ranges(vm, priority);
    let (non_zero, zero) = match split {
        Some(index) => (&priority[..index], &priority[index..]),
        None => (priority, &[][..]),
    };
    let mut list: Vec<String> = Vec::new();
    for lr in non_zero {
        let range = lr_range(vm, *lr);
        if range == "*" {
            for tag in tags {
                let lower = tag.to_lowercase();
                if !case_insensitive_match(&list, &lower)
                    && !should_ignore_filter_extended(vm, zero, &lower)
                {
                    list.push(tag.clone());
                }
            }
            break;
        }
        let range_subtags: Vec<&str> = range.split('-').collect();
        for tag in tags {
            let lower = tag.to_lowercase();
            let tag_subtags: Vec<&str> = lower.split('-').collect();
            if range_subtags[0] != tag_subtags[0] && range_subtags[0] != "*" {
                continue;
            }
            let range_index = match_filter_extended_subtags(&range_subtags, &tag_subtags);
            if range_subtags.len() == range_index
                && !case_insensitive_match(&list, &lower)
                && !should_ignore_filter_extended(vm, zero, &lower)
            {
                list.push(tag.clone());
            }
        }
    }
    list
}

fn should_ignore_filter_extended(vm: &Vm<'_>, zero: &[ObjectRef], tag: &str) -> bool {
    let tag_subtags: Vec<&str> = tag.split('-').collect();
    for lr in zero {
        let range = lr_range(vm, *lr);
        if range == "*" {
            return true;
        }
        let range_subtags: Vec<&str> = range.split('-').collect();
        if range_subtags[0] != tag_subtags[0] && range_subtags[0] != "*" {
            continue;
        }
        if range_subtags.len() == match_filter_extended_subtags(&range_subtags, &tag_subtags) {
            return true;
        }
    }
    false
}

fn match_filter_extended_subtags(range: &[&str], tag: &[&str]) -> usize {
    let mut range_index = 1;
    let mut tag_index = 1;
    while range_index < range.len() && tag_index < tag.len() {
        if range[range_index] == "*" {
            range_index += 1;
        } else if range[range_index] == tag[tag_index] {
            range_index += 1;
            tag_index += 1;
        } else if tag[tag_index].len() == 1 && tag[tag_index] != "*" {
            break;
        } else {
            tag_index += 1;
        }
    }
    range_index
}

fn lookup_tag(vm: &Vm<'_>, priority: &[ObjectRef], tags: &[String]) -> Option<String> {
    let split = split_ranges(vm, priority);
    let (non_zero, zero) = match split {
        Some(index) => (&priority[..index], &priority[index..]),
        None => (priority, &[][..]),
    };
    for lr in non_zero {
        let range = lr_range(vm, *lr);
        if range == "*" {
            continue;
        }
        let mut pattern = range_regex(&range);
        while !pattern.is_empty() {
            for tag in tags {
                let lower = tag.to_lowercase();
                if regex_matches(&pattern, &lower) && !should_ignore_lookup(vm, zero, &lower) {
                    return Some(tag.clone());
                }
            }
            pattern = truncate_range(&pattern);
        }
    }
    None
}

fn should_ignore_lookup(vm: &Vm<'_>, zero: &[ObjectRef], tag: &str) -> bool {
    for lr in zero {
        let range = lr_range(vm, *lr);
        if range == "*" {
            continue;
        }
        let mut pattern = range_regex(&range);
        while !pattern.is_empty() {
            if regex_matches(&pattern, tag) {
                return true;
            }
            pattern = truncate_range(&pattern);
        }
    }
    false
}

/// The JDK replaces `*` with the regex `\p{Alnum}*`; keep that literal form so
/// `truncate_range` sees the same string the reference implementation does.
fn range_regex(range: &str) -> String {
    range.replace('*', "\\p{Alnum}*")
}

fn truncate_range(pattern: &str) -> String {
    match pattern.rfind('-') {
        Some(index) => {
            let mut truncated = pattern[..index].to_string();
            if let Some(last) = truncated.rfind('-') {
                if last == truncated.len() - 2 {
                    truncated = truncated[..truncated.len() - 2].to_string();
                }
            }
            truncated
        }
        None => String::new(),
    }
}

/// Full match of the lookup pattern (literal chars plus `\p{Alnum}*` wildcards).
fn regex_matches(pattern: &str, tag: &str) -> bool {
    let mut tokens: Vec<Option<char>> = Vec::new(); // None = wildcard
    let chars: Vec<char> = pattern.chars().collect();
    let wildcard: Vec<char> = "\\p{Alnum}*".chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && chars[i..].starts_with(&wildcard) {
            tokens.push(None);
            i += wildcard.len();
        } else {
            tokens.push(Some(chars[i]));
            i += 1;
        }
    }
    let tag_chars: Vec<char> = tag.chars().collect();
    fn go(tokens: &[Option<char>], p: usize, tag: &[char], t: usize) -> bool {
        if p == tokens.len() {
            return t == tag.len();
        }
        match tokens[p] {
            None => {
                let mut k = t;
                loop {
                    if go(tokens, p + 1, tag, k) {
                        return true;
                    }
                    if k < tag.len() && tag[k].is_ascii_alphanumeric() {
                        k += 1;
                    } else {
                        return false;
                    }
                }
            }
            Some(c) => t < tag.len() && tag[t] == c && go(tokens, p + 1, tag, t + 1),
        }
    }
    go(&tokens, 0, &tag_chars, 0)
}

// -------------------------------------------------------------------------------------------
// LanguageRange.parse and mapEquivalents
// -------------------------------------------------------------------------------------------

fn parse_ranges(vm: &mut Vm<'_>, ranges: &str) -> Result<Vec<ObjectRef>, VmError> {
    let mut text = ranges.replace(' ', "").to_lowercase();
    if let Some(stripped) = text.strip_prefix("accept-language:") {
        text = stripped.to_string();
    }
    let mut list: Vec<ObjectRef> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for range in text.split(',') {
        let (r, w) = match range.find(";q=") {
            None => (range.to_string(), 1.0),
            Some(index) => {
                let r = range[..index].to_string();
                let weight_text = &range[index + 3..];
                let w: f64 = weight_text.parse().map_err(|_| {
                    vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some(&alloc::format!(
                            "weight=\"{weight_text}\" for language range \"{r}\""
                        )),
                    )
                })?;
                if !(0.0..=1.0).contains(&w) {
                    return Err(vm.throw_new(
                        "java/lang/IllegalArgumentException",
                        Some(&alloc::format!(
                            "weight={w} for language range \"{r}\". It must be between 0.0 and 1.0."
                        )),
                    ));
                }
                (r, w)
            }
        };
        if seen.contains(&r) {
            continue;
        }
        let lr = new_language_range(vm, &r, w)?;
        let mut index = list.len();
        for (j, existing) in list.iter().enumerate() {
            if lr_weight(vm, *existing) < w {
                index = j;
                break;
            }
        }
        list.insert(index, lr);
        seen.push(r.clone());

        if let Some(equivalent) = equivalent_region_variant(&r) {
            if !seen.contains(&equivalent) {
                list.insert(index + 1, new_language_range(vm, &equivalent, w)?);
                seen.push(equivalent);
            }
        }
        if let Some(equivalents) = equivalents_language(&r) {
            for equiv in equivalents {
                if !seen.contains(&equiv) {
                    list.insert(index + 1, new_language_range(vm, &equiv, w)?);
                    seen.push(equiv.clone());
                }
                if let Some(equivalent) = equivalent_region_variant(&equiv) {
                    if !seen.contains(&equivalent) {
                        list.insert(index + 1, new_language_range(vm, &equivalent, w)?);
                        seen.push(equivalent);
                    }
                }
            }
        }
    }
    Ok(list)
}

fn replace_first(range: &str, substr: &str, replacement: &str) -> String {
    match range.find(substr) {
        Some(pos) => alloc::format!(
            "{}{replacement}{}",
            &range[..pos],
            &range[pos + substr.len()..]
        ),
        None => range.to_string(),
    }
}

fn equivalents_language(range: &str) -> Option<Vec<String>> {
    let mut r = range.to_string();
    while !r.is_empty() {
        if let Some((_, equiv)) = SINGLE_EQUIV_MAP.iter().find(|(k, _)| *k == r) {
            return Some(vec![replace_first(range, &r, equiv)]);
        }
        if let Some((_, equivs)) = MULTI_EQUIVS_MAP.iter().find(|(k, _)| *k == r) {
            return Some(equivs.iter().map(|e| replace_first(range, &r, e)).collect());
        }
        match r.rfind('-') {
            Some(index) => r = r[..index].to_string(),
            None => break,
        }
    }
    None
}

fn equivalent_region_variant(range: &str) -> Option<String> {
    let extension_key_index = extension_key_index(range);
    for (subtag, replacement) in REGION_VARIANT_EQUIV_MAP.iter() {
        if let Some(index) = range.find(subtag) {
            if let Some(ext) = extension_key_index {
                if index > ext {
                    continue;
                }
            }
            let len = index + subtag.len();
            if range.len() == len || range.as_bytes()[len] == b'-' {
                return Some(replace_first(range, subtag, replacement));
            }
        }
    }
    None
}

fn extension_key_index(s: &str) -> Option<usize> {
    let mut index: Option<usize> = None;
    for (i, c) in s.chars().enumerate().skip(1) {
        if c == '-' {
            if let Some(prev) = index {
                if i - prev == 2 {
                    return Some(prev);
                }
            }
            index = Some(i);
        }
    }
    None
}

fn map_equivalents(
    vm: &mut Vm<'_>,
    priority: &[ObjectRef],
    map: ObjectRef,
) -> Result<Vec<ObjectRef>, VmError> {
    if priority.is_empty() {
        return Ok(Vec::new());
    }
    let pairs = if map.is_null() {
        Vec::new()
    } else {
        crate::util::hash_map_pairs(vm, map)
    };
    if pairs.is_empty() {
        return Ok(priority.to_vec());
    }
    // key (lowercased) -> list of equivalent strings
    let mut key_map: Vec<(String, Vec<String>)> = Vec::new();
    for (key, value) in pairs {
        let key_text = vm.string_value(key).unwrap_or_default().to_lowercase();
        let equivalents = crate::util::collection_elements(vm, value)?;
        let mut texts = Vec::new();
        for equivalent in equivalents {
            texts.push(string_arg(vm, Value::Ref(equivalent))?);
        }
        key_map.push((key_text, texts));
    }
    let mut list: Vec<ObjectRef> = Vec::new();
    for lr in priority {
        let range = lr_range(vm, *lr);
        let weight = lr_weight(vm, *lr);
        let mut r = range.clone();
        let mut has_equivalent = false;
        while !r.is_empty() {
            if let Some((_, equivalents)) = key_map.iter().find(|(k, _)| *k == r) {
                has_equivalent = true;
                let len = r.len();
                for equivalent in equivalents {
                    let mapped = alloc::format!("{}{}", equivalent.to_lowercase(), &range[len..]);
                    list.push(new_language_range(vm, &mapped, weight)?);
                }
                break;
            }
            match r.rfind('-') {
                Some(index) => r = r[..index].to_string(),
                None => break,
            }
        }
        if !has_equivalent {
            list.push(*lr);
        }
    }
    Ok(list)
}
// -------------------------------------------------------------------------------------------
// IANA Language Subtag Registry equivalent maps (sun.util.locale.LocaleEquivalentMaps)
// -------------------------------------------------------------------------------------------

/// IANA Language Subtag Registry equivalents (single mapping), LSR 2026-05-05.
static SINGLE_EQUIV_MAP: [(&str, &str); 658] = [
    ("aam", "aas"),
    ("aao", "ar-aao"),
    ("aas", "aam"),
    ("abh", "ar-abh"),
    ("abv", "ar-abv"),
    ("acm", "ar-acm"),
    ("acn", "xia"),
    ("acq", "ar-acq"),
    ("acw", "ar-acw"),
    ("acx", "ar-acx"),
    ("acy", "ar-acy"),
    ("adf", "ar-adf"),
    ("adp", "dz"),
    ("ads", "sgn-ads"),
    ("adx", "pcr"),
    ("aec", "ar-aec"),
    ("aed", "sgn-aed"),
    ("aen", "sgn-aen"),
    ("afb", "ar-afb"),
    ("afg", "sgn-afg"),
    ("ajs", "sgn-ajs"),
    ("ami", "i-ami"),
    ("aog", "myd"),
    ("apd", "ar-apd"),
    ("ar-aao", "aao"),
    ("ar-abh", "abh"),
    ("ar-abv", "abv"),
    ("ar-acm", "acm"),
    ("ar-acq", "acq"),
    ("ar-acw", "acw"),
    ("ar-acx", "acx"),
    ("ar-acy", "acy"),
    ("ar-adf", "adf"),
    ("ar-aec", "aec"),
    ("ar-afb", "afb"),
    ("ar-apd", "apd"),
    ("ar-arb", "arb"),
    ("ar-arq", "arq"),
    ("ar-ars", "ars"),
    ("ar-ary", "ary"),
    ("ar-arz", "arz"),
    ("ar-auz", "auz"),
    ("ar-avl", "avl"),
    ("ar-ayh", "ayh"),
    ("ar-ayl", "ayl"),
    ("ar-ayn", "ayn"),
    ("ar-ayp", "ayp"),
    ("ar-bbz", "bbz"),
    ("ar-pga", "pga"),
    ("ar-shu", "shu"),
    ("ar-ssh", "ssh"),
    ("arb", "ar-arb"),
    ("arq", "ar-arq"),
    ("ars", "ar-ars"),
    ("art-lojban", "jbo"),
    ("ary", "ar-ary"),
    ("arz", "ar-arz"),
    ("asd", "snz"),
    ("asf", "sgn-asf"),
    ("asp", "sgn-asp"),
    ("asq", "sgn-asq"),
    ("asw", "sgn-asw"),
    ("aue", "ktz"),
    ("auz", "ar-auz"),
    ("avl", "ar-avl"),
    ("ayh", "ar-ayh"),
    ("ayl", "ar-ayl"),
    ("ayn", "ar-ayn"),
    ("ayp", "ar-ayp"),
    ("ayx", "nun"),
    ("bbz", "ar-bbz"),
    ("bcg", "bgm"),
    ("bfk", "sgn-bfk"),
    ("bfy", "ppa"),
    ("bgm", "bcg"),
    ("bic", "bir"),
    ("bir", "bic"),
    ("bjd", "drl"),
    ("bjn", "ms-bjn"),
    ("bmf", "krm"),
    ("bnn", "i-bnn"),
    ("bog", "sgn-bog"),
    ("bpp", "nxu"),
    ("bqn", "sgn-bqn"),
    ("bqy", "sgn-bqy"),
    ("btj", "ms-btj"),
    ("bve", "ms-bve"),
    ("bvl", "sgn-bvl"),
    ("bvu", "ms-bvu"),
    ("cax", "xba"),
    ("cbr", "nom"),
    ("cdo", "zh-cdo"),
    ("cds", "sgn-cds"),
    ("cir", "meg"),
    ("cjr", "mom"),
    ("cjy", "zh-cjy"),
    ("cka", "cmr"),
    ("cmk", "xch"),
    ("cmn-hans", "zh-cmn-hans"),
    ("cmn-hant", "zh-cmn-hant"),
    ("cmr", "cka"),
    ("cnp", "zh-cnp"),
    ("coa", "ms-coa"),
    ("cpx", "zh-cpx"),
    ("cqu", "quh"),
    ("crr", "pmk"),
    ("csc", "sgn-csc"),
    ("csd", "sgn-csd"),
    ("cse", "sgn-cse"),
    ("csf", "sgn-csf"),
    ("csg", "sgn-csg"),
    ("csl", "sgn-csl"),
    ("csp", "zh-csp"),
    ("csq", "sgn-csq"),
    ("csr", "sgn-csr"),
    ("csx", "sgn-csx"),
    ("czh", "zh-czh"),
    ("czo", "zh-czo"),
    ("dek", "sqm"),
    ("dev", "gav"),
    ("dif", "dit"),
    ("dit", "dif"),
    ("dmw", "xrq"),
    ("doq", "sgn-doq"),
    ("drh", "khk"),
    ("drl", "bjd"),
    ("dsz", "sgn-dsz"),
    ("dup", "ms-dup"),
    ("duz", "guv"),
    ("dyl", "sgn-dyl"),
    ("dz", "adp"),
    ("ecs", "sgn-ecs"),
    ("ehs", "sgn-ehs"),
    ("eko", "nte"),
    ("ema", "uok"),
    ("en-gb-oed", "en-gb-oxendict"),
    ("en-gb-oxendict", "en-gb-oed"),
    ("enm", "yol"),
    ("esl", "sgn-esl"),
    ("esn", "sgn-esn"),
    ("eso", "sgn-eso"),
    ("eth", "sgn-eth"),
    ("fcs", "sgn-fcs"),
    ("fse", "sgn-fse"),
    ("fss", "sgn-fss"),
    ("gal", "ilw"),
    ("gan", "zh-gan"),
    ("gav", "dev"),
    ("gdj", "kvs"),
    ("gds", "sgn-gds"),
    ("ggn", "gvr"),
    ("gom", "kok-gom"),
    ("gse", "sgn-gse"),
    ("gsm", "sgn-gsm"),
    ("gti", "nyc"),
    ("gu", "prp"),
    ("gus", "sgn-gus"),
    ("guv", "duz"),
    ("gvr", "ggn"),
    ("hab", "sgn-hab"),
    ("haf", "sgn-haf"),
    ("hds", "sgn-hds"),
    ("he", "iw"),
    ("hji", "ms-hji"),
    ("hks", "sgn-hks"),
    ("hle", "sca"),
    ("hnm", "zh-hnm"),
    ("hos", "sgn-hos"),
    ("hps", "sgn-hps"),
    ("hrr", "jal"),
    ("hsh", "sgn-hsh"),
    ("hsl", "sgn-hsl"),
    ("huw", "pmc"),
    ("i-ami", "ami"),
    ("i-bnn", "bnn"),
    ("i-klingon", "tlh"),
    ("i-lux", "lb"),
    ("i-navajo", "nv"),
    ("i-pwn", "pwn"),
    ("i-tao", "tao"),
    ("i-tay", "tay"),
    ("i-tsu", "tsu"),
    ("ibi", "opa"),
    ("icl", "sgn-icl"),
    ("id", "in"),
    ("iks", "sgn-iks"),
    ("ils", "sgn-ils"),
    ("ilw", "gal"),
    ("in", "id"),
    ("inl", "sgn-inl"),
    ("ins", "sgn-ins"),
    ("isr", "sgn-isr"),
    ("iw", "he"),
    ("jak", "ms-jak"),
    ("jal", "hrr"),
    ("jax", "ms-jax"),
    ("jbo", "art-lojban"),
    ("jcs", "sgn-jcs"),
    ("jhs", "sgn-jhs"),
    ("ji", "yi"),
    ("jks", "sgn-jks"),
    ("jls", "sgn-jls"),
    ("jos", "sgn-jos"),
    ("jus", "sgn-jus"),
    ("jv", "jw"),
    ("jw", "jv"),
    ("kak", "tne"),
    ("kdz", "ncp"),
    ("kgc", "tdf"),
    ("kgh", "kml"),
    ("kgi", "sgn-kgi"),
    ("kgm", "plu"),
    ("khk", "drh"),
    ("kjh", "zkb"),
    ("kmb", "smd"),
    ("kml", "kgh"),
    ("knn", "kok-knn"),
    ("koj", "kwv"),
    ("kok-gom", "gom"),
    ("kok-knn", "knn"),
    ("krm", "bmf"),
    ("kru", "kxl"),
    ("ksp", "lak"),
    ("ktz", "aue"),
    ("kvb", "ms-kvb"),
    ("kvk", "sgn-kvk"),
    ("kvr", "ms-kvr"),
    ("kvs", "gdj"),
    ("kwq", "yam"),
    ("kwv", "koj"),
    ("kxd", "ms-kxd"),
    ("kxe", "tvd"),
    ("kxl", "kru"),
    ("kxr", "pat"),
    ("lak", "ksp"),
    ("lb", "i-lux"),
    ("lbs", "sgn-lbs"),
    ("lce", "ms-lce"),
    ("lcf", "ms-lcf"),
    ("lcq", "ppr"),
    ("lgs", "sgn-lgs"),
    ("lii", "raq"),
    ("liw", "ms-liw"),
    ("llo", "ngt"),
    ("lls", "sgn-lls"),
    ("lmm", "rmx"),
    ("lrr", "yma"),
    ("lsb", "sgn-lsb"),
    ("lsc", "sgn-lsc"),
    ("lsg", "sgn-lsg"),
    ("lsl", "sgn-lsl"),
    ("lsn", "sgn-lsn"),
    ("lso", "sgn-lso"),
    ("lsp", "sgn-lsp"),
    ("lst", "sgn-lst"),
    ("lsv", "sgn-lsv"),
    ("lsw", "sgn-lsw"),
    ("lsy", "sgn-lsy"),
    ("ltg", "lv-ltg"),
    ("luh", "zh-luh"),
    ("lv-ltg", "ltg"),
    ("lv-lvs", "lvs"),
    ("lvs", "lv-lvs"),
    ("lws", "sgn-lws"),
    ("lzh", "zh-lzh"),
    ("max", "ms-max"),
    ("mdl", "sgn-mdl"),
    ("meg", "cir"),
    ("meo", "ms-meo"),
    ("mfa", "ms-mfa"),
    ("mfb", "ms-mfb"),
    ("min", "ms-min"),
    ("mnp", "zh-mnp"),
    ("mo", "ro"),
    ("mom", "cjr"),
    ("mqg", "ms-mqg"),
    ("mre", "sgn-mre"),
    ("ms-bjn", "bjn"),
    ("ms-btj", "btj"),
    ("ms-bve", "bve"),
    ("ms-bvu", "bvu"),
    ("ms-coa", "coa"),
    ("ms-dup", "dup"),
    ("ms-hji", "hji"),
    ("ms-jak", "jak"),
    ("ms-jax", "jax"),
    ("ms-kvb", "kvb"),
    ("ms-kvr", "kvr"),
    ("ms-kxd", "kxd"),
    ("ms-lce", "lce"),
    ("ms-lcf", "lcf"),
    ("ms-liw", "liw"),
    ("ms-max", "max"),
    ("ms-meo", "meo"),
    ("ms-mfa", "mfa"),
    ("ms-mfb", "mfb"),
    ("ms-min", "min"),
    ("ms-mqg", "mqg"),
    ("ms-msi", "msi"),
    ("ms-mui", "mui"),
    ("ms-orn", "orn"),
    ("ms-ors", "ors"),
    ("ms-pel", "pel"),
    ("ms-pse", "pse"),
    ("ms-tmw", "tmw"),
    ("ms-urk", "urk"),
    ("ms-vkk", "vkk"),
    ("ms-vkt", "vkt"),
    ("ms-xmm", "xmm"),
    ("ms-zlm", "zlm"),
    ("ms-zmi", "zmi"),
    ("ms-zsm", "zsm"),
    ("msd", "sgn-msd"),
    ("msi", "ms-msi"),
    ("msr", "sgn-msr"),
    ("mtm", "ymt"),
    ("mui", "ms-mui"),
    ("myd", "aog"),
    ("mzc", "sgn-mzc"),
    ("mzg", "sgn-mzg"),
    ("mzy", "sgn-mzy"),
    ("nad", "xny"),
    ("nb", "no-bok"),
    ("nbr", "nns"),
    ("nbs", "sgn-nbs"),
    ("ncp", "kdz"),
    ("ngt", "llo"),
    ("ngv", "nnx"),
    ("nn", "no-nyn"),
    ("nns", "nbr"),
    ("nnx", "ngv"),
    ("no-bok", "nb"),
    ("no-nyn", "nn"),
    ("nom", "cbr"),
    ("nsi", "sgn-nsi"),
    ("nsp", "sgn-nsp"),
    ("nsr", "sgn-nsr"),
    ("nte", "eko"),
    ("nun", "ayx"),
    ("nv", "i-navajo"),
    ("nxu", "bpp"),
    ("nyc", "gti"),
    ("nzs", "sgn-nzs"),
    ("okl", "sgn-okl"),
    ("ola", "thw"),
    ("opa", "ibi"),
    ("orn", "ms-orn"),
    ("ors", "ms-ors"),
    ("pat", "kxr"),
    ("pcr", "adx"),
    ("pel", "ms-pel"),
    ("pga", "ar-pga"),
    ("pgz", "sgn-pgz"),
    ("phr", "pmu"),
    ("pks", "sgn-pks"),
    ("plu", "kgm"),
    ("pmc", "huw"),
    ("pmk", "crr"),
    ("pmu", "phr"),
    ("ppa", "bfy"),
    ("ppr", "lcq"),
    ("prl", "sgn-prl"),
    ("prp", "gu"),
    ("prt", "pry"),
    ("pry", "prt"),
    ("prz", "sgn-prz"),
    ("psc", "sgn-psc"),
    ("psd", "sgn-psd"),
    ("pse", "ms-pse"),
    ("psg", "sgn-psg"),
    ("psl", "sgn-psl"),
    ("pso", "sgn-pso"),
    ("psp", "sgn-psp"),
    ("pub", "puz"),
    ("puz", "pub"),
    ("pwn", "i-pwn"),
    ("pys", "sgn-pys"),
    ("quh", "cqu"),
    ("raq", "lii"),
    ("ras", "tie"),
    ("rib", "sgn-rib"),
    ("rms", "sgn-rms"),
    ("rmx", "lmm"),
    ("rnb", "sgn-rnb"),
    ("ro", "mo"),
    ("rsi", "sgn-rsi"),
    ("rsl", "sgn-rsl"),
    ("rsm", "sgn-rsm"),
    ("rsn", "sgn-rsn"),
    ("sca", "hle"),
    ("scv", "zir"),
    ("sdl", "sgn-sdl"),
    ("sgn-ads", "ads"),
    ("sgn-aed", "aed"),
    ("sgn-aen", "aen"),
    ("sgn-afg", "afg"),
    ("sgn-ajs", "ajs"),
    ("sgn-asf", "asf"),
    ("sgn-asp", "asp"),
    ("sgn-asq", "asq"),
    ("sgn-asw", "asw"),
    ("sgn-bfk", "bfk"),
    ("sgn-bog", "bog"),
    ("sgn-bqn", "bqn"),
    ("sgn-bqy", "bqy"),
    ("sgn-bvl", "bvl"),
    ("sgn-cds", "cds"),
    ("sgn-csc", "csc"),
    ("sgn-csd", "csd"),
    ("sgn-cse", "cse"),
    ("sgn-csf", "csf"),
    ("sgn-csg", "csg"),
    ("sgn-csl", "csl"),
    ("sgn-csq", "csq"),
    ("sgn-csr", "csr"),
    ("sgn-csx", "csx"),
    ("sgn-doq", "doq"),
    ("sgn-dsz", "dsz"),
    ("sgn-dyl", "dyl"),
    ("sgn-ecs", "ecs"),
    ("sgn-ehs", "ehs"),
    ("sgn-esl", "esl"),
    ("sgn-esn", "esn"),
    ("sgn-eso", "eso"),
    ("sgn-eth", "eth"),
    ("sgn-fcs", "fcs"),
    ("sgn-fse", "fse"),
    ("sgn-fss", "fss"),
    ("sgn-gds", "gds"),
    ("sgn-gse", "gse"),
    ("sgn-gsm", "gsm"),
    ("sgn-gus", "gus"),
    ("sgn-hab", "hab"),
    ("sgn-haf", "haf"),
    ("sgn-hds", "hds"),
    ("sgn-hks", "hks"),
    ("sgn-hos", "hos"),
    ("sgn-hps", "hps"),
    ("sgn-hsh", "hsh"),
    ("sgn-hsl", "hsl"),
    ("sgn-icl", "icl"),
    ("sgn-iks", "iks"),
    ("sgn-ils", "ils"),
    ("sgn-inl", "inl"),
    ("sgn-ins", "ins"),
    ("sgn-isr", "isr"),
    ("sgn-jcs", "jcs"),
    ("sgn-jhs", "jhs"),
    ("sgn-jks", "jks"),
    ("sgn-jls", "jls"),
    ("sgn-jos", "jos"),
    ("sgn-jus", "jus"),
    ("sgn-kgi", "kgi"),
    ("sgn-kvk", "kvk"),
    ("sgn-lbs", "lbs"),
    ("sgn-lgs", "lgs"),
    ("sgn-lls", "lls"),
    ("sgn-lsb", "lsb"),
    ("sgn-lsc", "lsc"),
    ("sgn-lsg", "lsg"),
    ("sgn-lsl", "lsl"),
    ("sgn-lsn", "lsn"),
    ("sgn-lso", "lso"),
    ("sgn-lsp", "lsp"),
    ("sgn-lst", "lst"),
    ("sgn-lsv", "lsv"),
    ("sgn-lsw", "lsw"),
    ("sgn-lsy", "lsy"),
    ("sgn-lws", "lws"),
    ("sgn-mdl", "mdl"),
    ("sgn-mre", "mre"),
    ("sgn-msd", "msd"),
    ("sgn-msr", "msr"),
    ("sgn-mzc", "mzc"),
    ("sgn-mzg", "mzg"),
    ("sgn-mzy", "mzy"),
    ("sgn-nbs", "nbs"),
    ("sgn-nsi", "nsi"),
    ("sgn-nsp", "nsp"),
    ("sgn-nsr", "nsr"),
    ("sgn-nzs", "nzs"),
    ("sgn-okl", "okl"),
    ("sgn-pgz", "pgz"),
    ("sgn-pks", "pks"),
    ("sgn-prl", "prl"),
    ("sgn-prz", "prz"),
    ("sgn-psc", "psc"),
    ("sgn-psd", "psd"),
    ("sgn-psg", "psg"),
    ("sgn-psl", "psl"),
    ("sgn-pso", "pso"),
    ("sgn-psp", "psp"),
    ("sgn-pys", "pys"),
    ("sgn-rib", "rib"),
    ("sgn-rms", "rms"),
    ("sgn-rnb", "rnb"),
    ("sgn-rsi", "rsi"),
    ("sgn-rsl", "rsl"),
    ("sgn-rsm", "rsm"),
    ("sgn-rsn", "rsn"),
    ("sgn-sdl", "sdl"),
    ("sgn-sgx", "sgx"),
    ("sgn-slf", "slf"),
    ("sgn-sls", "sls"),
    ("sgn-sqk", "sqk"),
    ("sgn-sqs", "sqs"),
    ("sgn-sqx", "sqx"),
    ("sgn-ssr", "ssr"),
    ("sgn-svk", "svk"),
    ("sgn-syy", "syy"),
    ("sgn-szs", "szs"),
    ("sgn-tse", "tse"),
    ("sgn-tsm", "tsm"),
    ("sgn-tsq", "tsq"),
    ("sgn-tss", "tss"),
    ("sgn-tsy", "tsy"),
    ("sgn-tza", "tza"),
    ("sgn-ugn", "ugn"),
    ("sgn-ugy", "ugy"),
    ("sgn-ukl", "ukl"),
    ("sgn-uks", "uks"),
    ("sgn-vsi", "vsi"),
    ("sgn-vsl", "vsl"),
    ("sgn-vsv", "vsv"),
    ("sgn-wbs", "wbs"),
    ("sgn-xki", "xki"),
    ("sgn-xml", "xml"),
    ("sgn-xms", "xms"),
    ("sgn-yds", "yds"),
    ("sgn-ygs", "ygs"),
    ("sgn-yhs", "yhs"),
    ("sgn-ysl", "ysl"),
    ("sgn-ysm", "ysm"),
    ("sgn-zhk", "zhk"),
    ("sgn-zib", "zib"),
    ("sgn-zsl", "zsl"),
    ("sgx", "sgn-sgx"),
    ("shu", "ar-shu"),
    ("sjc", "zh-sjc"),
    ("slf", "sgn-slf"),
    ("sls", "sgn-sls"),
    ("smd", "kmb"),
    ("snz", "asd"),
    ("sqk", "sgn-sqk"),
    ("sqm", "dek"),
    ("sqs", "sgn-sqs"),
    ("sqx", "sgn-sqx"),
    ("ssh", "ar-ssh"),
    ("ssr", "sgn-ssr"),
    ("svk", "sgn-svk"),
    ("sw-swc", "swc"),
    ("sw-swh", "swh"),
    ("swc", "sw-swc"),
    ("swh", "sw-swh"),
    ("syy", "sgn-syy"),
    ("szd", "umi"),
    ("szs", "sgn-szs"),
    ("taj", "tsf"),
    ("tao", "i-tao"),
    ("tay", "i-tay"),
    ("tdf", "kgc"),
    ("tdg", "tmk"),
    ("thc", "tpo"),
    ("thw", "ola"),
    ("tie", "ras"),
    ("tkk", "twm"),
    ("tlh", "i-klingon"),
    ("tlw", "weo"),
    ("tmk", "tdg"),
    ("tmp", "tyj"),
    ("tmw", "ms-tmw"),
    ("tne", "kak"),
    ("tpn", "tpw"),
    ("tpo", "thc"),
    ("tpw", "tpn"),
    ("tse", "sgn-tse"),
    ("tsf", "taj"),
    ("tsm", "sgn-tsm"),
    ("tsq", "sgn-tsq"),
    ("tss", "sgn-tss"),
    ("tsu", "i-tsu"),
    ("tsy", "sgn-tsy"),
    ("tvd", "kxe"),
    ("twm", "tkk"),
    ("tyj", "tmp"),
    ("tza", "sgn-tza"),
    ("ugn", "sgn-ugn"),
    ("ugy", "sgn-ugy"),
    ("ukl", "sgn-ukl"),
    ("uks", "sgn-uks"),
    ("umi", "szd"),
    ("uok", "ema"),
    ("urk", "ms-urk"),
    ("uz-uzn", "uzn"),
    ("uz-uzs", "uzs"),
    ("uzn", "uz-uzn"),
    ("uzs", "uz-uzs"),
    ("vkk", "ms-vkk"),
    ("vkt", "ms-vkt"),
    ("vsi", "sgn-vsi"),
    ("vsl", "sgn-vsl"),
    ("vsv", "sgn-vsv"),
    ("waw", "xkh"),
    ("wbs", "sgn-wbs"),
    ("weo", "tlw"),
    ("wuu", "zh-wuu"),
    ("xba", "cax"),
    ("xch", "cmk"),
    ("xia", "acn"),
    ("xkh", "waw"),
    ("xki", "sgn-xki"),
    ("xml", "sgn-xml"),
    ("xmm", "ms-xmm"),
    ("xms", "sgn-xms"),
    ("xny", "nad"),
    ("xrq", "dmw"),
    ("xss", "zko"),
    ("yam", "kwq"),
    ("yds", "sgn-yds"),
    ("ygs", "sgn-ygs"),
    ("yhs", "sgn-yhs"),
    ("yi", "ji"),
    ("yma", "lrr"),
    ("ymt", "mtm"),
    ("yol", "enm"),
    ("yos", "zom"),
    ("ysl", "sgn-ysl"),
    ("ysm", "sgn-ysm"),
    ("yue", "zh-yue"),
    ("yug", "yuu"),
    ("yuu", "yug"),
    ("zh-cdo", "cdo"),
    ("zh-cjy", "cjy"),
    ("zh-cmn-hans", "cmn-hans"),
    ("zh-cmn-hant", "cmn-hant"),
    ("zh-cnp", "cnp"),
    ("zh-cpx", "cpx"),
    ("zh-csp", "csp"),
    ("zh-czh", "czh"),
    ("zh-czo", "czo"),
    ("zh-gan", "gan"),
    ("zh-hnm", "hnm"),
    ("zh-luh", "luh"),
    ("zh-lzh", "lzh"),
    ("zh-mnp", "mnp"),
    ("zh-sjc", "sjc"),
    ("zh-wuu", "wuu"),
    ("zh-yue", "yue"),
    ("zhk", "sgn-zhk"),
    ("zib", "sgn-zib"),
    ("zir", "scv"),
    ("zkb", "kjh"),
    ("zko", "xss"),
    ("zlm", "ms-zlm"),
    ("zmi", "ms-zmi"),
    ("zom", "yos"),
    ("zsl", "sgn-zsl"),
    ("zsm", "ms-zsm"),
];

/// IANA LSR equivalents (one-to-many).
static MULTI_EQUIVS_MAP: [(&str, &[&str]); 117] = [
    ("aeb", &["ajt", "ar-aeb"]),
    ("ajp", &["apc", "ar-ajp", "ar-apc"]),
    ("ajt", &["aeb", "ar-aeb"]),
    ("apc", &["ajp", "ar-ajp", "ar-apc"]),
    ("ar-aeb", &["aeb", "ajt"]),
    ("ar-ajp", &["apc", "ajp", "ar-apc"]),
    ("ar-apc", &["apc", "ajp", "ar-ajp"]),
    ("ase", &["sgn-ase", "sgn-us"]),
    ("bfi", &["sgn-bfi", "sgn-gb"]),
    ("blg", &["iba", "snb"]),
    ("bzs", &["sgn-bzs", "sgn-br"]),
    ("ccq", &["rki", "ybd"]),
    ("cmn", &["zh-cmn", "zh-guoyu"]),
    ("coy", &["pij", "nts"]),
    ("csn", &["sgn-csn", "sgn-co"]),
    ("drr", &["kzk", "gli"]),
    ("drw", &["prs", "tnf"]),
    ("dse", &["sgn-dse", "sgn-nl"]),
    ("dsl", &["sgn-dsl", "sgn-dk"]),
    ("dtp", &["ktr", "kzj", "kzt", "tdu"]),
    ("fsl", &["sgn-fsl", "sgn-fr"]),
    ("gfx", &["vaj", "mwj", "oun"]),
    ("gli", &["kzk", "drr"]),
    ("gsg", &["sgn-gsg", "sgn-de"]),
    ("gss", &["sgn-gss", "sgn-gr"]),
    ("hak", &["zh-hak", "i-hak", "zh-hakka"]),
    ("hsn", &["zh-hsn", "zh-xiang"]),
    ("i-hak", &["hak", "zh-hak", "zh-hakka"]),
    ("iba", &["blg", "snb"]),
    ("ise", &["sgn-ise", "sgn-it"]),
    ("isg", &["sgn-isg", "sgn-ie"]),
    ("jeg", &["oyb", "skk", "thx"]),
    ("jsl", &["sgn-jsl", "sgn-jp"]),
    ("ktr", &["dtp", "kzj", "kzt", "tdu"]),
    ("kzj", &["dtp", "ktr", "kzt", "tdu"]),
    ("kzk", &["drr", "gli"]),
    ("kzt", &["dtp", "ktr", "kzj", "tdu"]),
    ("mfs", &["sgn-mfs", "sgn-mx"]),
    ("mry", &["mst", "myt"]),
    ("mst", &["mry", "myt"]),
    ("mwj", &["vaj", "gfx", "oun"]),
    ("myt", &["mry", "mst"]),
    ("nan", &["zh-nan", "zh-min-nan"]),
    ("ncs", &["sgn-ncs", "sgn-ni"]),
    ("nsl", &["sgn-nsl", "sgn-no"]),
    ("nts", &["pij", "coy"]),
    ("oun", &["vaj", "gfx", "mwj"]),
    ("oyb", &["jeg", "skk", "thx"]),
    ("pij", &["coy", "nts"]),
    ("prs", &["drw", "tnf"]),
    ("psr", &["sgn-psr", "sgn-pt"]),
    ("rki", &["ccq", "ybd"]),
    ("sfb", &["sgn-sfb", "sgn-be-fr"]),
    ("sfs", &["sgn-sfs", "sgn-za"]),
    ("sgg", &["sgn-sgg", "sgn-ch-de"]),
    ("sgn-ase", &["ase", "sgn-us"]),
    ("sgn-be-fr", &["sfb", "sgn-sfb"]),
    ("sgn-be-nl", &["vgt", "sgn-vgt"]),
    ("sgn-bfi", &["bfi", "sgn-gb"]),
    ("sgn-br", &["bzs", "sgn-bzs"]),
    ("sgn-bzs", &["bzs", "sgn-br"]),
    ("sgn-ch-de", &["sgg", "sgn-sgg"]),
    ("sgn-co", &["csn", "sgn-csn"]),
    ("sgn-csn", &["csn", "sgn-co"]),
    ("sgn-de", &["gsg", "sgn-gsg"]),
    ("sgn-dk", &["dsl", "sgn-dsl"]),
    ("sgn-dse", &["dse", "sgn-nl"]),
    ("sgn-dsl", &["dsl", "sgn-dk"]),
    ("sgn-es", &["ssp", "sgn-ssp"]),
    ("sgn-fr", &["fsl", "sgn-fsl"]),
    ("sgn-fsl", &["fsl", "sgn-fr"]),
    ("sgn-gb", &["bfi", "sgn-bfi"]),
    ("sgn-gr", &["gss", "sgn-gss"]),
    ("sgn-gsg", &["gsg", "sgn-de"]),
    ("sgn-gss", &["gss", "sgn-gr"]),
    ("sgn-ie", &["isg", "sgn-isg"]),
    ("sgn-ise", &["ise", "sgn-it"]),
    ("sgn-isg", &["isg", "sgn-ie"]),
    ("sgn-it", &["ise", "sgn-ise"]),
    ("sgn-jp", &["jsl", "sgn-jsl"]),
    ("sgn-jsl", &["jsl", "sgn-jp"]),
    ("sgn-mfs", &["mfs", "sgn-mx"]),
    ("sgn-mx", &["mfs", "sgn-mfs"]),
    ("sgn-ncs", &["ncs", "sgn-ni"]),
    ("sgn-ni", &["ncs", "sgn-ncs"]),
    ("sgn-nl", &["dse", "sgn-dse"]),
    ("sgn-no", &["nsl", "sgn-nsl"]),
    ("sgn-nsl", &["nsl", "sgn-no"]),
    ("sgn-psr", &["psr", "sgn-pt"]),
    ("sgn-pt", &["psr", "sgn-psr"]),
    ("sgn-se", &["swl", "sgn-swl"]),
    ("sgn-sfb", &["sfb", "sgn-be-fr"]),
    ("sgn-sfs", &["sfs", "sgn-za"]),
    ("sgn-sgg", &["sgg", "sgn-ch-de"]),
    ("sgn-ssp", &["ssp", "sgn-es"]),
    ("sgn-swl", &["swl", "sgn-se"]),
    ("sgn-us", &["ase", "sgn-ase"]),
    ("sgn-vgt", &["vgt", "sgn-be-nl"]),
    ("sgn-za", &["sfs", "sgn-sfs"]),
    ("skk", &["oyb", "jeg", "thx"]),
    ("snb", &["iba", "blg"]),
    ("ssp", &["sgn-ssp", "sgn-es"]),
    ("swl", &["sgn-swl", "sgn-se"]),
    ("tdu", &["dtp", "ktr", "kzj", "kzt"]),
    ("thx", &["oyb", "jeg", "skk"]),
    ("tnf", &["prs", "drw"]),
    ("vaj", &["gfx", "mwj", "oun"]),
    ("vgt", &["sgn-vgt", "sgn-be-nl"]),
    ("ybd", &["rki", "ccq"]),
    ("zh-cmn", &["cmn", "zh-guoyu"]),
    ("zh-guoyu", &["cmn", "zh-cmn"]),
    ("zh-hak", &["hak", "i-hak", "zh-hakka"]),
    ("zh-hakka", &["hak", "zh-hak", "i-hak"]),
    ("zh-hsn", &["hsn", "zh-xiang"]),
    ("zh-min-nan", &["nan", "zh-nan"]),
    ("zh-nan", &["nan", "zh-min-nan"]),
    ("zh-xiang", &["hsn", "zh-hsn"]),
];

/// IANA LSR region/variant equivalents.
static REGION_VARIANT_EQUIV_MAP: [(&str, &str); 14] = [
    ("-alalc97", "-heploc"),
    ("-bu", "-mm"),
    ("-cd", "-zr"),
    ("-dd", "-de"),
    ("-de", "-dd"),
    ("-fr", "-fx"),
    ("-fx", "-fr"),
    ("-heploc", "-alalc97"),
    ("-mm", "-bu"),
    ("-tl", "-tp"),
    ("-tp", "-tl"),
    ("-yd", "-ye"),
    ("-ye", "-yd"),
    ("-zr", "-cd"),
];
