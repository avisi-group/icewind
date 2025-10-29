use {
    crate::guest::{
        Translation,
        models::{self, BUMP_ALLOCATOR, write_to_el},
        tracing::{
            trace_instruction_end, trace_instruction_start, trace_memory_read, trace_memory_write,
            trace_register_read, trace_register_write,
        },
    },
    alloc::{format, vec::Vec},
    common::{fuzz_test::InstructionFuzzTest, ktest},
    dbt::{
        bump_alloc::BumpAllocatorRef,
        emitter::Emitter,
        register_file::RegisterFile,
        translate::translate_instruction,
        x86::{Callbacks, X86TranslationContext, emitter::X86Emitter},
    },
};

#[ktest]
fn fuzz_scalar() {
    for test in
        postcard::from_bytes::<Vec<InstructionFuzzTest>>(include_bytes!("fuzz_scalar.postcard"))
            .unwrap()
    {
        log::trace!(
            "running scalar fuzz test \"{:08x}\" {}",
            test.instruction,
            test.test_number
        );

        run_test(
            test.instruction,
            test.test_number,
            &test.pre_gprs,
            &test.pre_fprs,
            &test.post_gprs,
            &test.post_fprs,
        );
    }
}

#[ktest]
fn fuzz_vector() {
    for test in
        postcard::from_bytes::<Vec<InstructionFuzzTest>>(include_bytes!("fuzz_vector.postcard"))
            .unwrap()
    {
        log::trace!(
            "running vector fuzz test \"{:08x}\" {}",
            test.instruction,
            test.test_number
        );

        run_test(
            test.instruction,
            test.test_number,
            &test.pre_gprs,
            &test.pre_fprs,
            &test.post_gprs,
            &test.post_fprs,
        );
    }
}

//#[ktest]
fn _fuzz_float() {
    for test in
        postcard::from_bytes::<Vec<InstructionFuzzTest>>(include_bytes!("fuzz_float.postcard"))
            .unwrap()
    {
        log::trace!(
            "running float fuzz test \"{:08x}\" {}",
            test.instruction,
            test.test_number
        );

        run_test(
            test.instruction,
            test.test_number,
            &test.pre_gprs,
            &test.pre_fprs,
            &test.post_gprs,
            &test.post_fprs,
        );
    }
}

fn run_test(
    instruction: u32,
    _index: usize,
    pre_gprs: &[u64; 32],
    pre_fprs: &[u128; 32],
    post_gprs: &[u64; 32],
    post_fprs: &[u128; 32],
) {
    let model = models::get("aarch64").unwrap();

    let register_file = RegisterFile::init(&*model);
    let mut ctx = X86TranslationContext::new_with_allocator(
        BumpAllocatorRef::new(&BUMP_ALLOCATOR),
        &model,
        false,
        register_file.global_register_offset(),
        Callbacks {
            el_changed_callback: write_to_el,
            trace_instruction_start,
            trace_instruction_end,
            trace_register_read,
            trace_register_write,
            trace_memory_read,
            trace_memory_write,
        },
    );
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write::<u8>("PSTATE_EL", 1);
    register_file.write::<u64>("CPACR_EL1_bits", 0b11u64 << 20 | 0b11 << 24);
    register_file.write::<u64>("SP_EL1", 0x400063b0);
    register_file.write::<u64>("_PC", 0x40005a5c);

    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        instruction,
        0x40005a5c,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    pre_gprs
        .iter()
        .take(31)
        .enumerate()
        .for_each(|(i, value)| register_file.write::<u64>(format!("R{i}"), *value));

    let z_offset = usize::try_from(model.reg_offset("_Z")).unwrap();

    pre_fprs
        .iter()
        .enumerate()
        .for_each(|(i, value)| register_file.write_raw(z_offset + (i * 256), *value));

    translation.execute(&register_file);

    post_gprs
        .iter()
        .take(31)
        .enumerate()
        .for_each(|(i, value)| {
            let read = register_file.read::<u64>(format!("R{i}"));
            if read != *value {
                panic!("R{i} mismatch! expected {value:#x}, got {:#x}", read)
                //log::error!("fuzz_{instruction:08x}_{index}")
            }
        });

    post_fprs.iter().enumerate().for_each(|(i, value)| {
        let read = register_file.read_raw::<u128>(z_offset + (i * 256));
        if read != *value {
            panic!("Q{i} mismatch! expected {value:#x}, got {:#x}", read)
            //log::error!("fuzz_{instruction:08x}_{index}")
        }
    });
}
