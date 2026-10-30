//! `javax.xml.datatype` and the `javax.xml.namespace.QName` it depends on.
//!
//! `DatatypeFactory`, `Duration`, and `XMLGregorianCalendar` are implemented directly on the
//! abstract classes: the registry has no bytecode bodies, so every method the JDK's concrete
//! `DatatypeFactoryImpl`/`DurationImpl`/`XMLGregorianCalendarImpl` would inherit is provided here
//! and the concrete subclasses stay empty. The value model covers the components the JAXP tests
//! observe (years through milliseconds, fractional seconds, timezone offset) and the XSD lexical
//! forms (`PnYnMnDTnHnMnS`, `yyyy-MM-ddTHH:mm:ss.SSS+hh:mm`).

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{
    boolean, class, field, int, long, method, object, receiver, ref_arg, string_arg, void,
};

/// `Integer.MIN_VALUE`: the JDK's unspecified field value.
const UNDEFINED: i32 = i32::MIN;

fn init_void(
    _vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    _args: &[Value],
) -> Result<Value, VmError> {
    void()
}

// ---------------------------------------------------------------------------------------------
// javax.xml.namespace.QName
// ---------------------------------------------------------------------------------------------

const QNAME_METHODS: [oxjvm_vm::NativeMethodDef; 9] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        qname_init_two,
    ),
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
        ACC_PUBLIC,
        qname_init_three,
    ),
    method(
        "<init>",
        "(Ljava/lang/String;)V",
        ACC_PUBLIC,
        qname_init_local,
    ),
    method(
        "getNamespaceURI",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(vm.intern(&qname_text(vm, receiver(args), "namespaceURI"))?),
    ),
    method(
        "getLocalPart",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(vm.intern(&qname_text(vm, receiver(args), "localPart"))?),
    ),
    method(
        "getPrefix",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(vm.intern(&qname_text(vm, receiver(args), "prefix"))?),
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC | ACC_FINAL,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            if other.is_null() || vm.class_of(other) != vm.class_of(receiver(args)) {
                return boolean(false);
            }
            let this = receiver(args);
            boolean(
                qname_text(vm, this, "namespaceURI") == qname_text(vm, other, "namespaceURI")
                    && qname_text(vm, this, "localPart") == qname_text(vm, other, "localPart"),
            )
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC | ACC_FINAL, |vm, _, args| {
        let this = receiver(args);
        int(hash_code(&qname_text(vm, this, "namespaceURI"))
            ^ hash_code(&qname_text(vm, this, "localPart")))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let prefix = qname_text(vm, this, "prefix");
            let local = qname_text(vm, this, "localPart");
            let text = if prefix.is_empty() {
                local
            } else {
                alloc::format!("{prefix}:{local}")
            };
            object(vm.intern(&text)?)
        },
    ),
];

pub(crate) const QNAME: oxjvm_vm::NativeClass = class(
    "javax/xml/namespace/QName",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC,
    &[
        field(
            "namespaceURI",
            "Ljava/lang/String;",
            ACC_PRIVATE | ACC_FINAL,
            None,
        ),
        field(
            "localPart",
            "Ljava/lang/String;",
            ACC_PRIVATE | ACC_FINAL,
            None,
        ),
        field(
            "prefix",
            "Ljava/lang/String;",
            ACC_PRIVATE | ACC_FINAL,
            None,
        ),
    ],
    &QNAME_METHODS,
    None,
);

fn qname_text(vm: &Vm<'_>, this: ObjectRef, name: &str) -> String {
    vm.read_string_field(this, vm.class_of(this), name)
        .unwrap_or_default()
}

fn qname_init_two(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let namespace = string_arg(vm, args[1])?;
    let local = string_arg(vm, args[2])?;
    let this = receiver(args);
    vm.set_string_field(this, vm.class_of(this), "namespaceURI", &namespace)?;
    vm.set_string_field(this, vm.class_of(this), "localPart", &local)?;
    void()
}

fn qname_init_three(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let namespace = string_arg(vm, args[1])?;
    let local = string_arg(vm, args[2])?;
    let prefix = string_arg(vm, args[3])?;
    let this = receiver(args);
    vm.set_string_field(this, vm.class_of(this), "namespaceURI", &namespace)?;
    vm.set_string_field(this, vm.class_of(this), "localPart", &local)?;
    vm.set_string_field(this, vm.class_of(this), "prefix", &prefix)?;
    void()
}

fn qname_init_local(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let local = string_arg(vm, args[1])?;
    let this = receiver(args);
    vm.set_string_field(this, vm.class_of(this), "namespaceURI", "")?;
    vm.set_string_field(this, vm.class_of(this), "localPart", &local)?;
    void()
}

fn new_qname_value(
    vm: &mut Vm<'_>,
    namespace: &str,
    local: &str,
    prefix: &str,
) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("javax/xml/namespace/QName")?;
    let instance = vm.new_instance(class)?;
    vm.set_string_field(instance, class, "namespaceURI", namespace)?;
    vm.set_string_field(instance, class, "localPart", local)?;
    vm.set_string_field(instance, class, "prefix", prefix)?;
    Ok(instance)
}

// ---------------------------------------------------------------------------------------------
// javax.xml.datatype.DatatypeConstants and Field
// ---------------------------------------------------------------------------------------------

const XML_SCHEMA_NS: &str = "http://www.w3.org/2001/XMLSchema";

/// The `Field` constants, in `getId()` order.
const FIELDS: [&str; 6] = ["YEARS", "MONTHS", "DAYS", "HOURS", "MINUTES", "SECONDS"];

/// The QName constants of `DatatypeConstants` and their local parts.
const QNAMES: [&str; 11] = [
    "DATETIME",
    "TIME",
    "DATE",
    "GYEARMONTH",
    "GMONTHDAY",
    "GYEAR",
    "GMONTH",
    "GDAY",
    "DURATION",
    "DURATION_DAYTIME",
    "DURATION_YEARMONTH",
];

const DATATYPE_CONSTANTS_METHODS: [oxjvm_vm::NativeMethodDef; 0] = [];

pub(crate) const DATATYPE_CONSTANTS: oxjvm_vm::NativeClass = class(
    "javax/xml/datatype/DatatypeConstants",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL,
    &DATATYPE_CONSTANT_FIELDS,
    &DATATYPE_CONSTANTS_METHODS,
    Some(datatype_constants_clinit),
);

static DATATYPE_CONSTANT_FIELDS: [oxjvm_vm::NativeFieldDef; 36] = [
    field("JANUARY", "I", ACC_PUBLIC | ACC_STATIC | ACC_FINAL, None),
    field("FEBRUARY", "I", ACC_PUBLIC | ACC_STATIC | ACC_FINAL, None),
    field("MARCH", "I", ACC_PUBLIC | ACC_STATIC | ACC_FINAL, None),
    field("APRIL", "I", ACC_PUBLIC | ACC_STATIC | ACC_FINAL, None),
    field("MAY", "I", ACC_PUBLIC | ACC_STATIC | ACC_FINAL, None),
    field("JUNE", "I", ACC_PUBLIC | ACC_STATIC | ACC_FINAL, None),
    field("JULY", "I", ACC_PUBLIC | ACC_STATIC | ACC_FINAL, None),
    field("AUGUST", "I", ACC_PUBLIC | ACC_STATIC | ACC_FINAL, None),
    field("SEPTEMBER", "I", ACC_PUBLIC | ACC_STATIC | ACC_FINAL, None),
    field("OCTOBER", "I", ACC_PUBLIC | ACC_STATIC | ACC_FINAL, None),
    field("NOVEMBER", "I", ACC_PUBLIC | ACC_STATIC | ACC_FINAL, None),
    field("DECEMBER", "I", ACC_PUBLIC | ACC_STATIC | ACC_FINAL, None),
    field("LESSER", "I", ACC_PUBLIC | ACC_STATIC | ACC_FINAL, None),
    field("EQUAL", "I", ACC_PUBLIC | ACC_STATIC | ACC_FINAL, None),
    field("GREATER", "I", ACC_PUBLIC | ACC_STATIC | ACC_FINAL, None),
    field(
        "INDETERMINATE",
        "I",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "FIELD_UNDEFINED",
        "I",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "MAX_TIMEZONE_OFFSET",
        "I",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "MIN_TIMEZONE_OFFSET",
        "I",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "YEARS",
        "Ljavax/xml/datatype/DatatypeConstants$Field;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "MONTHS",
        "Ljavax/xml/datatype/DatatypeConstants$Field;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "DAYS",
        "Ljavax/xml/datatype/DatatypeConstants$Field;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "HOURS",
        "Ljavax/xml/datatype/DatatypeConstants$Field;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "MINUTES",
        "Ljavax/xml/datatype/DatatypeConstants$Field;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "SECONDS",
        "Ljavax/xml/datatype/DatatypeConstants$Field;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "DATETIME",
        "Ljavax/xml/namespace/QName;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "TIME",
        "Ljavax/xml/namespace/QName;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "DATE",
        "Ljavax/xml/namespace/QName;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "GYEARMONTH",
        "Ljavax/xml/namespace/QName;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "GMONTHDAY",
        "Ljavax/xml/namespace/QName;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "GYEAR",
        "Ljavax/xml/namespace/QName;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "GMONTH",
        "Ljavax/xml/namespace/QName;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "GDAY",
        "Ljavax/xml/namespace/QName;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "DURATION",
        "Ljavax/xml/namespace/QName;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "DURATION_DAYTIME",
        "Ljavax/xml/namespace/QName;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
    field(
        "DURATION_YEARMONTH",
        "Ljavax/xml/namespace/QName;",
        ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
        None,
    ),
];

const FIELD_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method("<init>", "()V", ACC_PRIVATE, init_void),
    method("getId", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int(vm, receiver(args), "id"))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            object(vm.intern(&read_string(vm, this, "name"))?)
        },
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
            boolean(read_int(vm, receiver(args), "id") == read_int(vm, other, "id"))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int(vm, receiver(args), "id"))
    }),
];

pub(crate) const DATATYPE_FIELD: oxjvm_vm::NativeClass = class(
    "javax/xml/datatype/DatatypeConstants$Field",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("id", "I", ACC_PRIVATE | ACC_FINAL, None),
        field("name", "Ljava/lang/String;", ACC_PRIVATE | ACC_FINAL, None),
    ],
    &FIELD_METHODS,
    None,
);

