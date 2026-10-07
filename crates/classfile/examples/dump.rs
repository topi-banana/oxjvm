//! Dump a class file's methods as hex bytecode (development aid).
use oxjvm_classfile::{AttributeData, ClassFile};
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let class = ClassFile::read(&std::fs::read(&path).unwrap()).unwrap();
    for method in &class.methods {
        let name = class.constant_pool.utf8(method.name_index).unwrap();
        let desc = class.constant_pool.utf8(method.descriptor_index).unwrap();
        if let Some(code) = method.attributes.iter().find_map(|a| match &a.data {
            AttributeData::Code(c) => Some(c),
            _ => None,
        }) {
            println!(
                "{name}{desc}: max_stack={} max_locals={}",
                code.max_stack, code.max_locals
            );
            for (i, b) in code.code.iter().enumerate() {
                if i % 16 == 0 {
                    print!("{i:4}: ");
                }
                print!("{b:02x} ");
                if i % 16 == 15 {
                    println!();
                }
            }
            println!();
        }
    }
}
