//! Access flags (JVMS 4.1, 4.5, 4.6, 4.7.6).

/// `ACC_PUBLIC`.
pub const ACC_PUBLIC: u16 = 0x0001;
/// `ACC_PRIVATE`.
pub const ACC_PRIVATE: u16 = 0x0002;
/// `ACC_PROTECTED`.
pub const ACC_PROTECTED: u16 = 0x0004;
/// `ACC_STATIC`.
pub const ACC_STATIC: u16 = 0x0008;
/// `ACC_FINAL`.
pub const ACC_FINAL: u16 = 0x0010;
/// `ACC_SUPER`.
pub const ACC_SUPER: u16 = 0x0020;
/// `ACC_SYNCHRONIZED` (methods) / `ACC_OPEN` (modules).
pub const ACC_SYNCHRONIZED: u16 = 0x0020;
/// `ACC_OPEN` (module flags).
pub const ACC_OPEN: u16 = 0x0020;
/// `ACC_TRANSITIVE` (module flags).
pub const ACC_TRANSITIVE: u16 = 0x0020;
/// `ACC_VOLATILE` (fields) / `ACC_BRIDGE` (methods).
pub const ACC_VOLATILE: u16 = 0x0040;
/// `ACC_BRIDGE` (methods).
pub const ACC_BRIDGE: u16 = 0x0040;
/// `ACC_TRANSIENT` (fields) / `ACC_VARARGS` (methods).
pub const ACC_TRANSIENT: u16 = 0x0080;
/// `ACC_VARARGS` (methods).
pub const ACC_VARARGS: u16 = 0x0080;
/// `ACC_NATIVE`.
pub const ACC_NATIVE: u16 = 0x0100;
/// `ACC_INTERFACE`.
pub const ACC_INTERFACE: u16 = 0x0200;
/// `ACC_ABSTRACT`.
pub const ACC_ABSTRACT: u16 = 0x0400;
/// `ACC_STRICT`.
pub const ACC_STRICT: u16 = 0x0800;
/// `ACC_SYNTHETIC`.
pub const ACC_SYNTHETIC: u16 = 0x1000;
/// `ACC_ANNOTATION`.
pub const ACC_ANNOTATION: u16 = 0x2000;
/// `ACC_ENUM`.
pub const ACC_ENUM: u16 = 0x4000;
/// `ACC_MODULE`.
pub const ACC_MODULE: u16 = 0x8000;

/// Render class access flags as the canonical `public final`-style words.
#[must_use]
pub fn class_flags_to_string(flags: u16) -> alloc::string::String {
    use alloc::string::String;
    let mut out = String::new();
    let mut push = |word: &str| {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    };
    if flags & ACC_PUBLIC != 0 {
        push("public");
    }
    if flags & ACC_FINAL != 0 {
        push("final");
    }
    if flags & ACC_INTERFACE != 0 {
        push("interface");
    } else if flags & ACC_ANNOTATION != 0 {
        push("@interface");
    } else if flags & ACC_ENUM != 0 {
        push("enum");
    } else if flags & ACC_MODULE != 0 {
        push("module");
    } else if flags & ACC_ABSTRACT != 0 {
        push("abstract");
    }
    out
}

/// Render method access flags.
#[must_use]
pub fn method_flags_to_string(flags: u16) -> alloc::string::String {
    use alloc::string::String;
    let mut out = String::new();
    let mut push = |word: &str| {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    };
    if flags & ACC_PUBLIC != 0 {
        push("public");
    }
    if flags & ACC_PRIVATE != 0 {
        push("private");
    }
    if flags & ACC_PROTECTED != 0 {
        push("protected");
    }
    if flags & ACC_STATIC != 0 {
        push("static");
    }
    if flags & ACC_FINAL != 0 {
        push("final");
    }
    if flags & ACC_SYNCHRONIZED != 0 {
        push("synchronized");
    }
    if flags & ACC_BRIDGE != 0 {
        push("bridge");
    }
    if flags & ACC_VARARGS != 0 {
        push("varargs");
    }
    if flags & ACC_NATIVE != 0 {
        push("native");
    }
    if flags & ACC_ABSTRACT != 0 {
        push("abstract");
    }
    if flags & ACC_STRICT != 0 {
        push("strictfp");
    }
    if flags & ACC_SYNTHETIC != 0 {
        push("synthetic");
    }
    out
}

/// Render field access flags.
#[must_use]
pub fn field_flags_to_string(flags: u16) -> alloc::string::String {
    use alloc::string::String;
    let mut out = String::new();
    let mut push = |word: &str| {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    };
    if flags & ACC_PUBLIC != 0 {
        push("public");
    }
    if flags & ACC_PRIVATE != 0 {
        push("private");
    }
    if flags & ACC_PROTECTED != 0 {
        push("protected");
    }
    if flags & ACC_STATIC != 0 {
        push("static");
    }
    if flags & ACC_FINAL != 0 {
        push("final");
    }
    if flags & ACC_VOLATILE != 0 {
        push("volatile");
    }
    if flags & ACC_TRANSIENT != 0 {
        push("transient");
    }
    if flags & ACC_ENUM != 0 {
        push("enum");
    }
    if flags & ACC_SYNTHETIC != 0 {
        push("synthetic");
    }
    out
}