fn datatype_constants_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("javax/xml/datatype/DatatypeConstants")?;
    for (index, name) in [
        "JANUARY",
        "FEBRUARY",
        "MARCH",
        "APRIL",
        "MAY",
        "JUNE",
        "JULY",
        "AUGUST",
        "SEPTEMBER",
        "OCTOBER",
        "NOVEMBER",
        "DECEMBER",
    ]
    .iter()
    .enumerate()
    {
        vm.set_static_value(class, name, "I", Value::Int(index as i32 + 1));
    }
    for (name, value) in [
        ("LESSER", -1),
        ("EQUAL", 0),
        ("GREATER", 1),
        ("INDETERMINATE", 2),
        ("FIELD_UNDEFINED", UNDEFINED),
        ("MAX_TIMEZONE_OFFSET", 14 * 60),
        ("MIN_TIMEZONE_OFFSET", -14 * 60),
    ] {
        vm.set_static_value(class, name, "I", Value::Int(value));
    }
    let field_class = vm.resolve_class("javax/xml/datatype/DatatypeConstants$Field")?;
    for (index, name) in FIELDS.iter().enumerate() {
        let instance = vm.new_instance(field_class)?;
        write_int(vm, instance, "id", index as i32 + 1);
        vm.set_string_field(instance, field_class, "name", name)?;
        vm.set_static_value(
            class,
            name,
            "Ljavax/xml/datatype/DatatypeConstants$Field;",
            Value::Ref(instance),
        );
    }
    for name in QNAMES {
        let qname = new_qname_value(vm, XML_SCHEMA_NS, name, "")?;
        vm.set_static_value(
            class,
            name,
            "Ljavax/xml/namespace/QName;",
            Value::Ref(qname),
        );
    }
    Ok(())
}

fn constant_qname(vm: &mut Vm<'_>, name: &str) -> Result<Value, VmError> {
    let class = vm.resolve_class("javax/xml/datatype/DatatypeConstants")?;
    vm.ensure_initialized(class)?;
    Ok(Value::Ref(
        vm.read_static_ref(class, name, "Ljavax/xml/namespace/QName;")
            .unwrap_or(ObjectRef::NULL),
    ))
}

// ---------------------------------------------------------------------------------------------
// javax.xml.datatype.DatatypeConfigurationException
// ---------------------------------------------------------------------------------------------

pub(crate) const DATATYPE_CONFIGURATION_EXCEPTION: oxjvm_vm::NativeClass = class(
    "javax/xml/datatype/DatatypeConfigurationException",
    Some("java/lang/Exception"),
    &[],
    ACC_PUBLIC,
    &[],
    &[
        method("<init>", "()V", ACC_PUBLIC, init_void),
        method(
            "<init>",
            "(Ljava/lang/String;)V",
            ACC_PUBLIC,
            crate::throwable::exception_ctor,
        ),
    ],
    None,
);

// ---------------------------------------------------------------------------------------------
// javax.xml.datatype.DatatypeFactory
// ---------------------------------------------------------------------------------------------

pub(crate) const DATATYPE_FACTORY: oxjvm_vm::NativeClass = class(
    "javax/xml/datatype/DatatypeFactory",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &[],
    &DATATYPE_FACTORY_METHODS,
    None,
);

const DATATYPE_FACTORY_METHODS: [oxjvm_vm::NativeMethodDef; 25] = [
    method("<init>", "()V", ACC_PROTECTED, init_void),
    method(
        "newDefaultInstance",
        "()Ljavax/xml/datatype/DatatypeFactory;",
        ACC_PUBLIC | ACC_STATIC,
        new_factory,
    ),
    method(
        "newInstance",
        "()Ljavax/xml/datatype/DatatypeFactory;",
        ACC_PUBLIC | ACC_STATIC,
        new_factory,
    ),
    method(
        "newInstance",
        "(Ljava/lang/String;Ljava/lang/ClassLoader;)Ljavax/xml/datatype/DatatypeFactory;",
        ACC_PUBLIC | ACC_STATIC,
        new_factory,
    ),
    method(
        "newDuration",
        "(Ljava/lang/String;)Ljavax/xml/datatype/Duration;",
        ACC_PUBLIC,
        new_duration_from_string,
    ),
    method(
        "newDuration",
        "(J)Ljavax/xml/datatype/Duration;",
        ACC_PUBLIC,
        new_duration_from_millis,
    ),
    method(
        "newDuration",
        "(ZLjava/math/BigInteger;Ljava/math/BigInteger;Ljava/math/BigInteger;Ljava/math/BigInteger;Ljava/math/BigInteger;Ljava/math/BigDecimal;)Ljavax/xml/datatype/Duration;",
        ACC_PUBLIC,
        new_duration_from_components,
    ),
    method(
        "newDuration",
        "(ZIIIIII)Ljavax/xml/datatype/Duration;",
        ACC_PUBLIC,
        new_duration_from_components,
    ),
    method(
        "newDurationDayTime",
        "(Ljava/lang/String;)Ljavax/xml/datatype/Duration;",
        ACC_PUBLIC,
        new_duration_from_string,
    ),
    method(
        "newDurationDayTime",
        "(J)Ljavax/xml/datatype/Duration;",
        ACC_PUBLIC,
        new_duration_from_millis,
    ),
    method(
        "newDurationDayTime",
        "(ZLjava/math/BigInteger;Ljava/math/BigInteger;Ljava/math/BigInteger;Ljava/math/BigInteger;)Ljavax/xml/datatype/Duration;",
        ACC_PUBLIC,
        new_duration_from_components,
    ),
    method(
        "newDurationDayTime",
        "(ZIIII)Ljavax/xml/datatype/Duration;",
        ACC_PUBLIC,
        new_duration_from_components,
    ),
    method(
        "newDurationYearMonth",
        "(Ljava/lang/String;)Ljavax/xml/datatype/Duration;",
        ACC_PUBLIC,
        new_duration_from_string,
    ),
    method(
        "newDurationYearMonth",
        "(J)Ljavax/xml/datatype/Duration;",
        ACC_PUBLIC,
        new_duration_from_millis,
    ),
    method(
        "newDurationYearMonth",
        "(ZLjava/math/BigInteger;Ljava/math/BigInteger;)Ljavax/xml/datatype/Duration;",
        ACC_PUBLIC,
        new_duration_from_components,
    ),
    method(
        "newDurationYearMonth",
        "(ZII)Ljavax/xml/datatype/Duration;",
        ACC_PUBLIC,
        new_duration_from_components,
    ),
    method(
        "newXMLGregorianCalendar",
        "()Ljavax/xml/datatype/XMLGregorianCalendar;",
        ACC_PUBLIC,
        |vm, _, _| object(new_calendar(vm)?),
    ),
    method(
        "newXMLGregorianCalendar",
        "(Ljava/lang/String;)Ljavax/xml/datatype/XMLGregorianCalendar;",
        ACC_PUBLIC,
        new_calendar_from_string,
    ),
    method(
        "newXMLGregorianCalendar",
        "(Ljava/util/GregorianCalendar;)Ljavax/xml/datatype/XMLGregorianCalendar;",
        ACC_PUBLIC,
        new_calendar_from_calendar,
    ),
    method(
        "newXMLGregorianCalendar",
        "(Ljava/math/BigInteger;IIIIILjava/math/BigDecimal;I)Ljavax/xml/datatype/XMLGregorianCalendar;",
        ACC_PUBLIC,
        new_calendar_from_big_integer,
    ),
    method(
        "newXMLGregorianCalendar",
        "(IIIIIIII)Ljavax/xml/datatype/XMLGregorianCalendar;",
        ACC_PUBLIC,
        new_calendar_from_ints,
    ),
    method(
        "newXMLGregorianCalendarDate",
        "(IIII)Ljavax/xml/datatype/XMLGregorianCalendar;",
        ACC_PUBLIC,
        new_calendar_date,
    ),
    method(
        "newXMLGregorianCalendarTime",
        "(IIII)Ljavax/xml/datatype/XMLGregorianCalendar;",
        ACC_PUBLIC,
        new_calendar_time,
    ),
    method(
        "newXMLGregorianCalendarTime",
        "(IIILjava/math/BigDecimal;I)Ljavax/xml/datatype/XMLGregorianCalendar;",
        ACC_PUBLIC,
        new_calendar_time_fractional,
    ),
    method(
        "newXMLGregorianCalendarTime",
        "(IIIII)Ljavax/xml/datatype/XMLGregorianCalendar;",
        ACC_PUBLIC,
        new_calendar_time_millis,
    ),
];

pub(crate) const DATATYPE_FACTORY_IMPL: oxjvm_vm::NativeClass = class(
    "javax/xml/datatype/DatatypeFactory$NewInstance",
    Some("javax/xml/datatype/DatatypeFactory"),
    &[],
    ACC_PRIVATE | ACC_STATIC,
    &[],
    &[],
    None,
);

fn new_factory(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    _args: &[Value],
) -> Result<Value, VmError> {
    let class = vm.resolve_class("javax/xml/datatype/DatatypeFactory$NewInstance")?;
    let instance = vm.new_instance(class)?;
    object(instance)
}

// ---------------------------------------------------------------------------------------------
// Duration
// ---------------------------------------------------------------------------------------------

/// Bits of the `setMask` field: one per component in `DURATION_FIELD_NAMES` order.
const MASK_YEARS: i32 = 1 << 0;
const MASK_MONTHS: i32 = 1 << 1;
const MASK_DAYS: i32 = 1 << 2;
const MASK_HOURS: i32 = 1 << 3;
const MASK_MINUTES: i32 = 1 << 4;
const MASK_SECONDS: i32 = 1 << 5;
const MASK_ALL: i32 = 0b11_1111;
const MASK_DATE: i32 = MASK_YEARS | MASK_MONTHS;
const MASK_TIME: i32 = MASK_DAYS | MASK_HOURS | MASK_MINUTES | MASK_SECONDS;

const DURATION_FIELD_NAMES: [&str; 8] = [
    "signum", "years", "months", "days", "hours", "minutes", "seconds", "millis",
];

pub(crate) const DURATION: oxjvm_vm::NativeClass = class(
    "javax/xml/datatype/Duration",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_ABSTRACT,
    &DURATION_FIELD_DEFS,
    &DURATION_METHODS,
    None,
);

static DURATION_FIELD_DEFS: [oxjvm_vm::NativeFieldDef; 10] = [
    field("signum", "I", ACC_PRIVATE, None),
    field("years", "I", ACC_PRIVATE, None),
    field("months", "I", ACC_PRIVATE, None),
    field("days", "I", ACC_PRIVATE, None),
    field("hours", "I", ACC_PRIVATE, None),
    field("minutes", "I", ACC_PRIVATE, None),
    field("seconds", "I", ACC_PRIVATE, None),
    field("millis", "I", ACC_PRIVATE, None),
    field("setMask", "I", ACC_PRIVATE, None),
    field("secondsFraction", "Ljava/lang/String;", ACC_PRIVATE, None),
];

