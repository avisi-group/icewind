use {
    crate::guest::models,
    alloc::{alloc::Global, format, vec::Vec},
    common::{fuzz_test::InstructionFuzzTest, ktest},
    dbt::{
        emitter::Emitter,
        register_file::RegisterFile,
        translate::translate_instruction,
        x86::{X86TranslationContext, emitter::X86Emitter},
    },
};

const TEST_DATA: &[u8] = include_bytes!("fuzz_tests.postcard");

#[ktest]
fn fuzz() {
    for test in postcard::from_bytes::<Vec<InstructionFuzzTest>>(TEST_DATA).unwrap() {
        log::trace!(
            "running fuzz test \"{:08x}\" {}",
            test.instruction,
            test.test_number
        );

        run_test(
            test.instruction,
            test.test_number,
            &test.initial_state,
            &test.post_state,
        );
    }
}

fn run_test(instruction: u32, _index: usize, initial_state: &[u64; 31], post_state: &[u64; 31]) {
    let model = models::get("aarch64").unwrap();

    let register_file = RegisterFile::init(&*model);
    let mut ctx = X86TranslationContext::new(&model, false, register_file.global_register_offset());
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write::<u8>("PSTATE_EL", 1);
    register_file.write::<u64>("CPACR_EL1_bits", 0b11u64 << 20 | 0b11 << 24);
    register_file.write::<u64>("SP_EL1", 0x400058f0);
    register_file.write::<u64>("_PC", 0x40004f9c);

    translate_instruction(
        Global,
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        instruction,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = ctx.compile(num_regs);

    initial_state
        .iter()
        .enumerate()
        .for_each(|(i, value)| register_file.write::<u64>(format!("R{i}"), *value));

    translation.execute(&register_file);

    post_state.iter().enumerate().for_each(|(i, value)| {
        let read = register_file.read::<u64>(format!("R{i}"));
        if read != *value {
            panic!("R{i} mismatch! expected {value}, got {}", read)
            //log::error!("fuzz_{instruction:08x}_{index}")
        }
    });
}
