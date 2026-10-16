//! `java.beans`: the small interfaces tests link against.

use oxjvm_classfile::flags::*;

use crate::lang::abstract_method_pub;
use crate::{class, method};

pub(crate) const EXCEPTION_LISTENER: oxjvm_vm::NativeClass = class(
    "java/beans/ExceptionListener",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &[method(
        "exceptionThrown",
        "(Ljava/lang/Exception;)V",
        ACC_PUBLIC | ACC_ABSTRACT,
        abstract_method_pub,
    )],
    None,
);