const DURATION_METHODS: [oxjvm_vm::NativeMethodDef; 27] = [
    method("<init>", "()V", ACC_PUBLIC, init_void),
    method("getSign", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int(vm, receiver(args), "signum"))
    }),
    method("getYears", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int(vm, receiver(args), "years"))
    }),
    method("getMonths", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int(vm, receiver(args), "months"))
    }),
    method("getDays", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int(vm, receiver(args), "days"))
    }),
    method("getHours", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int(vm, receiver(args), "hours"))
    }),
    method("getMinutes", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int(vm, receiver(args), "minutes"))
    }),
    method("getSeconds", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int(vm, receiver(args), "seconds"))
    }),
    method(
        "getTimeInMillis",
        "(Ljava/util/Date;)J",
        ACC_PUBLIC,
        duration_millis_from_date,
    ),
    method(
        "getTimeInMillis",
        "(Ljava/util/Calendar;)J",
        ACC_PUBLIC,
        duration_millis_from_calendar,
    ),
    method("addTo", "(Ljava/util/Date;)V", ACC_PUBLIC, add_to_date),
    method(
        "addTo",
        "(Ljava/util/Calendar;)V",
        ACC_PUBLIC,
        add_to_calendar,
    ),
    method(
        "getField",
        "(Ljavax/xml/datatype/DatatypeConstants$Field;)Ljava/lang/Number;",
        ACC_PUBLIC,
        duration_get_field,
    ),
    method(
        "isSet",
        "(Ljavax/xml/datatype/DatatypeConstants$Field;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let field = ref_arg(args, 1);
            if field.is_null() {
                return Err(vm.throw_new("java/lang/NullPointerException", None));
            }
            let id = read_int(vm, field, "id");
            let mask = read_int(vm, receiver(args), "setMask");
            boolean((1..=6).contains(&id) && mask & (1 << (id - 1)) != 0)
        },
    ),
    method(
        "getXMLSchemaType",
        "()Ljavax/xml/namespace/QName;",
        ACC_PUBLIC,
        duration_schema_type,
    ),
    method(
        "add",
        "(Ljavax/xml/datatype/Duration;)Ljavax/xml/datatype/Duration;",
        ACC_PUBLIC,
        duration_add,
    ),
    method(
        "subtract",
        "(Ljavax/xml/datatype/Duration;)Ljavax/xml/datatype/Duration;",
        ACC_PUBLIC,
        duration_subtract,
    ),
    method(
        "multiply",
        "(I)Ljavax/xml/datatype/Duration;",
        ACC_PUBLIC,
        duration_multiply_int,
    ),
    method(
        "multiply",
        "(Ljava/math/BigDecimal;)Ljavax/xml/datatype/Duration;",
        ACC_PUBLIC,
        duration_multiply_decimal,
    ),
    method(
        "negate",
        "()Ljavax/xml/datatype/Duration;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let mut parts = read_duration_fields(vm, this);
            parts[0] = -parts[0];
            duration_new(
                vm,
                parts,
                read_int(vm, this, "setMask"),
                read_fraction(vm, this).as_deref(),
            )
        },
    ),
    method(
        "normalizeWith",
        "(Ljava/util/Calendar;)Ljavax/xml/datatype/Duration;",
        ACC_PUBLIC,
        |_, _, args| object(receiver(args)),
    ),
    method(
        "compare",
        "(Ljavax/xml/datatype/Duration;)I",
        ACC_PUBLIC,
        |vm, _, args| int(compare_durations(vm, receiver(args), ref_arg(args, 1))),
    ),
    method(
        "isLongerThan",
        "(Ljavax/xml/datatype/Duration;)Z",
        ACC_PUBLIC,
        |vm, _, args| boolean(compare_durations(vm, receiver(args), ref_arg(args, 1)) > 0),
    ),
    method(
        "isShorterThan",
        "(Ljavax/xml/datatype/Duration;)Z",
        ACC_PUBLIC,
        |vm, _, args| boolean(compare_durations(vm, receiver(args), ref_arg(args, 1)) < 0),
    ),
    method(
        "equals",
        "(Ljava/lang/Object;)Z",
        ACC_PUBLIC,
        |vm, _, args| {
            let other = ref_arg(args, 1);
            let duration = vm.resolve_class("javax/xml/datatype/Duration")?;
            if other.is_null() || !vm.is_instance(other, duration) {
                return boolean(false);
            }
            boolean(read_duration_fields(vm, receiver(args)) == read_duration_fields(vm, other))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(hash_code(&duration_to_string(vm, receiver(args))))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(vm.intern(&duration_to_string(vm, receiver(args)))?),
    ),
];

pub(crate) const DURATION_NEW_INSTANCE: oxjvm_vm::NativeClass = class(
    "javax/xml/datatype/Duration$NewInstance",
    Some("javax/xml/datatype/Duration"),
    &[],
    ACC_PRIVATE | ACC_STATIC,
    &[],
    &[],
    None,
);

fn read_int(vm: &Vm<'_>, this: ObjectRef, name: &str) -> i32 {
    let class = vm.class_of(this);
    vm.read_instance_int_named(this, class, name, "I")
        .unwrap_or(0)
}

fn read_string(vm: &Vm<'_>, this: ObjectRef, name: &str) -> String {
    vm.read_string_field(this, vm.class_of(this), name)
        .unwrap_or_default()
}

fn read_fraction(vm: &Vm<'_>, this: ObjectRef) -> Option<String> {
    let class = vm.class_of(this);
    let reference = vm
        .read_ref_field(this, class, "secondsFraction", "Ljava/lang/String;")
        .unwrap_or(ObjectRef::NULL);
    if reference.is_null() {
        None
    } else {
        Some(vm.string_or_empty(reference))
    }
}

fn write_int(vm: &mut Vm<'_>, this: ObjectRef, name: &str, value: i32) {
    let class = vm.class_of(this);
    if let Some((declaring, slot)) = vm.find_field_opt(class, name, "I") {
        vm.set_instance_int(this, declaring, slot, value);
    }
}

fn write_string(vm: &mut Vm<'_>, this: ObjectRef, name: &str, value: &str) -> Result<(), VmError> {
    let class = vm.class_of(this);
    let (declaring, slot) = vm.find_field(class, name, "Ljava/lang/String;")?;
    let text = vm.intern(value)?;
    vm.set_instance_ref(this, declaring, slot, text);
    Ok(())
}

fn read_duration_fields(vm: &Vm<'_>, this: ObjectRef) -> [i32; 8] {
    let mut parts = [0i32; 8];
    for (index, name) in DURATION_FIELD_NAMES.iter().enumerate() {
        parts[index] = read_int(vm, this, name);
    }
    parts
}

fn duration_new(
    vm: &mut Vm<'_>,
    parts: [i32; 8],
    mask: i32,
    fraction: Option<&str>,
) -> Result<Value, VmError> {
    let class = vm.resolve_class("javax/xml/datatype/Duration$NewInstance")?;
    let instance = vm.new_instance(class)?;
    for (index, name) in DURATION_FIELD_NAMES.iter().enumerate() {
        write_int(vm, instance, name, parts[index]);
    }
    write_int(vm, instance, "setMask", mask);
    if let Some(fraction) = fraction {
        write_string(vm, instance, "secondsFraction", fraction)?;
    }
    object(instance)
}

fn duration_millis_from_any(vm: &mut Vm<'_>, this: ObjectRef) -> i64 {
    let parts = read_duration_fields(vm, this);
    let sign = i64::from(parts[0].signum());
    sign * (i64::from(parts[6]) * 1000
        + i64::from(parts[7])
        + 86_400_000 * i64::from(parts[3])
        + 3_600_000 * i64::from(parts[4])
        + 60_000 * i64::from(parts[5]))
}

fn duration_millis_from_date(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    long(duration_millis_from_any(vm, receiver(args)))
}

fn duration_millis_from_calendar(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    long(duration_millis_from_any(vm, receiver(args)))
}

fn add_to_date(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let millis = duration_millis_from_any(vm, receiver(args));
    let date = ref_arg(args, 1);
    let (class, get) = vm.resolve_virtual_method(date, "getTime", "()J")?;
    let now = vm
        .invoke_method(class, get, alloc::vec![Value::Ref(date)])?
        .as_long();
    let (class, set) = vm.resolve_virtual_method(date, "setTime", "(J)V")?;
    vm.invoke_method(
        class,
        set,
        alloc::vec![Value::Ref(date), Value::Long(now + millis)],
    )?;
    void()
}

fn add_to_calendar(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let millis = duration_millis_from_any(vm, receiver(args));
    let calendar = ref_arg(args, 1);
    let (class, get) = vm.resolve_virtual_method(calendar, "getTimeInMillis", "()J")?;
    let now = vm
        .invoke_method(class, get, alloc::vec![Value::Ref(calendar)])?
        .as_long();
    let (class, set) = vm.resolve_virtual_method(calendar, "setTimeInMillis", "(J)V")?;
    vm.invoke_method(
        class,
        set,
        alloc::vec![Value::Ref(calendar), Value::Long(now + millis)],
    )?;
    void()
}

fn duration_get_field(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let field = ref_arg(args, 1);
    if field.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let id = read_int(vm, field, "id");
    let this = receiver(args);
    let mask = read_int(vm, this, "setMask");
    if !(1..=6).contains(&id) || mask & (1 << (id - 1)) == 0 {
        return object(ObjectRef::NULL);
    }
    let parts = read_duration_fields(vm, this);
    if id == 6 {
        let fraction = read_fraction(vm, this).unwrap_or_else(|| alloc::format!("{:03}", parts[7]));
        let text = alloc::format!("{}.{fraction}", parts[6]);
        crate::math::create_big_number(vm, "java/math/BigDecimal", &text)
    } else {
        crate::math::create_big_number(vm, "java/math/BigInteger", &parts[id as usize].to_string())
    }
}

fn duration_schema_type(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let mask = read_int(vm, this, "setMask");
    let has_date = mask & MASK_DATE != 0;
    let has_time = mask & MASK_TIME != 0;
    match (has_date, has_time) {
        (true, false) => constant_qname(vm, "DURATION_YEARMONTH"),
        (false, true) => constant_qname(vm, "DURATION_DAYTIME"),
        (true, true) => constant_qname(vm, "DURATION"),
        (false, false) => Err(vm.throw_new(
            "java/lang/IllegalStateException",
            Some("this Duration does not match one of the XML Schema date/time datatypes"),
        )),
    }
}

fn duration_add(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let left = receiver(args);
    let right = ref_arg(args, 1);
    let lparts = read_duration_fields(vm, left);
    let rparts = read_duration_fields(vm, right);
    let lmask = read_int(vm, left, "setMask");
    let rmask = read_int(vm, right, "setMask");
    combine_durations(vm, &lparts, lmask, &rparts, rmask, 1)
}

fn duration_subtract(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let left = receiver(args);
    let right = ref_arg(args, 1);
    let lparts = read_duration_fields(vm, left);
    let rparts = read_duration_fields(vm, right);
    let lmask = read_int(vm, left, "setMask");
    let rmask = read_int(vm, right, "setMask");
    combine_durations(vm, &lparts, lmask, &rparts, rmask, -1)
}

