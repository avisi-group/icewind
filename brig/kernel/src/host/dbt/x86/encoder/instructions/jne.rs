use {
    crate::host::dbt::{
        Alloc,
        x86::{
            emitter::X86Block,
            encoder::{Operand, OperandKind::Target as T},
        },
    },
    common::{arena::Ref, hashmap::HashMapA},
    iced_x86::code_asm::{CodeAssembler, CodeLabel},
};

pub fn encode<A: Alloc>(
    assembler: &mut CodeAssembler,
    label_map: &HashMapA<Ref<X86Block<A>>, CodeLabel, A>,
    tgt: &Operand<A>,
) {
    let T(target) = tgt.kind() else { panic!() };

    let label = label_map
        .get(target)
        .unwrap_or_else(|| panic!("no label for {target:?} found"))
        .clone();

    assembler.jne(label).unwrap();
}