fn combine_durations(
    vm: &mut Vm<'_>,
    lparts: &[i32; 8],
    lmask: i32,
    rparts: &[i32; 8],
    rmask: i32,
    sign: i32,
) -> Result<Value, VmError> {
    let lsign = i64::from(lparts[0].signum());
    let rsign = i64::from(rparts[0].signum() * sign);
    let mut months = lsign * i64::from(lparts[1] * 12 + lparts[2])
        + rsign * i64::from(rparts[1] * 12 + rparts[2]);
    let mut millis = lsign * total_millis(lparts) + rsign * total_millis(rparts);
    let months_supplied = lmask & MASK_DATE != 0 && lparts[1] + lparts[2] != 0
        || rmask & MASK_DATE != 0 && rparts[1] + rparts[2] != 0;
    let time_supplied = lmask & MASK_TIME != 0 || rmask & MASK_TIME != 0;
    if months_supplied && time_supplied && months.signum() * millis.signum() < 0 {
        return Err(vm.throw_new(
            "java/lang/IllegalStateException",
            Some("years and months cannot be subtracted from days and time"),
        ));
    }
    let negative = months < 0 || millis < 0;
    let signum = if months == 0 && millis == 0 {
        0
    } else if negative {
        -1
    } else {
        1
    };
    months = months.abs();
    millis = millis.abs();
    let years = months / 12;
    let months = months % 12;
    let days = millis / 86_400_000;
    let millis = millis % 86_400_000;
    let hours = millis / 3_600_000;
    let millis = millis % 3_600_000;
    let minutes = millis / 60_000;
    let millis = millis % 60_000;
    let seconds = millis / 1000;
    let millis = millis % 1000;
    let mut parts = [
        if negative { -1 } else { signum },
        years as i32,
        months as i32,
        days as i32,
        hours as i32,
        minutes as i32,
        seconds as i32,
        millis as i32,
    ];
    if parts[0] == 0 {
        parts[0] = signum;
    }
    let mask = nonzero_mask(&parts);
    duration_new(vm, parts, mask, None)
}

fn total_millis(parts: &[i32; 8]) -> i64 {
    i64::from(parts[3]) * 86_400_000
        + i64::from(parts[4]) * 3_600_000
        + i64::from(parts[5]) * 60_000
        + i64::from(parts[6]) * 1000
        + i64::from(parts[7])
}

fn nonzero_mask(parts: &[i32; 8]) -> i32 {
    let mut mask = 0;
    for (index, bit) in [
        MASK_YEARS,
        MASK_MONTHS,
        MASK_DAYS,
        MASK_HOURS,
        MASK_MINUTES,
        MASK_SECONDS,
    ]
    .iter()
    .enumerate()
    {
        if parts[index + 1] != 0 || (index == 5 && parts[7] != 0) {
            mask |= bit;
        }
    }
    if mask == 0 {
        mask = MASK_SECONDS;
    }
    mask
}

/// Carry day-time components up: milliseconds to seconds, seconds to minutes, and so on.
fn normalize_time_fields(parts: &mut [i32; 8], fraction: Option<&str>) {
    let mut millis = i64::from(parts[7]);
    if let Some(fraction) = fraction {
        if fraction.len() > 3 {
            // Values beyond milliseconds are truncated, which is what the component granularity
            // allows; the lexical form keeps them, so only whole seconds need carrying.
            millis = i64::from(parts[7]);
        }
    }
    let total = i64::from(parts[3]) * 86_400_000
        + i64::from(parts[4]) * 3_600_000
        + i64::from(parts[5]) * 60_000
        + i64::from(parts[6]) * 1000
        + millis;
    parts[3] = (total / 86_400_000) as i32;
    let rest = total % 86_400_000;
    parts[4] = (rest / 3_600_000) as i32;
    let rest = rest % 3_600_000;
    parts[5] = (rest / 60_000) as i32;
    let rest = rest % 60_000;
    parts[6] = (rest / 1000) as i32;
    if fraction.is_none() {
        parts[7] = (rest % 1000) as i32;
    }
}

fn duration_multiply_int(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    multiply_duration(vm, receiver(args), f64::from(args[1].as_int()))
}

fn duration_multiply_decimal(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let decimal = ref_arg(args, 1);
    let (class, method) = vm.resolve_virtual_method(decimal, "doubleValue", "()D")?;
    let factor = vm
        .invoke_method(class, method, alloc::vec![Value::Ref(decimal)])?
        .as_double();
    multiply_duration(vm, receiver(args), factor)
}

fn multiply_duration(vm: &mut Vm<'_>, this: ObjectRef, factor: f64) -> Result<Value, VmError> {
    let parts = read_duration_fields(vm, this);
    let sign = parts[0].signum();
    let months = ((parts[1] * 12 + parts[2]) as f64 * factor) as i64;
    let millis = (total_millis(&parts) as f64 * factor) as i64;
    let negative = months < 0 || millis < 0;
    let months = months.abs();
    let millis = millis.abs();
    let mut result = [
        if negative {
            -1
        } else if months == 0 && millis == 0 {
            0
        } else {
            sign
        },
        (months / 12) as i32,
        (months % 12) as i32,
        (millis / 86_400_000) as i32,
        ((millis % 86_400_000) / 3_600_000) as i32,
        ((millis % 3_600_000) / 60_000) as i32,
        ((millis % 60_000) / 1000) as i32,
        (millis % 1000) as i32,
    ];
    if result[0] == 0 {
        result[0] = 0;
    }
    let mask = nonzero_mask(&result);
    duration_new(vm, result, mask, None)
}

fn duration_to_string(vm: &Vm<'_>, this: ObjectRef) -> String {
    let parts = read_duration_fields(vm, this);
    let mask = read_int(vm, this, "setMask");
    let sign = parts[0];
    let mut text = String::from(if sign < 0 { "-P" } else { "P" });
    let mut wrote_date = false;
    if mask & MASK_YEARS != 0 {
        text.push_str(&alloc::format!("{}Y", parts[1].abs()));
        wrote_date = true;
    }
    if mask & MASK_MONTHS != 0 {
        text.push_str(&alloc::format!("{}M", parts[2].abs()));
        wrote_date = true;
    }
    if mask & MASK_DAYS != 0 {
        text.push_str(&alloc::format!("{}D", parts[3].abs()));
        wrote_date = true;
    }
    let has_time = mask & (MASK_HOURS | MASK_MINUTES | MASK_SECONDS) != 0;
    if has_time {
        text.push('T');
        if mask & MASK_HOURS != 0 {
            text.push_str(&alloc::format!("{}H", parts[4].abs()));
        }
        if mask & MASK_MINUTES != 0 {
            text.push_str(&alloc::format!("{}M", parts[5].abs()));
        }
        if mask & MASK_SECONDS != 0 {
            text.push_str(&alloc::format!("{}", parts[6].abs()));
            if let Some(fraction) = read_fraction(vm, this) {
                if !fraction.is_empty() {
                    text.push('.');
                    text.push_str(&fraction);
                }
            } else if parts[7] != 0 {
                let mut fraction = alloc::format!("{:03}", parts[7].abs());
                while fraction.ends_with('0') {
                    fraction.pop();
                }
                text.push('.');
                text.push_str(&fraction);
            }
            text.push('S');
        }
    }
    if !wrote_date && !has_time {
        text.push_str("T0S");
    }
    text
}

fn compare_durations(vm: &Vm<'_>, a: ObjectRef, b: ObjectRef) -> i32 {
    let left = read_duration_fields(vm, a);
    let right = read_duration_fields(vm, b);
    let lmask = read_int(vm, a, "setMask");
    let rmask = read_int(vm, b, "setMask");
    let month_group = lmask & MASK_DATE != 0 || rmask & MASK_DATE != 0;
    let time_group = lmask & MASK_TIME != 0 || rmask & MASK_TIME != 0;
    if month_group && !time_group {
        let x = left[0].signum() * (left[1] * 12 + left[2]);
        let y = right[0].signum() * (right[1] * 12 + right[2]);
        return x.cmp(&y) as i32;
    }
    if time_group && !month_group {
        let x = i64::from(left[0].signum()) * total_millis(&left);
        let y = i64::from(right[0].signum()) * total_millis(&right);
        return x.cmp(&y) as i32;
    }
    for index in 1..8 {
        let x = left[index] * left[0].signum();
        let y = right[index] * right[0].signum();
        if x != y {
            return if x < y { -1 } else { 1 };
        }
    }
    0
}

fn new_duration_from_string(
    vm: &mut Vm<'_>,
    ctx: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = string_arg(vm, args[1])?;
    let Some(parsed) = parse_duration(&text) else {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!("Invalid duration: {text}")),
        ));
    };
    let name = vm.classes.get(ctx.class).methods[ctx.method as usize]
        .name
        .clone();
    let mut parts = parsed.parts;
    let mut mask = parsed.mask;
    let mut fraction = parsed.fraction;
    if name == "newDurationYearMonth" {
        if mask & (MASK_DAYS | MASK_TIME) != 0 {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("newDurationYearMonth requires a year-month lexical form"),
            ));
        }
        let months = parts[1] * 12 + parts[2];
        parts[1] = months / 12;
        parts[2] = months % 12;
        parts[3] = 0;
        parts[4] = 0;
        parts[5] = 0;
        parts[6] = 0;
        parts[7] = 0;
        mask = 0;
        if parts[1] != 0 {
            mask |= MASK_YEARS;
        }
        if parts[2] != 0 {
            mask |= MASK_MONTHS;
        }
        fraction = None;
    } else if name == "newDurationDayTime" {
        parts[1] = 0;
        parts[2] = 0;
        mask &= MASK_TIME;
        let fraction_digits = fraction.clone();
        normalize_time_fields(&mut parts, fraction_digits.as_deref());
        fraction = fraction_digits;
    }
    duration_new(vm, parts, mask, fraction.as_deref())
}

fn new_duration_from_millis(
    vm: &mut Vm<'_>,
    ctx: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let millis = crate::long_arg(args, 1);
    let name = vm.classes.get(ctx.class).methods[ctx.method as usize]
        .name
        .clone();
    if name == "newDurationYearMonth" {
        // One month is a 365.2425/12-day month, like the JDK's conversion.
        let sign = millis.signum() as i32;
        let months = (millis.unsigned_abs() / 2_629_746_000) as i32;
        let (years, months) = (months / 12, months % 12);
        let sign = if years == 0 && months == 0 { 0 } else { sign };
        let parts = [sign, years, months, 0, 0, 0, 0, 0];
        return duration_new(vm, parts, MASK_DATE, None);
    }
    if name == "newDurationDayTime" {
        return duration_from_millis(vm, millis, MASK_TIME);
    }
    duration_from_millis_full(vm, millis)
}

/// `newDuration(long)`: years of 365 days, months of 31 days, then days and time.
fn duration_from_millis_full(vm: &mut Vm<'_>, millis: i64) -> Result<Value, VmError> {
    const DAY: u64 = 86_400_000;
    let sign = millis.signum() as i32;
    let mut total = millis.unsigned_abs();
    let years = total / (365 * DAY);
    total %= 365 * DAY;
    let months = total / (31 * DAY);
    total %= 31 * DAY;
    let days = total / DAY;
    total %= DAY;
    let hours = total / 3_600_000;
    let total = total % 3_600_000;
    let minutes = total / 60_000;
    let total = total % 60_000;
    let seconds = total / 1000;
    let millis = total % 1000;
    let parts = [
        if years == 0
            && months == 0
            && days == 0
            && hours == 0
            && minutes == 0
            && seconds == 0
            && millis == 0
        {
            0
        } else {
            sign
        },
        years as i32,
        months as i32,
        days as i32,
        hours as i32,
        minutes as i32,
        seconds as i32,
        millis as i32,
    ];
    let fraction = alloc::format!("{millis:03}");
    duration_new(vm, parts, MASK_ALL, Some(&fraction))
}

fn duration_from_millis(vm: &mut Vm<'_>, millis: i64, mask: i32) -> Result<Value, VmError> {
    let sign = millis.signum() as i32;
    let total = millis.unsigned_abs();
    let parts = [
        sign,
        0,
        0,
        (total / 86_400_000) as i32,
        ((total % 86_400_000) / 3_600_000) as i32,
        ((total % 3_600_000) / 60_000) as i32,
        ((total % 60_000) / 1000) as i32,
        (total % 1000) as i32,
    ];
    let fraction = alloc::format!("{:03}", parts[7]);
    duration_new(vm, parts, mask, Some(&fraction))
}

fn new_duration_from_components(
    vm: &mut Vm<'_>,
    ctx: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let signum = i32::from(crate::bool_arg(args, 1));
    let descriptor = &vm.classes.get(ctx.class).methods[ctx.method as usize].descriptor;
    let big = descriptor.contains("Ljava/math/BigInteger;");
    let values: Vec<i32> = if big {
        args[2..]
            .iter()
            .map(|value| int_value(vm, value.as_ref()))
            .collect()
    } else {
        args[2..].iter().map(|value| value.as_int()).collect()
    };
    let name = vm.classes.get(ctx.class).methods[ctx.method as usize]
        .name
        .clone();
    let mut parts = [signum, 0, 0, 0, 0, 0, 0, 0];
    let mut mask;
    if name == "newDurationYearMonth" {
        let months =
            values.first().copied().unwrap_or(0) * 12 + values.get(1).copied().unwrap_or(0);
        parts[1] = months / 12;
        parts[2] = months % 12;
        mask = 0;
        if parts[1] != 0 {
            mask |= MASK_YEARS;
        }
        if parts[2] != 0 {
            mask |= MASK_MONTHS;
        }
    } else if name == "newDurationDayTime" {
        parts[3] = values.first().copied().unwrap_or(0);
        parts[4] = values.get(1).copied().unwrap_or(0);
        parts[5] = values.get(2).copied().unwrap_or(0);
        parts[6] = values.get(3).copied().unwrap_or(0);
        parts[7] = values.get(4).copied().unwrap_or(0);
        mask = MASK_TIME;
        normalize_time_fields(&mut parts, None);
    } else {
        // `newDuration(boolean, int...)` and the BigInteger form list years through seconds;
        // the last value is a decimal number of seconds in the BigInteger form.
        for (index, value) in values.iter().take(6).enumerate() {
            parts[index + 1] = *value;
        }
        if big && values.len() >= 6 {
            parts[7] = 0;
        }
        mask = MASK_ALL;
    }
    if parts.iter().skip(1).all(|value| *value == 0) {
        parts[0] = 0;
    }
    let fraction = alloc::format!("{:03}", parts[7]);
    duration_new(vm, parts, mask, Some(&fraction))
}

fn int_value(vm: &mut Vm<'_>, reference: ObjectRef) -> i32 {
    if reference.is_null() {
        return 0;
    }
    let Ok((class, method)) = vm.resolve_virtual_method(reference, "intValue", "()I") else {
        return 0;
    };
    vm.invoke_method(class, method, alloc::vec![Value::Ref(reference)])
        .map(|value| value.as_int())
        .unwrap_or(0)
}

struct ParsedDuration {
    parts: [i32; 8],
    mask: i32,
    fraction: Option<String>,
}

/// `PnYnMnDTnHnMnS`, with the `-P...` negative form.
fn parse_duration(text: &str) -> Option<ParsedDuration> {
    let (sign, rest) = match text.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, text),
    };
    let rest = rest.strip_prefix('P')?;
    let mut parts = [sign, 0, 0, 0, 0, 0, 0, 0];
    let mut mask = 0;
    let mut fraction = None;
    let mut in_time = false;
    let mut number = String::new();
    let mut saw_component = false;
    for ch in rest.chars() {
        match ch {
            'T' => {
                in_time = true;
                number.clear();
            }
            '0'..='9' | '.' => number.push(ch),
            'Y' | 'M' | 'D' | 'H' | 'S' => {
                if number.is_empty() {
                    return None;
                }
                let (slot, bit) = match (in_time, ch) {
                    (false, 'Y') => (1, MASK_YEARS),
                    (false, 'M') => (2, MASK_MONTHS),
                    (false, 'D') => (3, MASK_DAYS),
                    (true, 'H') => (4, MASK_HOURS),
                    (true, 'M') => (5, MASK_MINUTES),
                    (true, 'S') => (6, MASK_SECONDS),
                    _ => return None,
                };
                if ch == 'S' {
                    let (whole, digits) = match number.split_once('.') {
                        Some((whole, digits)) => (whole, Some(digits)),
                        None => (number.as_str(), None),
                    };
                    parts[6] = whole.parse().ok()?;
                    if let Some(digits) = digits {
                        let mut padded = String::from(digits);
                        while padded.len() < 3 {
                            padded.push('0');
                        }
                        parts[7] = padded[..3].parse().ok()?;
                        fraction = Some(String::from(digits));
                    }
                } else {
                    parts[slot] = number.parse().ok()?;
                }
                mask |= bit;
                number.clear();
                saw_component = true;
            }
            _ => return None,
        }
    }
    if !saw_component {
        return None;
    }
    if parts.iter().skip(1).all(|value| *value == 0) {
        parts[0] = 0;
    }
    Some(ParsedDuration {
        parts,
        mask,
        fraction,
    })
}

// ---------------------------------------------------------------------------------------------
// XMLGregorianCalendar
// ---------------------------------------------------------------------------------------------

pub(crate) const XML_GREGORIAN_CALENDAR: oxjvm_vm::NativeClass = class(
    "javax/xml/datatype/XMLGregorianCalendar",
    Some("java/lang/Object"),
    &["java/lang/Cloneable"],
    ACC_PUBLIC | ACC_ABSTRACT,
    &CALENDAR_FIELD_DEFS,
    &CALENDAR_METHODS,
    None,
);

static CALENDAR_FIELD_DEFS: [oxjvm_vm::NativeFieldDef; 9] = [
    field("year", "I", ACC_PRIVATE, None),
    field("month", "I", ACC_PRIVATE, None),
    field("day", "I", ACC_PRIVATE, None),
    field("hour", "I", ACC_PRIVATE, None),
    field("minute", "I", ACC_PRIVATE, None),
    field("second", "I", ACC_PRIVATE, None),
    field("millisecond", "I", ACC_PRIVATE, None),
    field(
        "fractionalSecond",
        "Ljava/math/BigDecimal;",
        ACC_PRIVATE,
        None,
    ),
    field("timezone", "I", ACC_PRIVATE, None),
];

const CALENDAR_METHODS: [oxjvm_vm::NativeMethodDef; 40] = [
    method("<init>", "()V", ACC_PUBLIC, calendar_clear),
    method("clear", "()V", ACC_PUBLIC, calendar_clear),
    method("reset", "()V", ACC_PUBLIC, calendar_clear),
    method("setYear", "(I)V", ACC_PUBLIC, |vm, _, args| {
        write_int(vm, receiver(args), "year", args[1].as_int());
        void()
    }),
    method(
        "setYear",
        "(Ljava/math/BigInteger;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let year = int_value(vm, ref_arg(args, 1));
            write_int(vm, receiver(args), "year", year);
            void()
        },
    ),
    method("setMonth", "(I)V", ACC_PUBLIC, calendar_set_month),
    method("setDay", "(I)V", ACC_PUBLIC, calendar_set_day),
    method("setHour", "(I)V", ACC_PUBLIC, calendar_set_hour),
    method("setMinute", "(I)V", ACC_PUBLIC, calendar_set_minute),
    method("setSecond", "(I)V", ACC_PUBLIC, calendar_set_second),
    method("setMillisecond", "(I)V", ACC_PUBLIC, |vm, _, args| {
        write_int(vm, receiver(args), "millisecond", args[1].as_int());
        void()
    }),
    method("setTimezone", "(I)V", ACC_PUBLIC, calendar_set_timezone),
    method(
        "setFractionalSecond",
        "(Ljava/math/BigDecimal;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let value = ref_arg(args, 1);
            let class = vm.class_of(this);
            let (declaring, slot) =
                vm.find_field(class, "fractionalSecond", "Ljava/math/BigDecimal;")?;
            vm.set_instance_ref(this, declaring, slot, value);
            if !value.is_null() {
                let millis = fraction_millis(vm, value);
                write_int(vm, this, "millisecond", millis);
            }
            void()
        },
    ),
    method("setTime", "(III)V", ACC_PUBLIC, |vm, _, args| {
        set_time(
            vm,
            receiver(args),
            args[1].as_int(),
            args[2].as_int(),
            args[3].as_int(),
            None,
        )
    }),
    method("setTime", "(IIII)V", ACC_PUBLIC, |vm, _, args| {
        let millis = args[4].as_int();
        set_time_with_millis(
            vm,
            receiver(args),
            args[1].as_int(),
            args[2].as_int(),
            args[3].as_int(),
            millis,
        )
    }),
    method(
        "setTime",
        "(IIILjava/math/BigDecimal;)V",
        ACC_PUBLIC,
        |vm, _, args| {
            set_time(
                vm,
                receiver(args),
                args[1].as_int(),
                args[2].as_int(),
                args[3].as_int(),
                Some(ref_arg(args, 4)),
            )
        },
    ),
    method("getYear", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int(vm, receiver(args), "year"))
    }),
    method("getMonth", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int(vm, receiver(args), "month"))
    }),
    method("getDay", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int(vm, receiver(args), "day"))
    }),
    method("getHour", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int(vm, receiver(args), "hour"))
    }),
    method("getMinute", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int(vm, receiver(args), "minute"))
    }),
    method("getSecond", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int(vm, receiver(args), "second"))
    }),
    method("getMillisecond", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int(vm, receiver(args), "millisecond"))
    }),
    method(
        "getFractionalSecond",
        "()Ljava/math/BigDecimal;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            object(
                vm.read_ref_field(this, class, "fractionalSecond", "Ljava/math/BigDecimal;")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method("getTimezone", "()I", ACC_PUBLIC, |vm, _, args| {
        int(read_int(vm, receiver(args), "timezone"))
    }),
    method(
        "getEon",
        "()Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |_, _, _| object(ObjectRef::NULL),
    ),
    method(
        "getEonAndYear",
        "()Ljava/math/BigInteger;",
        ACC_PUBLIC,
        |vm, _, args| {
            let year = read_int(vm, receiver(args), "year");
            if year == UNDEFINED {
                return object(ObjectRef::NULL);
            }
            let class = vm.resolve_class("java/math/BigInteger")?;
            let (declaring, ctor) = vm
                .find_method(class, "<init>", "(I)V")
                .ok_or_else(|| VmError::internal("BigInteger lacks its int constructor"))?;
            let instance = vm.new_instance(class)?;
            vm.invoke_method(
                declaring,
                ctor,
                alloc::vec![Value::Ref(instance), Value::Int(year)],
            )?;
            object(instance)
        },
    ),
    method(
        "compare",
        "(Ljavax/xml/datatype/XMLGregorianCalendar;)I",
        ACC_PUBLIC,
        |vm, _, args| int(compare_calendars(vm, receiver(args), ref_arg(args, 1))),
    ),
    method(
        "normalize",
        "()Ljavax/xml/datatype/XMLGregorianCalendar;",
        ACC_PUBLIC,
        |vm, _, args| calendar_normalize(vm, receiver(args)),
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
            boolean(calendars_equal(vm, receiver(args), other))
        },
    ),
    method("hashCode", "()I", ACC_PUBLIC, |vm, _, args| {
        int(calendar_hash(vm, receiver(args)))
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(vm.intern(&calendar_to_string(vm, receiver(args)))?),
    ),
    method(
        "toXMLFormat",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(vm.intern(&calendar_to_string(vm, receiver(args)))?),
    ),
    method(
        "getXMLSchemaType",
        "()Ljavax/xml/namespace/QName;",
        ACC_PUBLIC,
        |vm, _, args| {
            let name = calendar_schema_type(vm, receiver(args));
            constant_qname(vm, name)
        },
    ),
    method("isValid", "()Z", ACC_PUBLIC, |_, _, _| boolean(true)),
    method(
        "add",
        "(Ljavax/xml/datatype/Duration;)V",
        ACC_PUBLIC,
        |_, _, _| void(),
    ),
    method(
        "toGregorianCalendar",
        "()Ljava/util/GregorianCalendar;",
        ACC_PUBLIC,
        |vm, _, args| to_gregorian_calendar(vm, receiver(args)),
    ),
    method(
        "toGregorianCalendar",
        "(Ljava/util/TimeZone;Ljava/util/Locale;Ljavax/xml/datatype/XMLGregorianCalendar;)Ljava/util/GregorianCalendar;",
        ACC_PUBLIC,
        |vm, _, args| to_gregorian_calendar(vm, receiver(args)),
    ),
    method(
        "getTimeZone",
        "(I)Ljava/util/TimeZone;",
        ACC_PUBLIC,
        |vm, _, args| calendar_time_zone(vm, args[1].as_int()),
    ),
    method(
        "clone",
        "()Ljava/lang/Object;",
        ACC_PUBLIC,
        |vm, _, args| calendar_clone(vm, receiver(args)),
    ),
];

pub(crate) const CALENDAR_NEW_INSTANCE: oxjvm_vm::NativeClass = class(
    "javax/xml/datatype/XMLGregorianCalendar$NewInstance",
    Some("javax/xml/datatype/XMLGregorianCalendar"),
    &[],
    ACC_PRIVATE | ACC_STATIC,
    &[],
    &[],
    None,
);

fn new_calendar(vm: &mut Vm<'_>) -> Result<ObjectRef, VmError> {
    let class = vm.resolve_class("javax/xml/datatype/XMLGregorianCalendar$NewInstance")?;
    let instance = vm.new_instance(class)?;
    for name in [
        "year",
        "month",
        "day",
        "hour",
        "minute",
        "second",
        "millisecond",
        "timezone",
    ] {
        write_int(vm, instance, name, UNDEFINED);
    }
    Ok(instance)
}

fn calendar_clear(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    for name in [
        "year",
        "month",
        "day",
        "hour",
        "minute",
        "second",
        "millisecond",
        "timezone",
    ] {
        write_int(vm, this, name, UNDEFINED);
    }
    let class = vm.class_of(this);
    if let Some((declaring, slot)) =
        vm.find_field_opt(class, "fractionalSecond", "Ljava/math/BigDecimal;")
    {
        vm.set_instance_ref(this, declaring, slot, ObjectRef::NULL);
    }
    void()
}

fn valid_range(value: i32, min: i32, max: i32) -> bool {
    (min..=max).contains(&value)
}

fn calendar_set_month(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let value = args[1].as_int();
    if value != UNDEFINED && !valid_range(value, 1, 12) {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!("Month value out of range: {value}")),
        ));
    }
    write_int(vm, receiver(args), "month", value);
    void()
}

fn calendar_set_day(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let value = args[1].as_int();
    if value != UNDEFINED && !valid_range(value, 1, 31) {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!("Day value out of range: {value}")),
        ));
    }
    write_int(vm, receiver(args), "day", value);
    void()
}

fn calendar_set_hour(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    set_hour_value(vm, receiver(args), args[1].as_int())
}

fn set_hour_value(vm: &mut Vm<'_>, this: ObjectRef, value: i32) -> Result<Value, VmError> {
    if value != UNDEFINED && !valid_range(value, 0, 24) {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!("Hour value out of range: {value}")),
        ));
    }
    if value == 24 {
        for name in ["minute", "second"] {
            let other = read_int(vm, this, name);
            if other != 0 {
                return Err(vm.throw_new(
                    "java/lang/IllegalArgumentException",
                    Some("Hour 24 requires zero minutes and seconds"),
                ));
            }
        }
    }
    if value == UNDEFINED {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("Hour cannot be undefined"),
        ));
    }
    if value == 24 {
        advance_calendar_day(vm, this);
        write_int(vm, this, "hour", 0);
    } else {
        write_int(vm, this, "hour", value);
    }
    void()
}

fn calendar_set_minute(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let value = args[1].as_int();
    if value != UNDEFINED && !valid_range(value, 0, 59) {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!("Minute value out of range: {value}")),
        ));
    }
    write_int(vm, receiver(args), "minute", value);
    void()
}

fn calendar_set_second(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let value = args[1].as_int();
    if value != UNDEFINED && !valid_range(value, 0, 59) {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!("Second value out of range: {value}")),
        ));
    }
    write_int(vm, receiver(args), "second", value);
    void()
}

fn calendar_set_timezone(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let value = args[1].as_int();
    if value != UNDEFINED && !valid_range(value, -14 * 60, 14 * 60) {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!("Timezone value out of range: {value}")),
        ));
    }
    write_int(vm, receiver(args), "timezone", value);
    void()
}

fn set_time(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    hour: i32,
    minute: i32,
    second: i32,
    fraction: Option<ObjectRef>,
) -> Result<Value, VmError> {
    if hour == UNDEFINED || minute == UNDEFINED || second == UNDEFINED {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("Time components must be defined"),
        ));
    }
    if !valid_range(hour, 0, 24) || !valid_range(minute, 0, 59) || !valid_range(second, 0, 59) {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("Time value out of range"),
        ));
    }
    if hour == 24 && (minute != 0 || second != 0) {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("Hour 24 requires zero minutes and seconds"),
        ));
    }
    if hour == 24 {
        advance_calendar_day(vm, this);
        write_int(vm, this, "hour", 0);
    } else {
        write_int(vm, this, "hour", hour);
    }
    write_int(vm, this, "minute", minute);
    write_int(vm, this, "second", second);
    if let Some(fraction) = fraction {
        if !fraction.is_null() {
            let class = vm.class_of(this);
            let (declaring, slot) =
                vm.find_field(class, "fractionalSecond", "Ljava/math/BigDecimal;")?;
            let millis = fraction_millis(vm, fraction);
            vm.set_instance_ref(this, declaring, slot, fraction);
            write_int(vm, this, "millisecond", millis);
        }
    }
    void()
}

fn set_time_with_millis(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    hour: i32,
    minute: i32,
    second: i32,
    millis: i32,
) -> Result<Value, VmError> {
    set_time(vm, this, hour, minute, second, None)?;
    if millis != UNDEFINED {
        if !valid_range(millis, 0, 999) {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some(&alloc::format!("Millisecond value out of range: {millis}")),
            ));
        }
        write_int(vm, this, "millisecond", millis);
    }
    void()
}

/// The integer number of milliseconds a `BigDecimal` fractional second holds.
fn fraction_millis(vm: &mut Vm<'_>, decimal: ObjectRef) -> i32 {
    let Ok((class, method)) =
        vm.resolve_virtual_method(decimal, "toPlainString", "()Ljava/lang/String;")
    else {
        return 0;
    };
    let Ok(value) = vm.invoke_method(class, method, alloc::vec![Value::Ref(decimal)]) else {
        return 0;
    };
    let text = vm.string_or_empty(value.as_ref());
    let fraction = text
        .split_once('.')
        .map(|(_, fraction)| fraction)
        .unwrap_or("");
    let mut digits = String::from(fraction);
    digits.truncate(3);
    while digits.len() < 3 {
        digits.push('0');
    }
    digits.parse().unwrap_or(0)
}

fn calendar_key(vm: &Vm<'_>, this: ObjectRef) -> (i32, i32, i32, i32, i32, i32, i32) {
    (
        read_int(vm, this, "year"),
        read_int(vm, this, "month"),
        read_int(vm, this, "day"),
        read_int(vm, this, "hour"),
        read_int(vm, this, "minute"),
        read_int(vm, this, "second"),
        read_int(vm, this, "millisecond"),
    )
}

fn calendar_timezone(vm: &Vm<'_>, this: ObjectRef) -> i32 {
    read_int(vm, this, "timezone")
}

/// Seconds since the epoch, relative to the calendar's timezone (undefined parts are zero).
fn calendar_instant(vm: &Vm<'_>, this: ObjectRef) -> i64 {
    let (year, month, day, hour, minute, second, millis) = calendar_key(vm, this);
    let year = if year == UNDEFINED { 1970 } else { year };
    let month = if month == UNDEFINED { 1 } else { month };
    let day = if day == UNDEFINED { 1 } else { day };
    let hour = if hour == UNDEFINED { 0 } else { hour };
    let minute = if minute == UNDEFINED { 0 } else { minute };
    let second = if second == UNDEFINED { 0 } else { second };
    let millis = if millis == UNDEFINED { 0 } else { millis };
    let timezone = calendar_timezone(vm, this);
    let offset = if timezone == UNDEFINED { 0 } else { timezone };
    let days = days_from_civil(year, month, day);
    days * 86_400 + i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second)
        - i64::from(offset) * 60
        + i64::from(millis) / 1000
}

fn days_from_civil(year: i32, month: i32, day: i32) -> i64 {
    let year = i64::from(year) - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = i64::from(month);
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i32, i32, i32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y = y + i64::from(m <= 2);
    (y as i32, m as i32, d as i32)
}

/// Move a fully specified date one day forward (midnight `24:00:00`).
fn advance_calendar_day(vm: &mut Vm<'_>, this: ObjectRef) {
    let (year, month, day) = (
        read_int(vm, this, "year"),
        read_int(vm, this, "month"),
        read_int(vm, this, "day"),
    );
    if year == UNDEFINED || month == UNDEFINED || day == UNDEFINED {
        return;
    }
    let (year, month, day) = civil_from_days(days_from_civil(year, month, day) + 1);
    write_int(vm, this, "year", year);
    write_int(vm, this, "month", month);
    write_int(vm, this, "day", day);
}

fn calendars_equal(vm: &Vm<'_>, a: ObjectRef, b: ObjectRef) -> bool {
    let a_timezone = calendar_timezone(vm, a);
    let b_timezone = calendar_timezone(vm, b);
    if a_timezone != UNDEFINED && b_timezone != UNDEFINED {
        return calendar_instant(vm, a) == calendar_instant(vm, b);
    }
    let (a_year, a_month, a_day, a_hour, a_minute, a_second, a_millis) = calendar_key(vm, a);
    let (b_year, b_month, b_day, b_hour, b_minute, b_second, b_millis) = calendar_key(vm, b);
    a_year == b_year
        && a_month == b_month
        && a_day == b_day
        && a_hour == b_hour
        && a_minute == b_minute
        && a_second == b_second
        && a_millis == b_millis
        && a_timezone == b_timezone
        && fraction_text(vm, a) == fraction_text(vm, b)
}

fn fraction_text(vm: &Vm<'_>, this: ObjectRef) -> Option<String> {
    let class = vm.class_of(this);
    let reference = vm
        .read_ref_field(this, class, "fractionalSecond", "Ljava/math/BigDecimal;")
        .unwrap_or(ObjectRef::NULL);
    if reference.is_null() {
        None
    } else {
        Some(vm.string_or_empty(reference))
    }
}

fn calendar_hash(vm: &Vm<'_>, this: ObjectRef) -> i32 {
    let timezone = calendar_timezone(vm, this);
    if timezone != UNDEFINED {
        // Hash the instant so equal calendars in different timezones hash alike.
        let instant = calendar_instant(vm, this);
        let mut hash = 0i32;
        for shift in [0i64, 32] {
            hash = hash
                .wrapping_mul(31)
                .wrapping_add((instant >> shift) as i32);
        }
        return hash;
    }
    let mut hash = 0i32;
    let (year, month, day, hour, minute, second, millis) = calendar_key(vm, this);
    for value in [year, month, day, hour, minute, second, millis] {
        hash = hash.wrapping_mul(31).wrapping_add(value);
    }
    hash
}

fn compare_calendars(vm: &Vm<'_>, a: ObjectRef, b: ObjectRef) -> i32 {
    if calendar_timezone(vm, a) != UNDEFINED && calendar_timezone(vm, b) != UNDEFINED {
        return calendar_instant(vm, a).cmp(&calendar_instant(vm, b)) as i32;
    }
    match calendar_key(vm, a).cmp(&calendar_key(vm, b)) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

fn calendar_normalize(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    if calendar_timezone(vm, this) == UNDEFINED {
        return calendar_clone_value(vm, this);
    }
    let instant = calendar_instant(vm, this);
    let days = instant.div_euclid(86_400);
    let seconds = instant.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let normalized = new_calendar(vm)?;
    write_int(vm, normalized, "year", year);
    write_int(vm, normalized, "month", month);
    write_int(vm, normalized, "day", day);
    write_int(vm, normalized, "hour", (seconds / 3600) as i32);
    write_int(vm, normalized, "minute", ((seconds % 3600) / 60) as i32);
    write_int(vm, normalized, "second", (seconds % 60) as i32);
    write_int(vm, normalized, "millisecond", math_millis(vm, this));
    write_int(vm, normalized, "timezone", 0);
    object(normalized)
}

fn math_millis(vm: &Vm<'_>, this: ObjectRef) -> i32 {
    read_int(vm, this, "millisecond")
}

fn calendar_clone(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    calendar_clone_value(vm, this)
}

fn calendar_clone_value(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let clone = new_calendar(vm)?;
    for name in [
        "year",
        "month",
        "day",
        "hour",
        "minute",
        "second",
        "millisecond",
        "timezone",
    ] {
        write_int(vm, clone, name, read_int(vm, this, name));
    }
    let class = vm.class_of(this);
    if let Ok((declaring, slot)) =
        vm.find_field(class, "fractionalSecond", "Ljava/math/BigDecimal;")
    {
        let fraction = vm
            .read_ref_field(this, class, "fractionalSecond", "Ljava/math/BigDecimal;")
            .unwrap_or(ObjectRef::NULL);
        vm.set_instance_ref(clone, declaring, slot, fraction);
    }
    object(clone)
}

fn calendar_schema_type(vm: &Vm<'_>, this: ObjectRef) -> &'static str {
    let (year, month, day, hour, _, _, _) = calendar_key(vm, this);
    let has_date = year != UNDEFINED;
    let has_time = hour != UNDEFINED;
    match (has_date, month != UNDEFINED, day != UNDEFINED, has_time) {
        (true, true, true, true) => "DATETIME",
        (true, true, true, false) => "DATE",
        (false, _, _, true) => "TIME",
        (true, true, false, _) => "GYEARMONTH",
        (true, false, true, _) => "GMONTHDAY",
        (true, false, false, _) => "GYEAR",
        _ => "DATETIME",
    }
}

fn calendar_to_string(vm: &Vm<'_>, this: ObjectRef) -> String {
    let (year, month, day, hour, minute, second, millis) = calendar_key(vm, this);
    let mut text = String::new();
    if year == UNDEFINED && month == UNDEFINED && day != UNDEFINED {
        text.push_str(&alloc::format!("---{day:02}"));
    } else if year == UNDEFINED && month != UNDEFINED {
        text.push_str(&alloc::format!("--{month:02}"));
        if day != UNDEFINED {
            text.push_str(&alloc::format!("-{day:02}"));
        }
    } else {
        if year != UNDEFINED {
            text.push_str(&alloc::format!("{year:04}"));
        }
        if month != UNDEFINED {
            text.push_str(&alloc::format!("-{month:02}"));
        }
        if day != UNDEFINED {
            text.push_str(&alloc::format!("-{day:02}"));
        }
    }
    if hour != UNDEFINED {
        if !text.is_empty() {
            text.push('T');
        }
        text.push_str(&alloc::format!("{hour:02}:{minute:02}:{second:02}"));
        let fraction = fraction_text(vm, this);
        if let Some(fraction) = fraction {
            let fraction = fraction
                .split_once('.')
                .map(|(_, fraction)| fraction.to_string())
                .unwrap_or_default();
            let fraction = fraction.trim_end_matches('0');
            if !fraction.is_empty() {
                text.push('.');
                text.push_str(fraction);
            }
        } else if millis != UNDEFINED && millis != 0 {
            let mut fraction = alloc::format!("{millis:03}");
            while fraction.ends_with('0') {
                fraction.pop();
            }
            text.push('.');
            text.push_str(&fraction);
        }
    }
    let timezone = calendar_timezone(vm, this);
    if timezone != UNDEFINED {
        if timezone == 0 {
            text.push('Z');
        } else {
            let minutes = timezone.unsigned_abs();
            text.push_str(&alloc::format!(
                "{}{:02}:{:02}",
                if timezone < 0 { '-' } else { '+' },
                minutes / 60,
                minutes % 60
            ));
        }
    }
    text
}

fn to_gregorian_calendar(vm: &mut Vm<'_>, this: ObjectRef) -> Result<Value, VmError> {
    let (year, month, day, hour, minute, second, _) = calendar_key(vm, this);
    let year = if year == UNDEFINED { 1970 } else { year };
    let month = if month == UNDEFINED { 1 } else { month };
    let day = if day == UNDEFINED { 1 } else { day };
    let hour = if hour == UNDEFINED { 0 } else { hour };
    let minute = if minute == UNDEFINED { 0 } else { minute };
    let second = if second == UNDEFINED { 0 } else { second };
    let class = vm.resolve_class("java/util/GregorianCalendar")?;
    let instance = vm.new_instance(class)?;
    let (declaring, ctor) = vm
        .find_method(class, "<init>", "(IIIIII)V")
        .ok_or_else(|| VmError::internal("GregorianCalendar lacks its six-argument constructor"))?;
    vm.invoke_method(
        declaring,
        ctor,
        alloc::vec![
            Value::Ref(instance),
            Value::Int(year),
            Value::Int(month - 1),
            Value::Int(day),
            Value::Int(hour),
            Value::Int(minute),
            Value::Int(second),
        ],
    )?;
    object(instance)
}

fn calendar_time_zone(vm: &mut Vm<'_>, offset: i32) -> Result<Value, VmError> {
    let minutes = offset.unsigned_abs();
    let id = alloc::format!(
        "GMT{}{:02}:{:02}",
        if offset < 0 { '-' } else { '+' },
        minutes / 60,
        minutes % 60
    );
    let class = vm.resolve_class("java/util/TimeZone")?;
    let (declaring, method) = vm
        .find_method(
            class,
            "getTimeZone",
            "(Ljava/lang/String;)Ljava/util/TimeZone;",
        )
        .ok_or_else(|| VmError::internal("TimeZone lacks getTimeZone"))?;
    let argument = vm.intern(&id)?;
    vm.invoke_method(declaring, method, alloc::vec![Value::Ref(argument)])
}

// ---------------------------------------------------------------------------------------------
// Factory: calendar construction
// ---------------------------------------------------------------------------------------------

fn new_calendar_from_string(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let text = string_arg(vm, args[1])?;
    let Some(parsed) = parse_calendar(&text) else {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some(&alloc::format!("Invalid lexical representation: {text}")),
        ));
    };
    let this = new_calendar(vm)?;
    let parsed = normalize_parsed_calendar(parsed);
    write_calendar_fields(vm, this, parsed);
    object(this)
}

struct ParsedCalendar {
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    minute: i32,
    second: i32,
    millisecond: i32,
    timezone: i32,
}

fn write_calendar_fields(vm: &mut Vm<'_>, this: ObjectRef, parsed: ParsedCalendar) {
    write_int(vm, this, "year", parsed.year);
    write_int(vm, this, "month", parsed.month);
    write_int(vm, this, "day", parsed.day);
    write_int(vm, this, "hour", parsed.hour);
    write_int(vm, this, "minute", parsed.minute);
    write_int(vm, this, "second", parsed.second);
    write_int(vm, this, "millisecond", parsed.millisecond);
    write_int(vm, this, "timezone", parsed.timezone);
}

fn new_calendar_from_ints(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let parsed = normalize_parsed_calendar(ParsedCalendar {
        year: args[1].as_int(),
        month: args[2].as_int(),
        day: args[3].as_int(),
        hour: args[4].as_int(),
        minute: args[5].as_int(),
        second: args[6].as_int(),
        millisecond: args[7].as_int(),
        timezone: args[8].as_int(),
    });
    validate_calendar(vm, &parsed)?;
    let this = new_calendar(vm)?;
    write_calendar_fields(vm, this, parsed);
    object(this)
}

fn new_calendar_from_big_integer(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let parsed = ParsedCalendar {
        year: int_value(vm, ref_arg(args, 1)),
        month: args[2].as_int(),
        day: args[3].as_int(),
        hour: args[4].as_int(),
        minute: args[5].as_int(),
        second: args[6].as_int(),
        millisecond: 0,
        timezone: args[8].as_int(),
    };
    validate_calendar(vm, &parsed)?;
    let this = new_calendar(vm)?;
    write_calendar_fields(vm, this, parsed);
    let fraction = ref_arg(args, 7);
    if !fraction.is_null() {
        if fraction_is_invalid(vm, fraction) {
            return Err(vm.throw_new(
                "java/lang/IllegalArgumentException",
                Some("Fractional second cannot be negative"),
            ));
        }
        let class = vm.class_of(this);
        let (declaring, slot) =
            vm.find_field(class, "fractionalSecond", "Ljava/math/BigDecimal;")?;
        let millis = fraction_millis(vm, fraction);
        vm.set_instance_ref(this, declaring, slot, fraction);
        write_int(vm, this, "millisecond", millis);
    }
    object(this)
}

fn fraction_is_invalid(vm: &mut Vm<'_>, fraction: ObjectRef) -> bool {
    let Ok((class, method)) =
        vm.resolve_virtual_method(fraction, "toPlainString", "()Ljava/lang/String;")
    else {
        return false;
    };
    let Ok(value) = vm.invoke_method(class, method, alloc::vec![Value::Ref(fraction)]) else {
        return false;
    };
    let text = vm.string_or_empty(value.as_ref());
    match text.parse::<f64>() {
        Ok(value) => value < 0.0 || value >= 1.0,
        Err(_) => false,
    }
}

fn new_calendar_date(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let parsed = ParsedCalendar {
        year: args[1].as_int(),
        month: args[2].as_int(),
        day: args[3].as_int(),
        hour: UNDEFINED,
        minute: UNDEFINED,
        second: UNDEFINED,
        millisecond: UNDEFINED,
        timezone: args[4].as_int(),
    };
    validate_calendar(vm, &parsed)?;
    let this = new_calendar(vm)?;
    write_calendar_fields(vm, this, parsed);
    object(this)
}

fn new_calendar_time(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let parsed = normalize_parsed_calendar(ParsedCalendar {
        year: UNDEFINED,
        month: UNDEFINED,
        day: UNDEFINED,
        hour: args[1].as_int(),
        minute: args[2].as_int(),
        second: args[3].as_int(),
        millisecond: UNDEFINED,
        timezone: args[4].as_int(),
    });
    validate_time(vm, &parsed)?;
    let this = new_calendar(vm)?;
    write_calendar_fields(vm, this, parsed);
    object(this)
}

fn new_calendar_time_fractional(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let parsed = normalize_parsed_calendar(ParsedCalendar {
        year: UNDEFINED,
        month: UNDEFINED,
        day: UNDEFINED,
        hour: args[1].as_int(),
        minute: args[2].as_int(),
        second: args[3].as_int(),
        millisecond: 0,
        timezone: args[5].as_int(),
    });
    validate_time(vm, &parsed)?;
    let this = new_calendar(vm)?;
    write_calendar_fields(vm, this, parsed);
    let fraction = ref_arg(args, 4);
    if !fraction.is_null() {
        let class = vm.class_of(this);
        let (declaring, slot) =
            vm.find_field(class, "fractionalSecond", "Ljava/math/BigDecimal;")?;
        let millis = fraction_millis(vm, fraction);
        vm.set_instance_ref(this, declaring, slot, fraction);
        write_int(vm, this, "millisecond", millis);
    }
    object(this)
}

fn new_calendar_time_millis(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let parsed = normalize_parsed_calendar(ParsedCalendar {
        year: UNDEFINED,
        month: UNDEFINED,
        day: UNDEFINED,
        hour: args[1].as_int(),
        minute: args[2].as_int(),
        second: args[3].as_int(),
        millisecond: args[4].as_int(),
        timezone: args[5].as_int(),
    });
    validate_time(vm, &parsed)?;
    let this = new_calendar(vm)?;
    write_calendar_fields(vm, this, parsed);
    object(this)
}

fn validate_calendar(vm: &mut Vm<'_>, parsed: &ParsedCalendar) -> Result<(), VmError> {
    if parsed.month != UNDEFINED && !valid_range(parsed.month, 1, 12) {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("Month value out of range"),
        ));
    }
    if parsed.day != UNDEFINED && !valid_range(parsed.day, 1, 31) {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("Day value out of range"),
        ));
    }
    validate_time(vm, parsed)
}

fn validate_time(vm: &mut Vm<'_>, parsed: &ParsedCalendar) -> Result<(), VmError> {
    if parsed.timezone != UNDEFINED && !valid_range(parsed.timezone, -14 * 60, 14 * 60) {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("Timezone value out of range"),
        ));
    }
    if parsed.hour == UNDEFINED {
        return Ok(());
    }
    if !valid_range(parsed.hour, 0, 24)
        || !valid_range(parsed.minute, 0, 59)
        || !valid_range(parsed.second, 0, 59)
    {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("Time value out of range"),
        ));
    }
    if parsed.hour == 24 && (parsed.minute != 0 || parsed.second != 0) {
        return Err(vm.throw_new(
            "java/lang/IllegalArgumentException",
            Some("Hour 24 requires zero minutes and seconds"),
        ));
    }
    Ok(())
}

fn new_calendar_from_calendar(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let calendar = ref_arg(args, 1);
    if calendar.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let (class, field_method) = vm.resolve_virtual_method(calendar, "get", "(I)I")?;
    let mut components = [0i32; 7];
    for (index, field) in [1, 2, 5, 11, 12, 13, 14].iter().enumerate() {
        components[index] = vm
            .invoke_method(
                class,
                field_method,
                alloc::vec![Value::Ref(calendar), Value::Int(*field)],
            )?
            .as_int();
    }
    let this = new_calendar(vm)?;
    write_calendar_fields(
        vm,
        this,
        ParsedCalendar {
            year: components[0],
            month: components[1] + 1,
            day: components[2],
            hour: components[3],
            minute: components[4],
            second: components[5],
            millisecond: components[6],
            timezone: UNDEFINED,
        },
    );
    object(this)
}

/// Treat `24:00:00` as midnight at the start of the next day.
fn normalize_parsed_calendar(mut parsed: ParsedCalendar) -> ParsedCalendar {
    if parsed.hour == 24 {
        parsed.hour = 0;
        if parsed.year != UNDEFINED && parsed.month != UNDEFINED && parsed.day != UNDEFINED {
            let (year, month, day) =
                civil_from_days(days_from_civil(parsed.year, parsed.month, parsed.day) + 1);
            parsed.year = year;
            parsed.month = month;
            parsed.day = day;
        }
    }
    parsed
}

fn parse_calendar(text: &str) -> Option<ParsedCalendar> {
    let mut rest = text;
    let mut timezone = UNDEFINED;
    if let Some(without_z) = rest.strip_suffix('Z') {
        rest = without_z;
        timezone = 0;
    } else if rest.len() >= 6 {
        // A trailing `+hh:mm` / `-hh:mm` is a timezone, not part of a `---DD` or `--MM-DD` form.
        let index = rest.len() - 6;
        let marker = rest.as_bytes()[index];
        let colon = rest.as_bytes()[index + 3];
        if (marker == b'+' || marker == b'-') && colon == b':' {
            let (hours, minutes) = rest[index + 1..].split_once(':')?;
            let total = hours.parse::<i32>().ok()? * 60 + minutes.parse::<i32>().ok()?;
            timezone = if marker == b'-' { -total } else { total };
            rest = &rest[..index];
        }
    }
    let (date, time) = match rest.split_once('T') {
        Some((date, time)) => (Some(date), Some(time)),
        None if rest.contains(':') => (None, Some(rest)),
        None => (Some(rest), None),
    };
    let mut parsed = ParsedCalendar {
        year: UNDEFINED,
        month: UNDEFINED,
        day: UNDEFINED,
        hour: UNDEFINED,
        minute: UNDEFINED,
        second: UNDEFINED,
        millisecond: UNDEFINED,
        timezone,
    };
    if let Some(date) = date {
        if let Some(day) = date.strip_prefix("---") {
            parsed.day = day.parse().ok()?;
        } else if let Some(rest) = date.strip_prefix("--") {
            let mut parts = rest.split('-');
            parsed.month = parts.next()?.parse().ok()?;
            if let Some(day) = parts.next() {
                parsed.day = day.parse().ok()?;
            }
        } else {
            let negative_year = date.starts_with('-');
            let date = date.trim_start_matches('-');
            let mut parts = date.split('-');
            let year = parts.next()?.parse::<i32>().ok()?;
            parsed.year = if negative_year { -year } else { year };
            if let Some(month) = parts.next() {
                parsed.month = month.parse().ok()?;
            }
            if let Some(day) = parts.next() {
                parsed.day = day.parse().ok()?;
            }
        }
    }
    if let Some(time) = time {
        let mut parts = time.split(':');
        parsed.hour = parts.next()?.parse().ok()?;
        parsed.minute = parts.next()?.parse().ok()?;
        let seconds = parts.next()?;
        let (whole, fraction) = match seconds.split_once('.') {
            Some((whole, fraction)) => (whole, Some(fraction)),
            None => (seconds, None),
        };
        parsed.second = whole.parse().ok()?;
        parsed.millisecond = 0;
        if let Some(fraction) = fraction {
            let mut digits = String::from(fraction);
            digits.truncate(3);
            while digits.len() < 3 {
                digits.push('0');
            }
            parsed.millisecond = digits.parse().ok()?;
        }
    }
    Some(parsed)
}

fn hash_code(text: &str) -> i32 {
    let mut hash = 0i32;
    for unit in text.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
    }
    hash
}
