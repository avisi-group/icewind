use {
    crate::guest::{
        Nanoseconds, Translation,
        devices::arm::{a9gic::GlobalInterruptController, generic_timer::GenericTimer},
        models::{self, BUMP_ALLOCATOR, write_to_el},
    },
    alloc::{alloc::Global, boxed::Box, sync::Arc, vec::Vec},
    common::{
        bits::{bit_insert, mask},
        hashmap::HashMap,
        ktest,
        rudder::Model,
        sysreg_helpers,
    },
    core::{panic, u128},
    dbt::{
        bump_alloc::{BumpAllocator, BumpAllocatorRef},
        emitter::{Emitter, Type},
        interpret::{self, Value, interpret},
        register_file::RegisterFile,
        translate::{translate, translate_instruction},
        x86::{
            X86TranslationContext,
            emitter::{
                BinaryOperationKind, CastOperationKind, NodeKind, ShiftOperationKind,
                UnaryOperationKind, X86Emitter, X86Node,
            },
        },
    },
    kernel::timer::{GLOBAL_CLOCK, Measurement},
};

mod fuzz;

fn setup() -> (Arc<Model>, RegisterFile, X86TranslationContext) {
    let model = models::get("aarch64").unwrap();

    let register_file = RegisterFile::init(&*model);

    let ctx = X86TranslationContext::new_with_allocator(
        BumpAllocatorRef::new(&BUMP_ALLOCATOR),
        &model,
        false,
        register_file.global_register_offset(),
        write_to_el,
    );

    (model, register_file, ctx)
}

#[ktest]
fn smoke() {
    assert!(1 + 1 == 2);
}

#[ktest]
fn init_system() {
    let model = models::get("aarch64").unwrap();

    let _register_file = RegisterFile::init(&*model);
}

#[ktest]
fn static_dynamic_chaos_smoke() {
    fn run(r0_value: u64, r1_value: u64, r2_value: u64) -> (u64, u64, u64) {
        let (model, register_file, mut ctx) = setup();
        let mut emitter = X86Emitter::new(&mut ctx);

        translate(
            &*model,
            "func_corrupted_var",
            &[],
            &mut emitter,
            &register_file,
        )
        .unwrap();

        emitter.leave();
        let num_regs = emitter.next_vreg();
        let translation = Translation::new(ctx.compile(num_regs));

        register_file.write("R0", r0_value);
        register_file.write("R1", r1_value);
        register_file.write("R2", r2_value);

        translation.execute(&register_file);

        (
            register_file.read("R0"),
            register_file.read("R1"),
            register_file.read("R2"),
        )
    }

    assert_eq!(run(0, 0, 0), (0, 0, 10));
    assert_eq!(run(0, 1, 0), (0, 1, 10));
    assert_eq!(run(1, 0, 0), (1, 0, 5));
    assert_eq!(run(1, 1, 0), (1, 1, 5));
}

// #[ktest]
// fn num_of_feature_dynamic() {
//     let model = &*models::get("aarch64").unwrap();

//     let  register_file = RegisterFile::init(&*model);
//

//     let mut ctx = X86TranslationContext::new(&model, false,
// register_file.global_register_offset());     let mut emitter =
// X86Emitter::new(&mut ctx);

//     let feature = emitter.read_register(model.reg_offset("R0"),
// Type::Signed(32));

//     let out = translate(Global,
//         &*model,
//         "num_of_Feature",
//         &[feature],
//         &mut emitter,
//         &register_file,
//     )
//     .unwrap();
//     emitter.write_register(model.reg_offset("R1"), out);
//     emitter.leave();
//     let num_regs = emitter.next_vreg();

//     let translation = Translation::new(ctx.compile(num_regs));

//     unsafe {
//         let r0 = register_file_ptr.add(model.reg_offset("R0") as usize) as
// *mut i32;         let r1 = register_file_ptr.add(model.reg_offset("R1") as
// usize) as *mut i64;

//         register_file.write::<u64,_>("R0",4);
//         *r1 = 0;

//         translation.execute(&register_file);

//         assert_eq!(4, ( register_file.read::<u64,_>("R0")));
//         assert_eq!(4, (*r1));
//         //assert_eq!(0xe, (*see)); //// todo: re-implement depending on
// result         // of SEE/cacheable registers work
//     }
// }

#[ktest]
fn num_of_feature_const_123() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let feature = emitter.constant(123, Type::Signed(32));

    let out = translate(
        &*model,
        "num_of_Feature",
        &[feature],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();

    emitter.leave();

    assert_eq!(
        *out.kind(),
        NodeKind::Constant {
            value: 159,
            width: 64
        }
    );
}

#[ktest]
fn have_lse2_ext_is_const() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let out = translate(&*model, "HaveLSE2Ext", &[], &mut emitter, &register_file)
        .unwrap()
        .unwrap();

    emitter.leave();

    assert_eq!(*out.kind(), NodeKind::Constant { value: 0, width: 1 });
}

#[ktest]
fn statistical_profiling_disabled() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let is_enabled = translate(
        &*model,
        "StatisticalProfilingEnabled",
        &[],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();

    emitter.write_register(model.reg_offset("R0"), is_enabled);

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));
    translation.execute(&register_file);

    assert_eq!(0, register_file.read::<u8>("R0"))
}

// /// Disabling because we enabled all the features, but this should really be
// a const false for the sake of performance #[ktest]
// fn havebrbext_disabled() {
//     let model = models::get("aarch64").unwrap();

//     let  register_file = RegisterFile::init(&*model);
//

//     let mut ctx = X86TranslationContext::new(&model, false,
// register_file.global_register_offset());     let mut emitter =
// X86Emitter::new(&mut ctx);

//     let is_enabled =
//         translate(Global,&*model, "HaveBRBExt", &[], &mut emitter,
// register_file_ptr).unwrap();

//     emitter.write_register(model.reg_offset("R0"), is_enabled);

//     emitter.leave();
//     let num_regs = emitter.next_vreg();
//     let translation = Translation::new(ctx.compile(num_regs));
//     translation.execute(&register_file);

//     unsafe {
//         assert_eq!(
//             false,
//             *(register_file_ptr.add(model.reg_offset("R0") as usize) as *mut
// bool)         )
//     }
// }

#[ktest]
fn using_aarch32_disabled() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let is_enabled = translate(&*model, "UsingAArch32", &[], &mut emitter, &register_file)
        .unwrap()
        .unwrap();

    emitter.write_register(model.reg_offset("R0"), is_enabled);

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));
    translation.execute(&register_file);

    assert_eq!(0, register_file.read::<u8>("R0"))
}

#[ktest]
fn branchto() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let target = emitter.constant(0xDEADFEED, Type::Unsigned(64));

    translate(&*model, "BranchTo", &[target], &mut emitter, &register_file).unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    assert_eq!(0x0, register_file.read::<u64>("_PC"));

    register_file.write("__BranchTaken", false);

    translation.execute(&register_file);

    assert_eq!(0xDEADFEED, register_file.read::<u64>("_PC"));
    assert_eq!(true, register_file.read::<bool>("__BranchTaken"))
}

#[ktest]
fn decodea64_addsub() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let opcode = emitter.constant(0x8b020020, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("SEE", -1i64);
    register_file.write::<u64>("R0", 2);
    register_file.write::<u64>("R1", 5);
    register_file.write::<u64>("R2", 10);

    translation.execute(&register_file);

    assert_eq!(15, register_file.read::<u64>("R0"));
    //assert_eq!(0xe, (*see)); //// todo: re-implement depending on result
    // of SEE/cacheable registers work
}

#[ktest]
fn decodea64_addsub_interpret() {
    let model = models::get("aarch64").unwrap();

    let register_file = RegisterFile::init(&*model);

    register_file.write("SEE", -1i64);
    register_file.write::<u64>("R0", 2);
    register_file.write::<u64>("R1", 5);
    register_file.write::<u64>("R2", 10);

    let opcode = interpret::Value::UnsignedInteger {
        value: 0x8b020020,
        width: 32,
    };
    interpret(&*model, "__DecodeA64", &[opcode], &register_file);

    assert_eq!(15, register_file.read::<u64>("R0"));
    //   assert_eq!(0xe, (*see)); // todo: re-implement depending on result
    // of SEE/cacheable registers work
}

#[ktest]
fn decodea64_mov() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    //   aa0103e0        mov     x0, x1
    let opcode = emitter.constant(0xaa0103e0, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("SEE", -1i64);
    register_file.write::<u64>("R0", 2);
    register_file.write::<u64>("R1", 43);

    translation.execute(&register_file);

    //log::info!("{translation:?}");

    assert_eq!(43, register_file.read::<u64>("R0"));
    assert_eq!(43, register_file.read::<u64>("R1"));
    // assert_eq!(55, (*see));// todo: re-implement depending on result of
    // SEE/cacheable registers work
}

#[ktest]
fn decodea64_branch() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let opcode = emitter.constant(0x17fffffa, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    //  log::trace!("{translation:?}");

    register_file.write("_PC", 44u64);
    register_file.write("SEE", -1i64);

    translation.execute(&register_file);

    assert_eq!(20, register_file.read::<u64>("_PC"));
    //assert_eq!(67, (*see));// todo: re-implement depending on result of
    // SEE/cacheable registers work
}

#[ktest]
fn branch_if_eq() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let opcode = emitter.constant(0x540000c0, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("SEE", -1i64);

    translation.execute(&register_file);

    //assert_eq!(0x45, (*see)); // todo: re-implement depending on result of
    // SEE/cacheable registers work

    assert_eq!(0x0, register_file.read::<u64>("_PC"));
    assert_eq!(true, register_file.read::<bool>("__BranchTaken"));
}

#[ktest]
fn branch_uncond_imm_offset_math() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // s0: read-var imm26:u26
    let s0 = emitter.constant(0x17fffffa & mask(26u32), Type::Unsigned(26));

    // s1: const #0u : u2
    let s1 = emitter.constant(0, Type::Unsigned(2));

    // s2: cast zx s0 -> u28
    let s2 = emitter.cast(s0, Type::Unsigned(28), CastOperationKind::ZeroExtend);

    // s3: const #2u : u16
    let s3 = emitter.constant(2, Type::Unsigned(16));

    // s4: lsl s2 s3
    let s4 = emitter.shift(s2, s3, ShiftOperationKind::LogicalShiftLeft);

    // s5: or s4 s1
    let s5 = emitter.binary_operation(BinaryOperationKind::Or(s4, s1));

    // s9: cast sx s5 -> u64
    let s9 = emitter.cast(s5, Type::Unsigned(64), CastOperationKind::SignExtend);

    let NodeKind::Constant { value, width } = s9.kind() else {
        panic!()
    };
    assert_eq!(*value, 0xffffffffffffffe8);
    assert_eq!(*width, 64);
}

/// Validated with:
///
/// ```rust
/// use std::arch::asm;
/// fn main() {
///     for (x, y) in [
///         (10, 5),
///         (5, 10),
///         (0, 0),
///         (u64::MAX, u64::MAX),
///         (0x7FFF_FFFF_FFFF_FFFF, -1i64 as u64),
///         (0x7FFF_FFFF_FFFF_FFFF, 1),
///         (0x0000000000000000, 0x8000000000000000),
///         (0x8000000000000000, -1i64 as u64),
///         (-1i64 as u64, 0),
///     ] {
///         println!("{x:x} {y:x}: {:04b}", get_flags(x, y))
///     }
///     println!();
///     println!();
///     for (r0, r2) in [
///         (0xffff_ffff_ffff_ff00, 0x0fff_ffff_ffff_ffc0),
///         (0xffff_ffff_ffff_ff00, 0xffff_ffff_ffff_ffc0),
///     ] {
///         println!("{r0:x} {r2:x}: {:x?}", cmp_csel(r0, r2))
///     }
/// }
/// fn get_flags(x: u64, y: u64) -> u8 {
///     let mut nzcv: u64;
///     unsafe {
///         asm!(
///             "cmp x0, x1",
///             "mrs x2, nzcv",
///             in("x0") x,
///             in("x1") y,
///             out("x2") nzcv,
///         );
///     }
///     u8::try_from(nzcv >> 28).unwrap()
/// }
/// fn cmp_csel(r0: u64, mut r2: u64) -> (u64, u8) {
///     let mut nzcv: u64;
///     unsafe {
///         asm!(
///             "cmp x2, x0",
///             "mrs x1, nzcv",
///             "csel    x2, x2, x0, ls",
///             in("x0") r0,
///             inout("x2") r2,
///             out("x1") nzcv,
///         );
///     }
///     (r2, u8::try_from(nzcv >> 28).unwrap())
/// }
/// ```
#[ktest]
fn cmp_csel() {
    assert_eq!(
        0xffff_ffff_ffff_ff00,
        cmp_csel_inner(0xffff_ffff_ffff_ff00, 0xffff_ffff_ffff_ffc0)
    );

    assert_eq!(
        0x0fff_ffff_ffff_ffc0,
        cmp_csel_inner(0xffff_ffff_ffff_ff00, 0x0fff_ffff_ffff_ffc0)
    );

    fn cmp_csel_inner(pre_r0: u64, pre_r2: u64) -> u64 {
        let (model, register_file, mut ctx) = setup();
        let mut emitter = X86Emitter::new(&mut ctx);

        let see_value = emitter.constant(-1i32 as u64, Type::Signed(32));
        emitter.write_register(model.reg_offset("SEE"), see_value);

        // cmp     x2, x0
        let opcode = emitter.constant(0xeb00005f, Type::Unsigned(32));
        translate(
            &*model,
            "__DecodeA64",
            &[opcode],
            &mut emitter,
            &register_file,
        )
        .unwrap();

        let see_value = emitter.constant(-1i32 as u64, Type::Signed(32));
        emitter.write_register(model.reg_offset("SEE"), see_value);

        // csel    x2, x2, x0, ls  // ls = plast

        let opcode = emitter.constant(0x9a809042, Type::Unsigned(32));
        translate(
            &*model,
            "__DecodeA64",
            &[opcode],
            &mut emitter,
            &register_file,
        )
        .unwrap();

        emitter.leave();

        let num_regs = emitter.next_vreg();
        let translation = Translation::new(ctx.compile(num_regs));

        register_file.write::<u64>("R0", pre_r0);
        register_file.write::<u64>("R2", pre_r2);

        translation.execute(&register_file);

        register_file.read::<u64>("R2")
    }
}

#[ktest]
fn fibonacci_instr() {
    let model = models::get("aarch64").unwrap();

    let register_file = RegisterFile::init(&*model);

    let program = [
        // <_start>
        0xd2800000, // mov     x0, #0x0 (#0)
        0xd2800021, // mov     x1, #0x1 (#1)
        0xd2800002, // mov     x2, #0x0 (#0)
        0xd2800003, // mov     x3, #0x0 (#0)
        0xd2800144, // mov     x4, #0xa (#10)
        // <loop>
        0xeb04007f, // cmp     x3, x4
        0x540000c0, // b.eq    400104 <done>  // b.none
        0x8b010002, // add     x2, x0, x1
        0xaa0103e0, // mov     x0, x1
        0xaa0203e1, // mov     x1, x2
        0x91000463, // add     x3, x3, #0x1
        0x17fffffa, // b       4000e8 <loop>
        // <done>
        0xaa0203e0, // mov     x0, x2
        0x52800ba8, // mov     w8, #0x5d (#93)
        0xd4000001, // svc     #0x0
    ];

    // bounded just in case
    for _ in 0..100 {
        register_file.write("SEE", -1i64);
        register_file.write("__BranchTaken", false);

        let pc = register_file.read::<u64>("_PC");

        // exit before the svc
        if pc == 0x38 {
            break;
        }

        let model = models::get("aarch64").unwrap();

        let mut ctx = X86TranslationContext::new_with_allocator(
            BumpAllocatorRef::new(&BUMP_ALLOCATOR),
            &model,
            false,
            register_file.global_register_offset(),
            write_to_el,
        );
        let mut emitter = X86Emitter::new(&mut ctx);

        {
            let opcode = emitter.constant(program[pc as usize / 4], Type::Unsigned(32));
            translate(
                &*model,
                "__DecodeA64",
                &[opcode],
                &mut emitter,
                &register_file,
            )
            .unwrap();
        }

        emitter.leave();
        let num_regs = emitter.next_vreg();
        let translation = Translation::new(ctx.compile(num_regs));
        translation.execute(&register_file);

        // increment PC if no branch was taken
        if !register_file.read::<bool>("__BranchTaken") {
            register_file.write("_PC", pc + 4);
        }
    }

    assert_eq!(89, register_file.read::<u64>("R0"));
    assert_eq!(10, register_file.read::<u64>("R3"));
}

///  4000d4:	d2955fe0 	mov	x0, #0xaaff                	// #43775
///  4000d8:	d2800001 	mov	x1, #0x0                   	// #0
///  4000dc:	91500421 	add	x1, x1, #0x401, lsl #12
///  4000e0:	f9000020 	str	x0, [x1]
///  4000e4:	f9400020 	ldr	x0, [x1]
#[ktest]
fn mem() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //execute_aarch64_instrs_memory_single_general_immediate_signed_post_idx

    let opcode = emitter.constant(0xf9000020, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    // log::trace!("translation:\n{translation:?}");

    let mem = alloc::boxed::Box::new(0xdead_c0de_0000_0000u64);

    register_file.write("SEE", -1i64);
    register_file.write::<u64>("R0", 0xdeadcafe);
    register_file.write::<u64>("R1", &*mem as *const u64 as u64);

    translation.execute(&register_file);

    assert_eq!(*mem, register_file.read::<u64>("R0"));
}

#[ktest]
fn mem_store() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let opcode = emitter.constant(0xf9000020, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    const VALUE: u64 = 0xdead_c0de_0000_0000; // will be overwritten
    let mem = alloc::boxed::Box::new(0xdeadcafeu64);

    register_file.write("SEE", -1i64);
    register_file.write::<u64>("R0", VALUE);
    register_file.write::<u64>("R1", &*mem as *const u64 as u64);

    translation.execute(&register_file);

    assert_eq!(*mem, VALUE);
}

#[ktest]
fn mem_load() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    //execute_aarch64_instrs_memory_single_general_immediate_signed_post_idx

    let opcode = emitter.constant(0xf9400020, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    const VALUE: u64 = 0xdead_c0de_0000_0000;
    let mem = alloc::boxed::Box::new(VALUE);

    register_file.write("SEE", -1i64);
    register_file.write::<u64>("R0", 0xdeadcafe); // will be overwritten
    register_file.write::<u64>("R1", &*mem as *const u64 as u64);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R0"), VALUE);
}

#[ktest]
fn fibonacci_block() {
    let model = models::get("aarch64").unwrap();

    let register_file = RegisterFile::init(&*model);

    let program = [
        // <_start>
        0xd2800000, // mov     x0, #0x0 (#0)
        0xd2800021, // mov     x1, #0x1 (#1)
        0xd2800002, // mov     x2, #0x0 (#0)
        0xd2800003, // mov     x3, #0x0 (#0)
        0xd2800c84, // mov     x4, #0x64 (#100)
        // <loop>
        0xeb04007f, // cmp     x3, x4
        0x540000c0, // b.eq    400104 <done>  // b.none
        0x8b010002, // add     x2, x0, x1
        0xaa0103e0, // mov     x0, x1
        0xaa0203e1, // mov     x1, x2
        0x91000463, // add     x3, x3, #0x1
        0x17fffffa, // b       4000e8 <loop>
        // <done>
        0xaa0203e0, // mov     x0, x2
        0x52800ba8, // mov     w8, #0x5d (#93)
        0xd4000001, // svc     #0x0
    ];

    let mut blocks = HashMap::<u64, Translation>::default();

    loop {
        let pc_offset = model.reg_offset("_PC");
        let mut current_pc = register_file.read::<u64>("_PC");

        // log::trace!(
        //     "starting loop @ {current_pc}: {} {} {} {} {}",
        //     register_file.read::<u64>("R0"),
        //     register_file.read::<u64>("R1"),
        //     register_file.read::<u64>("R2"),
        //     register_file.read::<u64>("R3"),
        //     register_file.read::<u64>("R4"),
        // );

        let start_pc = current_pc;
        if let Some(translation) = blocks.get(&start_pc) {
            translation.execute(&register_file);
            continue;
        }

        if current_pc == 56 {
            break;
        }

        let mut ctx = X86TranslationContext::new_with_allocator(
            BumpAllocatorRef::new(&BUMP_ALLOCATOR),
            &model,
            false,
            register_file.global_register_offset(),
            write_to_el,
        );
        let mut emitter = X86Emitter::new(&mut ctx);

        loop {
            let _false = emitter.constant(0 as u64, Type::Unsigned(1));
            emitter.write_register(model.reg_offset("__BranchTaken"), _false);

            translate_instruction(
                &*model,
                "__DecodeA64",
                &mut emitter,
                &register_file,
                program[current_pc as usize / 4],
            )
            .unwrap();

            if emitter.ctx().get_pc_write_flag() || (current_pc == ((program.len() * 4) - 8) as u64)
            {
                break;
            } else {
                let pc = emitter.read_register(pc_offset, Type::Unsigned(64));
                let _4 = emitter.constant(4, Type::Unsigned(64));
                let pc_inc = emitter.binary_operation(BinaryOperationKind::Add(pc, _4));
                emitter.write_register(pc_offset, pc_inc);

                current_pc += 4;
            }
        }

        //log::trace!("stopped translating @ {current_pc}");

        // inc PC if branch not taken
        {
            let branch_taken =
                emitter.read_register(model.reg_offset("__BranchTaken"), Type::Unsigned(1));

            let _0 = emitter.constant(0, Type::Unsigned(64));
            let _4 = emitter.constant(4, Type::Unsigned(64));
            let addend = emitter.select(branch_taken, _0, _4);

            let pc = emitter.read_register(pc_offset, Type::Unsigned(64));
            let new_pc = emitter.binary_operation(BinaryOperationKind::Add(pc, addend));
            emitter.write_register(pc_offset, new_pc);
        }

        emitter.leave();
        let num_regs = emitter.next_vreg();
        let translation = Translation::new(ctx.compile(num_regs));

        // log::trace!("{translation:?}");

        translation.execute(&register_file);
        blocks.insert(start_pc, translation);

        log::trace!(
            "{} {}",
            register_file.read::<u64>("_PC"),
            register_file.read::<bool>("__BranchTaken")
        );
    }

    assert_eq!(
        1298777728820984005, /* technically this is fib 101, fib 100 = 3736710778780434371,
                              * but this depends whether you treat x0 or x1 as the final
                              * result */
        register_file.read::<u64>("R0")
    );
    assert_eq!(100, register_file.read::<u64>("R3"));
}

#[ktest]
fn addwithcarry_negative() {
    let (sum, flags) = add_with_carry_harness(0, -5i64 as u64, false);

    assert_eq!(sum, -5i64 as u64);
    assert_eq!(flags, 0b1000);
}

#[ktest]
fn addwithcarry_zero() {
    let (sum, flags) = add_with_carry_harness(0, 0, false);
    assert_eq!(sum, 0);
    assert_eq!(flags, 0b0100);
}

#[ktest]
fn addwithcarry_carry() {
    let (sum, flags) = add_with_carry_harness(u64::MAX, 1, false);
    assert_eq!(sum, 0);
    assert_eq!(flags, 0b0110);
}

#[ktest]
fn addwithcarry_overflow() {
    let (sum, flags) = add_with_carry_harness(u64::MAX / 2, u64::MAX / 2, false);
    assert_eq!(sum, !1);
    assert_eq!(flags, 0b1001);
}

// Testing the flags of the `0x0000000040234888:  eb01001f      cmp x0,x1`
// instruction
#[ktest]
fn addwithcarry_early_4880_loop() {
    let (sum, flags) = add_with_carry_harness(0x425a6004, !0x425a6020, false);
    assert_eq!(sum, 0xffffffffffffffe3);
    assert_eq!(flags, 0b1000);
}

#[ktest]
fn addwithcarry_linux_regression() {
    let (sum, flags) = add_with_carry_harness(0xffffffc0082b3cd0, 0xffffffffffffffd8, false);
    assert_eq!(sum, 0xffffffc0082b3ca8);
    assert_eq!(flags, 0b1010);
}

fn add_with_carry_harness(x: u64, y: u64, carry_in: bool) -> (u64, u8) {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write::<u64>("R0", x);
    register_file.write::<u64>("R1", y);
    register_file.write::<u64>("R2", carry_in as u64);

    let x = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(0x40));
    let y = emitter.read_register(model.reg_offset("R1"), Type::Unsigned(0x40));
    let carry_in = emitter.read_register(model.reg_offset("R2"), Type::Unsigned(0x1));

    let res = translate(
        &*model,
        "add_with_carry_test",
        &[x, y, carry_in],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();

    let flags = emitter.access_tuple(res.clone(), 1);
    let _0 = emitter.constant(0, Type::Signed(64));
    let _1 = emitter.constant(1, Type::Signed(64));
    let _2 = emitter.constant(1, Type::Signed(64));
    let _3 = emitter.constant(1, Type::Signed(64));

    let n = emitter.bit_extract(flags.clone(), _0.clone(), _1.clone());
    emitter.write_register(model.reg_offset("PSTATE_N"), n);
    let z = emitter.bit_extract(flags.clone(), _1.clone(), _1.clone());
    emitter.write_register(model.reg_offset("PSTATE_Z"), z);
    let c = emitter.bit_extract(flags.clone(), _2.clone(), _1.clone());
    emitter.write_register(model.reg_offset("PSTATE_C"), c);
    let v = emitter.bit_extract(flags.clone(), _3.clone(), _1.clone());
    emitter.write_register(model.reg_offset("PSTATE_V"), v);

    let sum = emitter.access_tuple(res.clone(), 0);
    emitter.write_register(model.reg_offset("R0"), sum);

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);

    (
        register_file.read::<u64>("R0"),
        register_file.read::<u8>("PSTATE_N") << 3
            | register_file.read::<u8>("PSTATE_Z") << 2
            | register_file.read::<u8>("PSTATE_C") << 1
            | register_file.read::<u8>("PSTATE_V"),
    )
}

#[ktest]
fn decodea64_cmp_first_greater() {
    let flags = decodea64_cmp_harness(10, 5);
    assert_eq!(flags, 0b0010);
}
#[ktest]
fn decodea64_cmp_second_greater() {
    let flags = decodea64_cmp_harness(5, 10);
    assert_eq!(flags, 0b1000);
}

#[ktest]
fn decodea64_cmp_zero() {
    let flags = decodea64_cmp_harness(0, 0);
    assert_eq!(flags, 0b0110);
}

#[ktest]
fn decodea64_cmp_equal() {
    let flags = decodea64_cmp_harness(u64::MAX, u64::MAX);
    assert_eq!(flags, 0b0110);
}

#[ktest]
fn decodea64_cmp_signed_overflow() {
    let flags = decodea64_cmp_harness(0x7fffffffffffffff, 0xffffffffffffffff);
    assert_eq!(flags, 0b1001);
}

#[ktest]
fn decodea64_cmp_positive_overflow() {
    let flags = decodea64_cmp_harness(0x7FFF_FFFF_FFFF_FFFF, 1);
    assert_eq!(flags, 0b0010);
}

#[ktest]
fn decodea64_cmp_negative_overflow() {
    let flags = decodea64_cmp_harness(0, 0x8000000000000000);
    assert_eq!(flags, 0b1001);
}

#[ktest]
fn decodea64_cmp_signed_underflow() {
    let flags = decodea64_cmp_harness(0x8000000000000000, u64::MAX);
    assert_eq!(flags, 0b1000);
}

#[ktest]
fn decodea64_cmp_something() {
    let flags = decodea64_cmp_harness(u64::MAX, 0);
    assert_eq!(flags, 0b1010);
}

/// verified with
/// ```rust
/// fn get_flags(x: u64, y: u64) -> u8 {
///     let mut nzcv: u64;
///     unsafe {
///         asm!(
///             "cmp x0, x1",
///             "mrs x2, nzcv",
///             in("x0") x,
///             in("x1") y,
///             out("x2") nzcv,
///         );
///     }
///     u8::try_from(nzcv >> 28).unwrap()
/// }
/// ```
fn decodea64_cmp_harness(x: u64, y: u64) -> u8 {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write::<u64>("R0", x);
    register_file.write::<u64>("R1", y);

    register_file.write("SEE", -1i64);

    // cmp    x0, x1
    let opcode = emitter.constant(0xeb01001f, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);

    register_file.read::<u8>("PSTATE_N") << 3
        | register_file.read::<u8>("PSTATE_Z") << 2
        | register_file.read::<u8>("PSTATE_C") << 1
        | register_file.read::<u8>("PSTATE_V")
}

#[ktest]
fn shiftreg() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let _1 = emitter.constant(1, Type::Signed(64));
    let shift_type = emitter.constant(1, Type::Signed(32));
    let amount = emitter.constant(0, Type::Signed(64));
    let width = emitter.constant(64, Type::Signed(64));
    let value = translate(
        &*model,
        "ShiftReg",
        &[_1, shift_type, amount, width],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();

    emitter.write_register(model.reg_offset("R0"), value);

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R0", 0);
    register_file.write::<u64>("R1", 0xdeadfeeddeadfeed);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R0"), 0xdeadfeeddeadfeed);
    assert_eq!(register_file.read::<u64>("R1"), 0xdeadfeeddeadfeed);
}

#[ktest]
fn floorpow2_constant() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let x = emitter.constant(2048, Type::Signed(64));
    let value = translate(&*model, "FloorPow2", &[x], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        value.kind(),
        &NodeKind::Constant {
            value: 2048,
            width: 64
        }
    );
    let x = emitter.constant(2397, Type::Signed(64));
    let value = translate(&*model, "FloorPow2", &[x], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        value.kind(),
        &NodeKind::Constant {
            value: 2048,
            width: 64
        }
    );
    let x = emitter.constant(4095, Type::Signed(64));
    let value = translate(&*model, "FloorPow2", &[x], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        value.kind(),
        &NodeKind::Constant {
            value: 2048,
            width: 64
        }
    );
    let x = emitter.constant(1231, Type::Signed(64));
    let value = translate(&*model, "FloorPow2", &[x], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        value.kind(),
        &NodeKind::Constant {
            value: 1024,
            width: 64
        }
    );
}

#[ktest]
fn ceilpow2_constant() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let x = emitter.constant(2048, Type::Signed(64));
    let value = translate(&*model, "CeilPow2", &[x], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        value.kind(),
        &NodeKind::Constant {
            value: 2048,
            width: 64
        }
    );
    let x = emitter.constant(2397, Type::Signed(64));
    let value = translate(&*model, "CeilPow2", &[x], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        value.kind(),
        &NodeKind::Constant {
            value: 4096,
            width: 64
        }
    );
    let x = emitter.constant(4095, Type::Signed(64));
    let value = translate(&*model, "CeilPow2", &[x], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        value.kind(),
        &NodeKind::Constant {
            value: 4096,
            width: 64
        }
    );
    let x = emitter.constant(1231, Type::Signed(64));
    let value = translate(&*model, "CeilPow2", &[x], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        value.kind(),
        &NodeKind::Constant {
            value: 2048,
            width: 64
        }
    );
}

//#[ktest]
fn _ispow2() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let x = emitter.read_register(model.reg_offset("R3"), Type::Unsigned(0x40));

    {
        let value = translate(
            &*model,
            "FloorPow2",
            &[x.clone()],
            &mut emitter,
            &register_file,
        )
        .unwrap()
        .unwrap();
        emitter.write_register(model.reg_offset("R0"), value);
    }

    {
        let value = translate(
            &*model,
            "CeilPow2",
            &[x.clone()],
            &mut emitter,
            &register_file,
        )
        .unwrap()
        .unwrap();
        emitter.write_register(model.reg_offset("R1"), value);
    }

    {
        let value = translate(&*model, "IsPow2", &[x], &mut emitter, &register_file)
            .unwrap()
            .unwrap();
        emitter.write_register(model.reg_offset("R2"), value);
    }

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));
    // log::debug!("{translation:?}");

    register_file.write::<u64>("R0", 0);
    register_file.write::<u64>("R1", 0);
    register_file.write::<u64>("R2", 0);
    register_file.write::<u64>("R3", 2048);

    translation.execute(&register_file);

    assert_eq!(
        register_file.read::<u64>("R0"),
        register_file.read::<u64>("R1")
    );
    assert_eq!(1, register_file.read::<u64>("R2"))
}

#[ktest]
fn rbitx0_interpret() {
    let model = models::get("aarch64").unwrap();

    let register_file = RegisterFile::init(&*model);

    register_file.write::<u64>("R0", 0x0123_4567_89ab_cdef);
    register_file.write("SEE", -1i64);

    // rbit x0
    let opcode = Value::UnsignedInteger {
        value: 0xdac00000,
        width: 32,
    };
    interpret(&*model, "__DecodeA64", &[opcode], &register_file);

    // assert bits are reversed
    assert_eq!(register_file.read::<u64>("R0"), 0xf7b3_d591_e6a2_c480);
}

#[ktest]
fn rbitx0() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // rbit x0

    let opcode = emitter.constant(0xdac00000, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R0", 0x0123_4567_89ab_cdef);
    register_file.write("SEE", -1i64);

    translation.execute(&register_file);

    // assert bits are reversed
    assert_eq!(register_file.read::<u64>("R0"), 0xf7b3_d591_e6a2_c480);
}

#[ktest]
fn bitinsert() {
    for (target, source, start, length) in [
        (0x0, 0xff, 0, 8),
        (0xffff_0000_ffff, 0xffff, 16, 16),
        (0xdeadfeed, 0xaaa, 13, 7),
        (0xbbbb_bbbb_bbbb_bbbb, 0xaaaa_aaaa_aaaa_aaaa, 0, 64),
    ] {
        assert_eq!(
            bit_insert(target, source, start, length),
            harness(target, source, start, length)
        );
    }

    fn harness(target: u64, source: u64, start: u64, length: u64) -> u64 {
        let (model, register_file, mut ctx) = setup();
        let mut emitter = X86Emitter::new(&mut ctx);

        {
            let target = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(64));
            let source = emitter.read_register(model.reg_offset("R1"), Type::Unsigned(64));
            let start = emitter.constant(start, Type::Signed(64));
            let length = emitter.constant(length, Type::Signed(64));

            let inserted = emitter.bit_insert(target, source, start, length);

            emitter.write_register(model.reg_offset("R2"), inserted);

            emitter.leave();
        }

        let num_regs = emitter.next_vreg();
        let translation = Translation::new(ctx.compile(num_regs));

        register_file.write::<u64>("R0", target);
        register_file.write::<u64>("R1", source);

        translation.execute(&register_file);

        register_file.read::<u64>("R2")
    }
}

#[ktest]
fn ubfx() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // ubfx x3, x3, #16, #4

    let opcode = emitter.constant(0xd3504c63, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("SEE", -1i64);
    register_file.write("R3", 0x8444_c004u64);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R3"), 0x4);
}

#[ktest]
fn highest_set_bit() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let x = emitter.constant(0b100, Type::Unsigned(64));
    let res = translate(&*model, "HighestSetBit", &[x], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        res.kind(),
        &NodeKind::Constant {
            value: 2,
            width: 64
        }
    );

    let x = emitter.constant(u64::MAX, Type::Unsigned(64));
    let res = translate(&*model, "HighestSetBit", &[x], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        res.kind(),
        &NodeKind::Constant {
            value: 63,
            width: 64
        }
    );
}

#[ktest]
fn ror() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let x = emitter.constant(0xff00, Type::Unsigned(64));
    let shift = emitter.constant(8, Type::Signed(64));
    let res = translate(&*model, "ROR", &[x, shift], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        res.kind(),
        &NodeKind::Constant {
            value: 0xff,
            width: 64
        }
    );

    let x = emitter.constant(0xff, Type::Unsigned(64));
    let shift = emitter.constant(8, Type::Signed(64));
    let res = translate(&*model, "ROR", &[x, shift], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        res.kind(),
        &NodeKind::Constant {
            value: 0xff00_0000_0000_0000,
            width: 64
        }
    );

    let x = emitter.constant(0xff, Type::Unsigned(32));
    let shift = emitter.constant(8, Type::Signed(64));
    let res = translate(&*model, "ROR", &[x, shift], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        res.kind(),
        &NodeKind::Constant {
            value: 0xff00_0000,
            width: 32
        }
    );
}

#[ktest]
fn extsv() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let m = emitter.constant(32, Type::Signed(64));
    let v = emitter.constant(0xFFFF_FFFF_FFFF_FFFF, Type::Unsigned(64));
    let res = translate(&*model, "extsv", &[m, v], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        res.kind(),
        &NodeKind::Constant {
            value: 0xFFFF_FFFF,
            width: 32
        }
    );
    let m = emitter.constant(64, Type::Signed(64));
    let v = emitter.constant(-1i32 as u64, Type::Unsigned(32));
    let res = translate(&*model, "extsv", &[m, v], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        res.kind(),
        &NodeKind::Constant {
            value: -1i64 as u64,
            width: 64
        }
    );
    let m = emitter.constant(64, Type::Signed(64));
    let v = emitter.constant(1, Type::Unsigned(1));
    let res = translate(&*model, "extsv", &[m, v], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        res.kind(),
        &NodeKind::Constant {
            value: u64::MAX,
            width: 64
        }
    );

    let m = emitter.constant(1, Type::Signed(64));
    let v = emitter.constant(1, Type::Unsigned(1));
    let res = translate(&*model, "extsv", &[m, v], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(res.kind(), &NodeKind::Constant { value: 1, width: 1 });
}

#[ktest]
fn zext_ones() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let n = emitter.constant(1, Type::Signed(64));
    let m = emitter.constant(1, Type::Signed(64));
    let res = translate(&*model, "zext_ones", &[n, m], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(res.kind(), &NodeKind::Constant { value: 1, width: 1 });

    let n = emitter.constant(64, Type::Signed(64));
    let m = emitter.constant(0, Type::Signed(64));
    let res = translate(&*model, "zext_ones", &[n, m], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        res.kind(),
        &NodeKind::Constant {
            value: 0,
            width: 64
        }
    );

    let n = emitter.constant(64, Type::Signed(64));
    let m = emitter.constant(32, Type::Signed(64));
    let res = translate(&*model, "zext_ones", &[n, m], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        res.kind(),
        &NodeKind::Constant {
            value: 0xFFFF_FFFF,
            width: 64
        }
    );

    let n = emitter.constant(64, Type::Signed(64));
    let m = emitter.constant(64, Type::Signed(64));
    let res = translate(&*model, "zext_ones", &[n, m], &mut emitter, &register_file)
        .unwrap()
        .unwrap();
    assert_eq!(
        res.kind(),
        &NodeKind::Constant {
            value: u64::MAX,
            width: 64
        }
    );
}

#[ktest]
fn decodebitmasks() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // times out:(
    // assert_eq!(
    //     interpret(
    //         &*model,
    //         "DecodeBitMasks",
    //         &[
    //             Value::UnsignedInteger {
    //                 value: 1,
    //                 length: 1,
    //             },
    //             Value::UnsignedInteger {
    //                 value: 0x13,
    //                 length: 6,
    //             },
    //             Value::UnsignedInteger {
    //                 value: 0x10,
    //                 length: 6,
    //             },
    //             Value::UnsignedInteger {
    //                 value: 0,
    //                 length: 1,
    //             },
    //             Value::SignedInteger {
    //                 value: 0x40,
    //                 length: 64,
    //             },
    //         ],
    //         &register_file,
    //     ),
    //     Value::Tuple(alloc::vec![
    //         Value::UnsignedInteger {
    //             value: 0xFFFF00000000000F,
    //             length: 64
    //         },
    //         Value::UnsignedInteger {
    //             value: 0xF,
    //             length: 64
    //         }
    //     ])
    // );

    let immn = emitter.constant(1, Type::Unsigned(1));
    let imms = emitter.constant(0x13, Type::Unsigned(6));
    let immr = emitter.constant(0x10, Type::Unsigned(6));
    let immediate = emitter.constant(0, Type::Unsigned(1));
    let m = emitter.constant(0x40, Type::Signed(64));
    let res = translate(
        &*model,
        "DecodeBitMasks",
        &[immn, imms, immr, immediate, m],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();

    assert_eq!(
        emitter.access_tuple(res.clone(), 0).kind(),
        &NodeKind::Constant {
            value: 0xFFFF00000000000F,
            width: 64
        }
    );
    assert_eq!(
        emitter.access_tuple(res, 1).kind(),
        &NodeKind::Constant {
            value: 0xF,
            width: 64
        }
    );
}

#[ktest]
fn replicate_bits_const() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    {
        let pattern = emitter.constant(0xaa, Type::Unsigned(8));
        let count = emitter.constant(2, Type::Signed(64));
        assert_eq!(
            &NodeKind::Constant {
                value: 0xaaaa,
                width: 16
            },
            emitter.bit_replicate(pattern, count).kind()
        );
    }
    {
        let pattern = emitter.constant(0x1, Type::Unsigned(1));
        let count = emitter.constant(32, Type::Signed(64));
        assert_eq!(
            &NodeKind::Constant {
                value: 0xffff_ffff,
                width: 32
            },
            emitter.bit_replicate(pattern, count).kind()
        );
    }
    {
        let pattern = emitter.constant(0xaaff, Type::Unsigned(16));
        let count = emitter.constant(4, Type::Signed(64));
        assert_eq!(
            &NodeKind::Constant {
                value: 0xaaff_aaff_aaff_aaff,
                width: 64
            },
            emitter.bit_replicate(pattern, count).kind()
        );
    }
}

#[ktest]
fn replicate_bits_dynamic() {
    fn harness(pattern: u64, pattern_width: u32, count: u64) -> u64 {
        let (model, register_file, mut ctx) = setup();
        let mut emitter = X86Emitter::new(&mut ctx);

        let count = emitter.constant(count, Type::Unsigned(16));
        let pattern_reg_read =
            emitter.read_register(model.reg_offset("R1"), Type::Unsigned(pattern_width));

        let replicated = emitter.bit_replicate(pattern_reg_read, count);
        emitter.write_register(model.reg_offset("R2"), replicated);

        emitter.leave();

        let num_regs = emitter.next_vreg();
        let translation = Translation::new(ctx.compile(num_regs));

        register_file.write("R1", pattern);

        translation.execute(&register_file);

        register_file.read("R2")
    }

    assert_eq!(0xaaaa, harness(0xaa, 8, 2));
    assert_eq!(0xffff_ffff, harness(0x1, 1, 32));
    assert_eq!(0xaaff_aaff_aaff_aaff, harness(0xaaff, 16, 4));
}

#[ktest]
fn rev_d00dfeed() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let _32 = emitter.constant(32, Type::Signed(64));
    let _3 = emitter.constant(3, Type::Signed(64));
    translate(
        &*model,
        "execute_aarch64_instrs_integer_arithmetic_rev",
        &[_32.clone(), _3.clone(), _32, _3],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("SEE", -1i64);
    register_file.write("R3", 0xedfe0dd0u64);

    translation.execute(&register_file);

    assert_eq!(0xd00dfeed, register_file.read::<u64>("R3"));
}

#[ktest]
fn place_slice() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let m = emitter.constant(64, Type::Signed(64));
    let xs = emitter.constant(0xffffffd8, Type::Unsigned(64));
    let i = emitter.constant(0, Type::Signed(64));
    let l = emitter.constant(32, Type::Signed(64));
    let shift = emitter.constant(0, Type::Signed(64));

    let res = translate(
        &*model,
        "place_slice_signed",
        &[m, xs, i, l, shift],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        res.kind(),
        &NodeKind::Constant {
            value: 0xffffffffffffffd8,
            width: 64
        }
    );
}

// #[ktest]
// fn to_real_const() {
//     let model = models::get("aarch64").unwrap();

//     let  register_file = RegisterFile::init(&*model);
//

//     let mut ctx = X86TranslationContext::new(&model, false,
// register_file.global_register_offset());     let mut emitter =
// X86Emitter::new(&mut ctx);

//     let i = emitter.constant(1, Type::Signed(64));

//     let res = translate(Global,&*model, "to_real", &[i], &mut emitter,
// register_file_ptr);

//     panic!("{res:?}")
// }

// #[ktest]
// fn to_real_dyn() {
//     let model = models::get("aarch64").unwrap();

//     let  register_file = RegisterFile::init(&*model);
//

//     let mut ctx = X86TranslationContext::new(&model, false,
// register_file.global_register_offset());     let mut emitter =
// X86Emitter::new(&mut ctx);

//     let r = emitter.read_register(0, Type::Signed(64));

//     let res = translate(Global,&*model, "to_real", &[r], &mut emitter,
// register_file_ptr);

//     panic!("{res:?}")
// }

#[ktest]
fn floor() {
    assert_eq!(0, harness(3, 4));
    assert_eq!(1, harness(5, 4));
    assert_eq!(2, harness(8, 4));

    fn harness(n: i64, d: i64) -> i64 {
        let (model, register_file, mut ctx) = setup();
        let mut emitter = X86Emitter::new(&mut ctx);

        {
            let n = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(64));
            let d = emitter.read_register(model.reg_offset("R1"), Type::Unsigned(64));

            let mut vec = Vec::new_in(emitter.ctx().allocator());
            vec.push(n);
            vec.push(d);

            let real = emitter.create_tuple(vec);
            let floor = emitter.unary_operation(UnaryOperationKind::Floor(real));
            emitter.write_register(model.reg_offset("R0"), floor);
        }
        emitter.leave();

        let num_regs = emitter.next_vreg();
        let translation = Translation::new(ctx.compile(num_regs));

        register_file.write("R0", n);
        register_file.write("R1", d);

        translation.execute(&register_file);

        register_file.read::<i64>("R0")
    }
}

// todo fix me, this test fails, because I removed the logic from the `ceil`
// to_operand implementation, but all the i/udiv/mul tests pass so idk whats
// going on
//#[ktest]
fn _ceil() {
    assert_eq!(1, harness(3, 4));
    assert_eq!(2, harness(5, 4));
    assert_eq!(2, harness(8, 4));

    fn harness(n: i64, d: i64) -> i64 {
        let (model, register_file, mut ctx) = setup();
        let mut emitter = X86Emitter::new(&mut ctx);

        {
            let n = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(64));
            let d = emitter.read_register(model.reg_offset("R1"), Type::Unsigned(64));

            let mut vec = Vec::new_in(emitter.ctx().allocator());
            vec.push(n);
            vec.push(d);

            let real = emitter.create_tuple(vec);
            let floor = emitter.unary_operation(UnaryOperationKind::Ceil(real));
            emitter.write_register(model.reg_offset("R0"), floor);
        }
        emitter.leave();

        let num_regs = emitter.next_vreg();
        let translation = Translation::new(ctx.compile(num_regs));

        register_file.write("R0", n);
        register_file.write("R1", d);

        translation.execute(&register_file);

        register_file.read::<i64>("R0")
    }
}

#[ktest]
fn msr() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //  d51be000        msr     cntfrq_el0, x0

    let opcode = emitter.constant(0xd51be000, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("SEE", -1i64);

    translation.execute(&register_file);
    // todo: test more here
}

#[ktest]
fn stp() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    //  a9bf7bfd        stp     x29, x30, [sp, #-16]!

    let opcode = emitter.constant(0xa9bf7bfd, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();
    //__DecodeA64_LoadStore
    // decode_stp_gen_aarch64_instrs_memory_pair_general_pre_idx
    // execute_aarch64_instrs_memory_pair_general_post_idx

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let dst = Box::<(u64, u64)>::new((0, 0));

    register_file.write("SEE", -1i64);
    register_file.write("R29", 0xFEEDu64);
    register_file.write("R30", 0xDEADu64);
    register_file.write("SP_EL3", (((&*dst) as *const (u64, u64)) as u64) + 16);

    translation.execute(&register_file);

    assert_eq!(*dst, (0xFEED, 0xDEAD));
}

#[ktest]
fn ldrsw() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //  b9802fe0        ldrsw   x0, [sp, #44]
    let opcode = emitter.constant(0xb9802fe0, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    // DEBUG [kernel::dbt::translate] translating "__DecodeA64_LoadStore"
    // DEBUG [kernel::dbt::translate] translating
    // "decode_ldrsw_imm_aarch64_instrs_memory_single_general_immediate_unsigned"
    // DEBUG [kernel::dbt::translate] translating
    // "execute_aarch64_instrs_memory_single_general_immediate_signed_post_idx"

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    // verified with this program:
    // let input: u64 = 0x8001_0000;
    // let input_ptr: u64 = (&input as *const u64) as u64;
    // let mut result: u64;
    // unsafe {
    //     asm!(
    //         "
    //             mov sp, {:x}
    //             ldrsw   x0, [sp, #0]
    //             mov {:x}, x0
    //         ",
    //         in(reg) input_ptr,
    //         out(reg) result
    //     )
    // }
    // println!("{result:x}");

    let src = Box::<u32>::new(0x8001_0000); // negative signed 32-bit int

    register_file.write("R0", 0xDEADu64);

    register_file.write("SP_EL3", (((&*src) as *const u32) as u64) - 44);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R0"), 0xffff_ffff_8001_0000);
}

#[ktest]
fn get_num_event_counters_accessible() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    let result = translate(
        &*model,
        "AArch64_GetNumEventCountersAccessible",
        &[],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();
    emitter.write_register(model.reg_offset("R0"), result);

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R0"), 31);
}

#[ktest]
fn sub_pc() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //  d10043ff    sub                sp, sp, #0x10

    let opcode = emitter.constant(0xd10043ff, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("SP_EL3", 0xdeadbe90);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("SP_EL3"), 0xdeadbe80);
}

#[ktest]
fn lsrv() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //  lsrv              x0, x1, x0

    let opcode = emitter.constant(0x9ac02420, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("R0", 0x3cu64);
    register_file.write("R1", 0x3u64);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R0"), 0x0);
}

#[ktest]
fn mem_load_immediate() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //  ldr                w0, 0xdc

    let opcode = emitter.constant(0x180006e0, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let mut src = Box::<u64>::new(0xBEE5BEE5);

    register_file.write("_PC", (&mut *src) as *mut u64 as u64 - 0xdc);
    register_file.write("R0", 0x0u64);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R0"), 0xBEE5BEE5);
}

#[ktest]
fn eret() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //  eret

    let opcode = emitter.constant(0xd69f03e0, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();

    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("SPSR_EL3_bits", 6);

    assert_eq!(register_file.read::<u8>("PSTATE_EL"), 3);

    register_file.write::<u64>("ELR_EL3", 0x8000_0020);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("_PC"), 0x8000_0020);
}

#[ktest]
fn clz() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //clz               x9, x9

    let opcode = emitter.constant(0xdac01129, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();

    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("R9", 0x1u64);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R9"), 63);
}

#[ktest]
fn highest_set_bit_const() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let bv = emitter.constant(0x1, Type::Unsigned(64));
    let n = translate(
        &*model,
        "HighestSetBit",
        &[bv],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        *n.kind(),
        NodeKind::Constant {
            value: 0,
            width: 64
        }
    );

    let bv = emitter.constant(0b1000, Type::Unsigned(64));
    let n = translate(
        &*model,
        "HighestSetBit",
        &[bv],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        *n.kind(),
        NodeKind::Constant {
            value: 3,
            width: 64
        }
    );

    let bv = emitter.constant(u64::MAX, Type::Unsigned(64));
    let n = translate(
        &*model,
        "HighestSetBit",
        &[bv],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        *n.kind(),
        NodeKind::Constant {
            value: 63,
            width: 64
        }
    );

    let bv = emitter.constant(u8::MAX as u64, Type::Unsigned(8));
    let n = translate(
        &*model,
        "HighestSetBit",
        &[bv],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        *n.kind(),
        NodeKind::Constant {
            value: 7,
            width: 64
        }
    );

    let bv = emitter.constant(
        0b0001_0000_0001_1010_1000_1010_1000_1010,
        Type::Unsigned(32),
    );
    let n = translate(
        &*model,
        "HighestSetBit",
        &[bv],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        *n.kind(),
        NodeKind::Constant {
            value: 28,
            width: 64
        }
    );
}

#[ktest]
fn count_leading_zero_bits_const() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let bv = emitter.constant(0x0, Type::Unsigned(64));
    let n = translate(
        &*model,
        "CountLeadingZeroBits",
        &[bv],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        *n.kind(),
        NodeKind::Constant {
            value: 64,
            width: 64
        }
    );

    let bv = emitter.constant(0b1000, Type::Unsigned(64));
    let n = translate(
        &*model,
        "CountLeadingZeroBits",
        &[bv],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        *n.kind(),
        NodeKind::Constant {
            value: 60,
            width: 64
        }
    );

    let bv = emitter.constant(u64::MAX, Type::Unsigned(64));
    let n = translate(
        &*model,
        "CountLeadingZeroBits",
        &[bv],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        *n.kind(),
        NodeKind::Constant {
            value: 0,
            width: 64
        }
    );

    let bv = emitter.constant(u8::MAX as u64, Type::Unsigned(8));
    let n = translate(
        &*model,
        "CountLeadingZeroBits",
        &[bv],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        *n.kind(),
        NodeKind::Constant {
            value: 0,
            width: 64
        }
    );

    let bv = emitter.constant(
        0b0001_0000_0001_1010_1000_1010_1000_1010,
        Type::Unsigned(32),
    );
    let n = translate(
        &*model,
        "CountLeadingZeroBits",
        &[bv],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        *n.kind(),
        NodeKind::Constant {
            value: 3,
            width: 64
        }
    );
}

#[ktest]
fn highest_set_bit_dynamic() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let r0 = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(64));
    let n = translate(
        &*model,
        "HighestSetBit",
        &[r0],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();
    emitter.write_register(model.reg_offset("R0"), n);

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R0", 0x1);

    translation.execute(&register_file);
    assert_eq!(register_file.read::<u64>("R0"), 0);
}

#[ktest]
fn msr_daifclr() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //  d50348ff        msr               daifclr, #0x8
    let opcode = emitter.constant(0xd50348ff, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("SEE", -1i64);

    translation.execute(&register_file);

    // todo: test more here
}

#[ktest]
fn mrs_cntvct_el0() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //  d53be040        mrs     x0, cntvct_el0

    let opcode = emitter.constant(0xd53be040, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("SEE", -1i64);

    translation.execute(&register_file);

    // todo: test more here
}

#[ktest]
fn current_security_state_is_const() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let state = translate(
        &*model,
        "CurrentSecurityState",
        &[],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    assert_eq!(
        *state.unwrap().kind(),
        NodeKind::Constant {
            value: 2,
            width: 32
        }
    )
}

#[ktest]
fn sys_movzx_investigation() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //  sys               #3, c7, c4, #1, x8
    // (dc      zva, x8)

    let opcode = emitter.constant(0xd50b7428, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let _translation = ctx.compile(num_regs);

    let mut dst = Box::new(0xAAu8);

    register_file.write::<u64>("R0", (&mut *dst as *mut u8) as u64);

    // memory not set up for tests
    //         panicked at kernel/src/guest/mod.rs:51:18:
    // null pointer dereference occurred
    //   translation.execute(&register_file);

    //   assert_eq!(*dst, 0x0);
}

#[ktest]
fn ttbr1_el1_write() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    let val = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(64));

    translate(
        &*model,
        "TTBR1_EL1_write",
        &[val],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R0", 0xF0F0_0000_F0F0_0000);
    register_file.write::<u64>("_TTBR1_EL1_bits", 0x0);
    translation.execute(&register_file);

    assert_eq!(
        register_file.read::<u64>("_TTBR1_EL1_bits"),
        0xF0F0_0000_F0F0_0000
    );
}

#[ktest]
fn aarch64_sysregwrite() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    let t = emitter.constant(1, Type::Signed(64));

    translate(
        &*model,
        "TTBR1_EL1_SysRegWrite_949dc27ace2a7dbe",
        &[t],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R1", 0x8224e000);
    register_file.write::<u64>("_TTBR1_EL1_bits", 0x0);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("_TTBR1_EL1_bits"), 0x8224e000);
}

#[ktest]
fn msr_ttbr() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //  msr               ttbr1_el1, x1

    let opcode = emitter.constant(0xd5182021, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R1", 0x8224e000);
    register_file.write::<u64>("_TTBR1_EL1_bits", 0x0);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("_TTBR1_EL1_bits"), 0x8224e000);
}

#[ktest]
fn branch_link_pc_flag() {
    let (model, register_file, mut ctx) = setup();
    assert!(!ctx.get_pc_write_flag());

    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //  bl         0x1134

    let opcode = emitter.constant(0x9400044d, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    assert!(ctx.get_pc_write_flag());
}

#[ktest]
fn mrs_mpidr_el1() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    assert_eq!(register_file.read::<u64>("MPIDR_EL1_bits"), 0x80000000);
    register_file.write("SEE", -1i64);

    // mrs     x5, mpidr_el1

    let opcode = emitter.constant(0xd53800a5, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R5"), 0x80000000);
}

#[ktest]
fn mov_300000() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //  mov     x4, #0x300000

    let opcode = emitter.constant(0xd2a00604, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R4"), 0x300000);
}

#[ktest]
fn mrs_ctr_el0() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //          mrs     x3, ctr_el0

    let opcode = emitter.constant(0xd53b0023, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R3"), 0x4_8444_8004);
}

#[ktest]
fn mrs_id_aa64dfr0_el1() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    assert_eq!(
        register_file.read::<u64>("ID_AA64DFR0_EL1_bits"),
        0x112101f5e1e1e91b
    );

    register_file.write("SEE", -1i64);

    // mrs               x1, id_aa64dfr0_el1

    let opcode = emitter.constant(0xd5380501, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R1"), 0x112101f5e1e1e91b);
}

// /// disabled because of failing the second assertion
// /// panicked at kernel/src/dbt/tests.rs:3294:9:
// /// assertion `left == right` failed
// /// left: 1234269928444520731
// /// right: 1373915719029297426
// ///
// /// which I got from the Sail interpreter logs from the mrs instruction
// ///
// /// but now the trace is valid even with the test failing
// ///
// /// leaving off for now but can always come back later
// ///
// #[ktest]
// fn mrs_id_aa64pfr0_el1() {
//     let model = models::get("aarch64").unwrap();

//     let  register_file = RegisterFile::init(&*model);
//
//     let mut ctx = X86TranslationContext::new(&model, false,
// register_file.global_register_offset());     let mut emitter =
// X86Emitter::new(&mut ctx);

//     unsafe {
//         let id_aa64pfr0_el1 =
//             *(register_file_ptr.add(model.reg_offset("ID_AA64PFR0_EL1_bits")
// as usize) as *mut u64);         assert_eq!(id_aa64pfr0_el1,
// 0x1311211130111112);

//         let see = register_file_ptr.add(model.reg_offset("SEE") as usize) as
// *mut i64;         register_file.write("SEE", -1i64);
//     }

//     // mrs               x1, id_aa64pfr0_el1
//
//     let opcode = emitter.constant(0xd5380501, Type::Unsigned(32));
//     translate(Global,
//         &*model,
//         "__DecodeA64",
//         &[opcode],
//         &mut emitter,
//         &register_file,
//     );

//     emitter.leave();

//     let num_regs = emitter.next_vreg();
//     let translation = Translation::new(ctx.compile(num_regs));

//     unsafe {
//         let x1 = register_file_ptr.add(model.reg_offset("R1") as usize) as
// *mut u64;

//         translation.execute(&register_file);

//         assert_eq!(*x1, 0x1311211130111112);
//     }
//}
#[ktest]
fn ldaxr() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    // ldaxr            x3, [x0]

    let opcode = emitter.constant(0xc85ffc03, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let _translation = ctx.compile(num_regs);
}

//#[ktest]
fn _slow_benchmark() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    let start = GLOBAL_CLOCK.now();

    let opcode = emitter.constant(0xa9bf7bfd, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let end = GLOBAL_CLOCK.now();

    let translation_time = end - start;

    let start = GLOBAL_CLOCK.now();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let end = GLOBAL_CLOCK.now();

    let compilation_time = end - start;

    panic!("translated in {translation_time}ns\ncompiled in {compilation_time}ns\n{translation:?}");
}

#[ktest]
fn slow_msr_2() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    let opcode = emitter.constant(0xd5181000, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let _translation = ctx.compile(num_regs);
}

#[ktest]
fn csinc() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    // csinc		w3, wzr, wzr, ne

    let opcode = emitter.constant(0x1a9f17e3, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("PSTATE_Z", 0x1u8);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R3"), 0x1);
}

#[ktest]
fn ldrh() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //   78635823        ldrh    w3, [x1, w3, uxtw #1]

    let opcode = emitter.constant(0x78635823, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let src = Box::<u32>::new(0xAAAA_DEAD); // negative signed 32-bit int

    register_file.write("R3", 0xABu64);

    register_file.write(
        "R1",
        ((&*src) as *const u32) as u64 - (register_file.read::<u64>("R3") << 1),
    );

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R3"), 0x0000_DEAD);
}

#[ktest]
fn csneg() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //  5a8307e3        csneg   w3, wzr, w3, eq // eq = none

    let opcode = emitter.constant(0x5a8307e3, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R3", 0x9);
    register_file.write("PSTATE_Z", 0u8);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R3"), 0xfffffff7);
}

#[ktest]
fn ldp() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    //  a9405400        ldp     x0, x21, [x0]
    let opcode = emitter.constant(0xa9405400, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let src = Box::<(u64, u64)>::new((0xBBBB_BBBB_BBBB_BBBB, 0xCCCC_CCCC_CCCC_CCCC));

    register_file.write("SEE", -1i64);
    register_file.write("R0", ((&*src) as *const (u64, u64)) as u64);
    register_file.write("R21", 0xAAAA_AAAA_AAAA_AAAAu64);

    translation.execute(&register_file);

    assert_eq!(
        (
            register_file.read::<u64>("R0"),
            register_file.read::<u64>("R21")
        ),
        *src
    );
}

#[ktest]
fn mem_load_32_bit() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //  ldr		w0, [x0]

    let opcode = emitter.constant(0xb9400000, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let mut src = Box::<u32>::new(0xF1F0F1F0);

    register_file.write::<u64>("R0", ((&mut *src) as *mut u32) as u64);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R0"), 0xF1F0F1F0);
}

#[ktest]
fn ccmp() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //  ccmp x5, #0x0, #0x0, eq

    let opcode = emitter.constant(0xfa4008a0, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("R5", 0x0u64);
    register_file.write::<u8>("PSTATE_N", 1);
    register_file.write::<u8>("PSTATE_Z", 0);
    register_file.write::<u8>("PSTATE_C", 0);
    register_file.write::<u8>("PSTATE_V", 0);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u8>("PSTATE_N"), 0);
    assert_eq!(register_file.read::<u8>("PSTATE_Z"), 0);
    assert_eq!(register_file.read::<u8>("PSTATE_C"), 0);
    assert_eq!(register_file.read::<u8>("PSTATE_V"), 0);
    assert_eq!(register_file.read::<u64>("R5"), 0);
}

#[ktest]
fn msr_elr_el2() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);
    // msr		elr_el2, x4

    let opcode = emitter.constant(0xd51c4024, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();

    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R4", 0x82080000);

    // uncommenting causes DBT runtime assert, commenting causes panic on line 2443

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("ELR_EL2"), 0x82080000);
}

#[ktest]
fn eret_3() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);
    register_file.write("SCR_EL3_bits", 0b1); // SCR_EL3.NS = 0

    //  eret

    let opcode = emitter.constant(0xd69f03e0, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();

    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("SPSR_EL3_bits", 0x3c9); // PSTATE.EL  = spsr<3:2>;
    assert_eq!(register_file.read::<u8>("PSTATE_EL"), 3);
    register_file.write::<u64>("ELR_EL3", 0x80000004);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("_PC"), 0x80000004);
    assert_eq!(register_file.read::<u8>("PSTATE_EL"), 2);
}

#[ktest]
fn exception_return() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SCR_EL3_bits", 0b1);

    let new_pc = emitter.constant(0x80000004, Type::Unsigned(64));
    let spsr = emitter.constant(0x3c9, Type::Unsigned(64));
    translate(
        &*model,
        "AArch64_ExceptionReturn",
        &[new_pc, spsr],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();

    let translation = Translation::new(ctx.compile(num_regs));

    assert_eq!(register_file.read::<u8>("PSTATE_EL"), 3);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("_PC"), 0x80000004);
    assert_eq!(register_file.read::<u8>("PSTATE_EL"), 2);
    //  assert_eq!(*el, 2); todo: find out why this assertion fails
}

#[ktest]
fn illegal_exception_return() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SCR_EL3_bits", 0x5b1);
    register_file.write("SCTLR_EL2_bits", 0x30c50830);
    register_file.write("CPTR_EL2_bits", 0x33ff);

    let spsr = emitter.constant(0x3c9, Type::Unsigned(64));
    let illegal_psr_state = translate(
        &*model,
        "IllegalExceptionReturn",
        &[spsr],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();

    emitter.write_register(model.reg_offset("R0"), illegal_psr_state);
    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    assert_eq!(register_file.read::<u64>("R0"), 0x0);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R0"), 0x0);
}

#[ktest]
fn el_from_spsr() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SCR_EL3_bits", 0x5b1);

    let spsr = emitter.constant(0b1111001001, Type::Unsigned(64));
    let valid_target_tuple =
        translate(&*model, "ELFromSPSR", &[spsr], &mut emitter, &register_file)
            .unwrap()
            .unwrap();

    let valid = emitter.access_tuple(valid_target_tuple.clone(), 0);
    let target = emitter.access_tuple(valid_target_tuple, 1);
    emitter.write_register(model.reg_offset("R0"), valid);
    emitter.write_register(model.reg_offset("R1"), target);
    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    assert_eq!(register_file.read::<u64>("R0"), 0x0);
    register_file.write("SCTLR_EL2_bits", 0x30c50830);
    register_file.write("CPTR_EL2_bits", 0x33ff);

    translation.execute(&register_file);

    // valid = true
    assert_eq!(register_file.read::<u64>("R0"), 0x1);

    // EL should be 2 afterwards
    assert_eq!(register_file.read::<u64>("R1"), 0x2);
}

#[ktest]
fn el_state_using_aarch32k() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let el = emitter.constant(1, Type::Unsigned(2));

    let secure = emitter.node(X86Node {
        typ: Type::Unsigned(1),
        kind: NodeKind::BinaryOperation(BinaryOperationKind::CompareEqual(
            emitter.node(X86Node {
                typ: Type::Unsigned(1),
                kind: NodeKind::Cast {
                    value: emitter.node(X86Node {
                        typ: Type::Unsigned(1),
                        kind: NodeKind::BinaryOperation(BinaryOperationKind::And(
                            emitter.node(X86Node {
                                typ: Type::Unsigned(1),
                                kind: NodeKind::Cast {
                                    value: emitter.node(X86Node {
                                        typ: Type::Unsigned(64),
                                        kind: NodeKind::Shift {
                                            value: emitter.node(X86Node {
                                                typ: Type::Unsigned(64),
                                                kind: NodeKind::GuestRegister { offset: 7696 },
                                            }),
                                            amount: emitter.node(X86Node {
                                                typ: Type::Signed(64),
                                                kind: NodeKind::Constant {
                                                    value: 0,
                                                    width: 64,
                                                },
                                            }),
                                            kind: ShiftOperationKind::LogicalShiftRight,
                                        },
                                    }),
                                    kind: CastOperationKind::Truncate,
                                },
                            }),
                            emitter.node(X86Node {
                                typ: Type::Unsigned(1),
                                kind: NodeKind::Constant { value: 1, width: 1 },
                            }),
                        )),
                    }),
                    kind: CastOperationKind::Truncate,
                },
            }),
            emitter.node(X86Node {
                typ: Type::Unsigned(1),
                kind: NodeKind::Constant { value: 0, width: 1 },
            }),
        )),
    });
    let known_aarch32_tuple = translate(
        &*model,
        "ELStateUsingAArch32K",
        &[el, secure],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();

    let known = emitter.access_tuple(known_aarch32_tuple.clone(), 0);
    let aarch32 = emitter.access_tuple(known_aarch32_tuple, 1);
    emitter.write_register(model.reg_offset("R0"), known);
    emitter.write_register(model.reg_offset("R1"), aarch32);
    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R0"), 1);
    assert_eq!(register_file.read::<u64>("R1"), 0);
}

#[ktest]
fn el_state_using_aarch32k_dynamic() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let target = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(2));

    let tuple = translate(
        &*model,
        "ELUsingAArch32K",
        &[target],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();

    let known = emitter.access_tuple(tuple.clone(), 0);
    emitter.write_register(model.reg_offset("R1"), known);

    let aarch32 = emitter.access_tuple(tuple, 1);
    emitter.write_register(model.reg_offset("R2"), aarch32);

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    // EL2 target?
    register_file.write::<u64>("R0", 2);

    translation.execute(&register_file);

    // known
    assert_eq!(register_file.read::<u64>("R1"), 1);
    // target_el_is_aarch32
    assert_eq!(register_file.read::<u64>("R2"), 0);
}

#[ktest]
fn have_aarch64() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let have_aarch64 = translate(&*model, "HaveAArch64", &[], &mut emitter, &register_file)
        .unwrap()
        .unwrap();

    assert_eq!(
        *have_aarch64.kind(),
        NodeKind::Constant { value: 1, width: 1 }
    )
}

#[ktest]
fn xpaclri() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // nop
    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0xd503201f,
    )
    .unwrap();

    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0xd50320ff,
    )
    .unwrap();

    // nop
    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0xd503201f,
    )
    .unwrap();

    // nop
    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0xd503201f,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);
}

// todo: enable me
// #[ktest]
fn _brk() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    // brk              #0x800

    let opcode = emitter.constant(0xd4210000, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);
}

#[ktest]
fn lsr() {
    //d360fd08        lsr     x8, x8, #32
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //   d360fd08        lsr     x8, x8, #32

    let opcode = emitter.constant(0xd360fd08, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("R8", 0x1);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R8"), 0x1 >> 32);
}

// #[ktest]
// fn br_btype() {
//     let model = models::get("aarch64").unwrap();

//     let register_file = RegisterFile::init(&*model);

//     let mut ctx = X86TranslationContext::new(&model, false,
// register_file.global_register_offset());     let mut emitter =
// X86Emitter::new(&mut ctx);

//     register_file.write("SEE", -1i64);

//     //   0xd61f0100  br                x8
//     translate_instruction(
//
//         &*model,
//         "__DecodeA64",
//         &mut emitter,
//         &register_file,
//         0,
//         0xd61f0100,
//     )
//     .unwrap();

//     emitter.leave();

//     let num_regs = emitter.next_vreg();
//     let translation = Translation::new(ctx.compile(num_regs));

//     register_file.write::<u64>("R8", 0xffffffc008250254);
//   //  assert_eq!(register_file.read::<u8>("BTypeNext"), 0x0);

//     translation.execute(&register_file);

//     assert_eq!(register_file.read::<u64>("_PC"), 0xffffffc008250254);
//     // assert_eq!(register_file.read::<u8>("BTypeNext"), 0x1);
//     // assert_eq!(register_file.read::<u8>("PSTATE_BTYPE"), 0x1);
// }

// #[ktest]
// fn bl_btype() {
//     let model = models::get("aarch64").unwrap();

//     let register_file = RegisterFile::init(&*model);

//     let mut ctx = X86TranslationContext::new(&model, false,
// register_file.global_register_offset());     let mut emitter =
// X86Emitter::new(&mut ctx);

//     register_file.write("SEE", -1i64);

//     //   0x97ffffdf bl      0xffff_ffff_ffff_ff7c
//     translate_instruction(
//
//         &*model,
//         "__DecodeA64",
//         &mut emitter,
//         &register_file,
//         0x1000,
//         0x97ffffdf,
//     )
//     .unwrap();

//     emitter.leave();

//     let num_regs = emitter.next_vreg();
//     let translation = Translation::new(ctx.compile(num_regs));

//     register_file.write::<u64>("_PC", 0x1000);
//     register_file.write::<u8>("BTypeNext", 0x3);
//     register_file.write::<u8>("PSTATE_BTYPE", 0x3);

//     translation.execute(&register_file);

//     assert_eq!(register_file.read::<u64>("_PC"), 0x1000 - 132); // jumping
// back 132     assert_eq!(register_file.read::<u64>("R30"), 0x1000 + 4); //
// next instruction     // assert_eq!(register_file.read::<u8>("PSTATE_BTYPE"),
// 0x0); // todo     // assert_eq!(register_file.read::<u8>("BTypeNext"), 0x0);
// }

// #[ktest]
// fn mrs_btype() {
//     let model = models::get("aarch64").unwrap();

//     let register_file = RegisterFile::init(&*model);

//     let mut ctx = X86TranslationContext::new(&model, false,
// register_file.global_register_offset());     let mut emitter =
// X86Emitter::new(&mut ctx);

//     register_file.write("SEE", -1i64);

//     //   d538d080        mrs     x0, tpidr_el1
//     translate_instruction(
//
//         &*model,
//         "__DecodeA64",
//         &mut emitter,
//         &register_file,
//         0,
//         0xd538d080,
//     )
//     .unwrap();

//     emitter.leave();

//     let num_regs = emitter.next_vreg();
//     let translation = Translation::new(ctx.compile(num_regs));

//     register_file.write::<u8>("BTypeNext", 0x3);
//     register_file.write::<u8>("PSTATE_BTYPE", 0x3);

//     translation.execute(&register_file);

//     // assert_eq!(register_file.read::<u8>("PSTATE_BTYPE"), 0x0);// todo
// }

#[ktest]
fn udf() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    //   00000115        udf     #277
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        00000115,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);
}

#[ktest]
fn eret_post_exception() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write::<u8>("PSTATE_EL", 0x1);

    //   0xd69f03e0        eret
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0xd69f03e0,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("ELR_EL1", 0x8225_0008);
    register_file.write::<u64>("SPSR_EL1_bits", 0x3c5);
    register_file.write::<u64>("SPSR_EL2_bits", 0x3c5);
    register_file.write::<u64>("SPSR_EL3_bits", 0x3c9);
    register_file.write::<u64>("_PC", 0x82205034);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("_PC"), 0x8225_0008); // todo
}

#[ktest]
fn check_eret_trap() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    let pac = emitter.constant(0, Type::Unsigned(1));
    let use_key_a = emitter.constant(1, Type::Unsigned(1));
    let res = translate(
        &*model,
        "AArch64_CheckForERetTrap",
        &[pac, use_key_a],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    assert!(res.is_none())
}

#[ktest]
fn exceptionreturn_post_exception() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);
    register_file.write::<u8>("have_exception", 0);

    let new_pc = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(64));
    let spsr = emitter.read_register(model.reg_offset("R1"), Type::Unsigned(64));

    //
    translate(
        &*model,
        "AArch64_ExceptionReturn",
        &[new_pc, spsr],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R0", 0x8225_0008);
    register_file.write::<u64>("R1", 0x3c5);

    register_file.write::<u64>("ELR_EL1", 0x8225_0008);
    register_file.write::<u8>("PSTATE_EL", 0x1);
    register_file.write::<u64>("SPSR_EL1_bits", 0x3c5);
    register_file.write::<u64>("SPSR_EL2_bits", 0x3c5);
    register_file.write::<u64>("SPSR_EL3_bits", 0x3c9);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("_PC"), 0x8225_0008);
}

#[ktest]
fn leave_with_cache() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let aaaa = emitter.constant(0xAAAA, Type::Unsigned(64));
    emitter.write_register(model.reg_offset("_PC"), aaaa);

    emitter.leave_with_cache(0x1234);

    let num_regs = emitter.next_vreg();
    let _translation = ctx.compile(num_regs);
}

#[ktest]
fn end_cycle() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    translate(&*model, "__EndCycle", &[], &mut emitter, &register_file).unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let _translation = ctx.compile(num_regs);
}

//#[ktest]
fn _decodea64_profiling() {
    let model = models::get("aarch64").unwrap();

    let mut measure = Measurement::start();

    let allocator_ref = BumpAllocatorRef::new(&BUMP_ALLOCATOR);

    let register_file = RegisterFile::init(&*model);

    let mut ctx = X86TranslationContext::new_with_allocator(
        allocator_ref,
        &model,
        false,
        register_file.global_register_offset(),
        write_to_el,
    );
    let mut emitter = X86Emitter::new(&mut ctx);

    measure.trigger("init");

    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0xd51be000,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();

    measure.trigger("translation");

    let translation = Translation::new(ctx.compile(num_regs));

    measure.trigger("compilation");

    register_file.write("SEE", -1i64);
    register_file.write::<u64>("R0", 2);
    register_file.write::<u64>("R1", 43);

    translation.execute(&register_file);

    measure.trigger("execution");

    //log::info!("{translation:?}");

    // assert_eq!(55, (*see));// todo: re-implement depending on result of
    // SEE/cacheable registers work
}

//#[ktest]
fn _branch_profiling() {
    let model = models::get("aarch64").unwrap();

    let mut measure = Measurement::start();

    let allocator_ref = BumpAllocatorRef::new(&BUMP_ALLOCATOR);

    let register_file = RegisterFile::init(&*model);

    let mut ctx = X86TranslationContext::new_with_allocator(
        allocator_ref,
        &model,
        false,
        register_file.global_register_offset(),
        write_to_el,
    );
    let mut emitter = X86Emitter::new(&mut ctx);

    measure.trigger("init");

    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0xd61f0100,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();

    measure.trigger("translation");

    let translation = Translation::new(ctx.compile(num_regs));

    measure.trigger("compilation");

    register_file.write("SEE", -1i64);
    register_file.write::<u64>("R0", 2);
    register_file.write::<u64>("R1", 43);

    translation.execute(&register_file);

    measure.trigger("execution");
}

#[ktest]
fn cond_branch() {
    let model = models::get("aarch64").unwrap();

    let allocator_ref = BumpAllocatorRef::new(&BUMP_ALLOCATOR);

    let register_file = RegisterFile::init(&*model);

    let mut ctx = X86TranslationContext::new_with_allocator(
        allocator_ref,
        &model,
        false,
        register_file.global_register_offset(),
        write_to_el,
    );
    let mut emitter = X86Emitter::new(&mut ctx);

    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x54000020,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    //panic!("sausage");
}

// todo: fix me, this broke when we changed how timers work
//#[ktest]
fn _mrs_timer() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let gic = Arc::new(GlobalInterruptController::new());
    let timer = Arc::new(GenericTimer::new(gic, 27, Nanoseconds::new(1_000)));

    sysreg_helpers::register_device(0x1be040, timer);

    assert_eq!(register_file.read::<u64>("MPIDR_EL1_bits"), 0x80000000);
    register_file.write("SEE", -1i64);

    // mrs     x0, cntvct_el0
    let opcode = emitter.constant(0xd53be040, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R0"), 0x1234);
}

#[ktest]
fn empty() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    emitter.prologue();
    emitter.leave();

    let num_regs = emitter.next_vreg();
    let _translation = ctx.compile(num_regs);
}

#[ktest]
fn create_gpr_access_desc() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let memop = emitter.constant(1, Type::Signed(32));
    let nontemporal = emitter.constant(0, Type::Unsigned(1));
    let privileged = emitter.constant(0, Type::Unsigned(1));
    let tagchecked = emitter.constant(0, Type::Unsigned(1));

    let start = GLOBAL_CLOCK.now();

    let _out = translate(
        &*model,
        "CreateAccDescGPR",
        &[memop, nontemporal, privileged, tagchecked],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();
    emitter.leave();

    let end = GLOBAL_CLOCK.now();

    let _translation_time = end - start;

    //panic!("{translation_time}ns");
}

#[ktest]
fn stp_mem_init() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // a901fc1f        stp     xzr, xzr, [x0, #24]
    let opcode = emitter.constant(0xa901fc1f, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();
    //__DecodeA64_LoadStore
    // decode_stp_gen_aarch64_instrs_memory_pair_general_pre_idx
    // execute_aarch64_instrs_memory_pair_general_post_idx

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let dst = Box::<(u64, u64)>::new((0xFEED, 0xDEAD));

    register_file.write("SEE", -1i64);
    register_file.write("R0", (((&*dst) as *const (u64, u64)) as u64) - 24);

    translation.execute(&register_file);

    assert_eq!(*dst, (0, 0));
}

#[ktest]
fn sbfm() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("PSTATE_EL", 3u8);

    // 93407c63        sxtw    x3, w3
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x93407c63,
    )
    .unwrap();
    //__DecodeA64_LoadStore
    // decode_stp_gen_aarch64_instrs_memory_pair_general_pre_idx
    // execute_aarch64_instrs_memory_pair_general_post_idx

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("R3", (-12i32) as u64);
    translation.execute(&register_file);
    assert_eq!(-12i64, register_file.read::<i64>("R3"));

    register_file.write("R3", i32::MAX as u64);
    translation.execute(&register_file);
    assert_eq!(i64::from(i32::MAX), register_file.read::<i64>("R3"));
}

#[ktest]
fn umulh() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // 9bc17c02     umulh   x2, x0, x1
    // decode_umulh_aarch64_instrs_integer_arithmetic_mul_widening_64_128hi
    // execute_aarch64_instrs_integer_arithmetic_mul_widening_64_128hi
    let opcode = emitter.constant(0x9bc17c02, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("R2", 0x0);
    register_file.write("R1", 8u64);
    register_file.write("R0", 4u64);
    translation.execute(&register_file);
    assert_eq!(register_file.read::<u64>("R2"), 0);

    register_file.write("R2", 0x0);
    register_file.write("R1", u64::MAX);
    register_file.write("R0", 4);
    translation.execute(&register_file);
    assert_eq!(register_file.read::<u64>("R2"), 0b11);

    // todo: actually fix this for unsigned integers

    // assert_eq!(*dst, (0xFEED, 0xDEAD));
}

#[ktest]
fn eor() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    //ca010042        eor     x2, x2, x1
    let opcode = emitter.constant(0xca010042, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("R2", 0x0000_FFFF_0000_FFFFu64);
    register_file.write("R1", 0x0F0F_0F0F_0F0F_0F0Fu64);
    translation.execute(&register_file);
    assert_eq!(register_file.read::<u64>("R2"), 0x0F0F_F0F0_0F0F_F0F0);
}

/// Validated with
///
/// ```rust
/// use std::arch::asm;
/// fn main() {
///     let a = 0x1122_3344_5566_7788u64;
///     let b = 0x9900_AABB_CCDD_EEFFu64;
///
///     println!("0: {:16x}", wrapper::<0>(a, b));
///     println!("1: {:16x}", wrapper::<1>(a, b));
///     println!("16: {:16x}", wrapper::<16>(a, b));
///     println!("32: {:16x}", wrapper::<32>(a, b));
///     println!("48: {:16x}", wrapper::<48>(a, b));
///     println!("63: {:16x}", wrapper::<63>(a, b));
/// }
///
/// fn wrapper<const SHIFT: usize>(mut a: u64, b: u64) -> u64 {
///     unsafe {
///         asm!(
///             "extr x2, x3, x2, #{shift}",
///             shift = const SHIFT,
///             in("x3") b,
///             inout("x2") a,
///         );
///
///         a
///     }
/// }
/// ```
#[ktest]
fn extr() {
    fn wrapper(opcode: u32) -> u64 {
        let (model, register_file, mut ctx) = setup();
        let mut emitter = X86Emitter::new(&mut ctx);

        // 93c28062        extr    x2, x3, x2, #32
        // execute_aarch64_instrs_integer_ins_ext_extract_immediate
        let opcode = emitter.constant(u64::from(opcode), Type::Unsigned(32));
        translate(
            &*model,
            "__DecodeA64",
            &[opcode],
            &mut emitter,
            &register_file,
        )
        .unwrap();

        emitter.leave();

        let num_regs = emitter.next_vreg();
        let translation = Translation::new(ctx.compile(num_regs));

        register_file.write("R2", 0x1122_3344_5566_7788u64);
        register_file.write("R3", 0x9900_AABB_CCDD_EEFFu64);
        translation.execute(&register_file);
        register_file.read::<u64>("R2")
    }

    // 1000009c4: 93c2fc62     extr    x2, x3, x2, #0x3f
    // 1000009f8: 93c28062     extr    x2, x3, x2, #0x20
    // 100000a2c: 93c24062     extr    x2, x3, x2, #0x10
    // 100000a60: 93c20062     extr    x2, x3, x2, #0x0
    // 100000a94: 93c2c062     extr    x2, x3, x2, #0x30
    // 100000ac8: 93c20462     extr    x2, x3, x2, #0x1

    for (opcode, expected) in [
        (0x93c20062, 0x1122334455667788), // shift = 0
        (0x93c20462, 0x889119a22ab33bc4), // shift = 1
        (0x93c24062, 0xeeff112233445566), // shift = 16
        (0x93c28062, 0xccddeeff11223344), // shift = 32
        (0x93c2c062, 0xaabbccddeeff1122), // shift = 48
        (0x93c2fc62, 0x3201557799bbddfe), // shift = 63
    ] {
        assert_eq!(wrapper(opcode), expected);
    }
}

#[ktest]
fn branch_maybe_2048() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let opcode = emitter.constant(0x17ffffe1, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("_PC", 1000u64);
    register_file.write("SEE", -1i64);

    translation.execute(&register_file);

    assert_eq!(876, register_file.read::<u64>("_PC"));
}

#[ktest]
fn cbz_maybe_2048() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    //ffffffc008093d60:       b40005b4        cbz     x20, ffffffc008093e14
    // <__flush_smp_call_function_queue+0x130>
    let opcode = emitter.constant(0xb40005b4, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("_PC", 1000u64);
    register_file.write("SEE", -1i64);

    translation.execute(&register_file);
    assert_eq!(1180, register_file.read::<u64>("_PC"));
}

#[ktest]
fn ldp_128() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // ad410c02        ldp     q2, q3, [x0, #32]
    // execute_aarch64_instrs_memory_pair_simdfp_post_idx
    let opcode = emitter.constant(0xad410c02, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let mem = alloc::boxed::Box::new((
        u128::from_ne_bytes([0xABu8; 16]),
        u128::from_ne_bytes([0xBAu8; 16]),
    ));

    register_file.write("R0", (&*mem as *const _ as u64) - 32);
    register_file.write("SEE", -1i64);

    translation.execute(&register_file);

    let z_offset = model.reg_offset("_Z");

    let q2_offset = z_offset + 2 * 256;
    let q3_offset = z_offset + 3 * 256;

    let q2 = register_file.read_raw::<u128>(q2_offset.try_into().unwrap());
    let q3 = register_file.read_raw::<u128>(q3_offset.try_into().unwrap());

    assert_eq!(q2.to_ne_bytes(), [0xAB; 16]);
    assert_eq!(q3.to_ne_bytes(), [0xBA; 16]);
}

#[ktest]
fn simd_128_reg_minimal() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let n = emitter.constant(3, Type::Signed(64));
    let width = emitter.constant(128, Type::Signed(64));
    let result = translate(&*model, "V_read", &[n, width], &mut emitter, &register_file)
        .unwrap()
        .unwrap();

    assert!(matches!(
        result.kind(),
        NodeKind::GuestRegister { offset: _ }
    ));
    assert_eq!(result.typ(), Type::Unsigned(128));
}

#[ktest]
fn simd_128_reg_to_mem() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let n = emitter.constant(3, Type::Signed(64));
    let width = emitter.constant(128, Type::Signed(64));
    let result = translate(&*model, "V_read", &[n, width], &mut emitter, &register_file)
        .unwrap()
        .unwrap();

    let addr = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(64));
    emitter.write_memory(addr, result, false);
    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let mut mem = alloc::boxed::Box::new(u128::MAX);

    register_file.write("R0", &mut *mem as *mut _ as u64);
    register_file.write("SEE", -1i64);

    translation.execute(&register_file);

    // todo: more complex test
    assert_eq!(*mem, 0);
}

// todo: const asserts during execution
//#[ktest]
fn _currentvl_read() {
    //CurrentVL_read

    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let res = translate(&*model, "CurrentVL_read", &[], &mut emitter, &register_file)
        .unwrap()
        .unwrap();

    emitter.write_register(model.reg_offset("R0"), res);

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));
    translation.execute(&register_file);
    panic!("{}", register_file.read::<u64>("R0"));
}

#[ktest]
fn v_set() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let magic = 0xABCD_EF01_2345_6789_9876_5432_10FE_DCBAu128;

    let mem = alloc::boxed::Box::new(magic);
    let address = emitter.constant(&*mem as *const u128 as u64, Type::Unsigned(64));
    let value = emitter.read_memory(address, Type::Unsigned(128));
    let n = emitter.constant(3, Type::Signed(64));
    let width = emitter.constant(128, Type::Signed(64));
    translate(
        &*model,
        "V_set",
        &[n, width, value],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);

    let z_offset = model.reg_offset("_Z");

    let q3_offset = z_offset + 3 * 256;

    let q3 = register_file.read_raw::<[u8; 16]>(q3_offset.try_into().unwrap());

    assert_eq!(u128::from_ne_bytes(q3), magic)
}

#[ktest]
fn slice_mask() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // [
    //     X86NodeRef(X86Node {
    //         typ: Signed(64),
    //         kind: Constant {
    //             value: 2048,
    //             width: 64,
    //         },
    //     }),
    //     X86NodeRef(X86Node {
    //         typ: Signed(64),
    //         kind: Constant {
    //             value: 128,
    //             width: 64,
    //         },
    //     }),
    //     X86NodeRef(X86Node {
    //         typ: Signed(64),
    //         kind: BinaryOperation(Add(
    //             X86NodeRef(X86Node {
    //                 typ: Signed(64),
    //                 kind: BinaryOperation(Sub(
    //                     X86NodeRef(X86Node {
    //                         typ: Signed(64),
    //                         kind: BinaryOperation(Sub(
    //                             X86NodeRef(X86Node {
    //                                 typ: Signed(64),
    //                                 kind: ReadStackVariable { id: 4, width:
    // 64 },
    //.                            }),
    //                             X86NodeRef(X86Node {
    //                                 typ: Signed(64),
    //                                 kind: Constant {
    //                                     value: 1,
    //                                     width: 64,
    //                                 },
    //                             }),
    //                         )),
    //                     }),
    //                     X86NodeRef(X86Node {
    //                         typ: Signed(64),
    //                         kind: Constant {
    //                             value: 128,
    //                             width: 64,
    //                         },
    //                     }),
    //                 )),
    //             }),
    //             X86NodeRef(X86Node {
    //                 typ: Signed(64),
    //                 kind: Constant {
    //                     value: 1,
    //                     width: 64,
    //                 },
    //             }),
    //         )),
    //     }),
    // ]

    let n = emitter.constant(2048, Type::Signed(64));
    let i = emitter.constant(128, Type::Signed(64));
    let l = emitter.constant(0, Type::Signed(64));

    let res = translate(
        &*model,
        "slice_mask",
        &[n, i, l],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();

    emitter.leave();

    assert_eq!(
        res.kind(),
        &NodeKind::Constant {
            value: 0,
            width: 2048
        }
    )
}

//#[ktest]
fn _mrs_ttbr() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);

    // d5382021        mrs     x1, ttbr1_el1
    let opcode = emitter.constant(0xd5382021, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R1", 0x0);
    register_file.write::<u64>("_TTBR1_EL1_bits", 0x8224e000);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R1"), 0x8224e000);
}

#[ktest]
fn sttr() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);
    register_file.write("PSTATE_EL", 1u8);

    //f800081f        sttr    xzr, [x0]
    let opcode = emitter.constant(0xf800081f, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let mut data = Box::new(0xbee5_abcd_0123_9876u64);

    register_file.write::<u64>("R0", (&mut *data as *mut u64) as u64);

    translation.execute(&register_file);

    assert_eq!(*data, 0x0);
}

/// needs a device set up otherwise panics in the handler
#[ktest]
fn at() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);
    register_file.write("PSTATE_EL", 1u8);

    // d5087816        at      s1e1r, x22
    // AT_S1E1R_SysOpsWrite_efb944f010174dbe
    let opcode = emitter.constant(0xd5087816, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R22", 0xffff); //???

    translation.execute(&register_file);
}

#[ktest]
fn svc() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("SEE", -1i64);
    register_file.write("PSTATE_EL", 0u8);

    //    0xd4000001  svc     #0x0
    // execute_aarch64_instrs_system_exceptions_runtime_svc
    let opcode = emitter.constant(0xd4000001, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);
}

#[ktest]
fn stp_stuck_loop() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // a902de74        stp     x20, x23, [x19, #40]

    let opcode = emitter.constant(0xa902de74, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();
    //__DecodeA64_LoadStore
    // decode_stp_gen_aarch64_instrs_memory_pair_general_pre_idx
    // execute_aarch64_instrs_memory_pair_general_post_idx

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let dst = Box::<(u64, u64)>::new((0, 0));

    register_file.write("SEE", -1i64);
    register_file.write("R20", 0xFEEDu64);
    register_file.write("R23", 0xDEADu64);
    register_file.write("R19", (((&*dst) as *const (u64, u64)) as u64) - 40);

    translation.execute(&register_file);

    assert_eq!(*dst, (0xFEED, 0xDEAD));
}

#[ktest]
fn sttr_2() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    //    f80008c3        sttr    x3, [x6]
    // decode_sttr_aarch64_instrs_memory_single_general_immediate_signed_offset_unpriv
    // execute_aarch64_instrs_memory_single_general_immediate_signed_offset_unpriv
    let opcode = emitter.constant(0xf80008c3, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let mut data = Box::new(0xffff_aaaa_ffff_aaaau64);

    register_file.write::<u64>("R3", 0xbee5_abcd_0123_9876);
    register_file.write::<u64>("R6", (&mut *data as *mut u64) as u64);

    translation.execute(&register_file);

    assert_eq!(*data, 0xbee5_abcd_0123_9876);
}

#[ktest]
fn cmeq_v116b() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // 4e209801        cmeq    v1.16b, v0.16b, #0
    // execute_aarch64_instrs_vector_arithmetic_unary_cmp_int_bulk_sisd
    let opcode = emitter.constant(0x4e209801, Type::Unsigned(32));
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let _translation = ctx.compile(num_regs);
}

#[ktest]
fn simbench_eret() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write::<u8>("PSTATE_EL", 1);
    register_file.write::<u64>("SCR_EL3_bits", 0x430);
    //  eret
    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0xd69f03e0,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();

    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("ELR_EL1", 0x1000);
    register_file.write::<u64>("SPSR_EL1_bits", 0x0);
    register_file.write::<u64>("_PC", 0x40000000);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u8>("PSTATE_IL"), 0);
    assert_eq!(register_file.read::<u8>("PSTATE_EL"), 0);
    assert_eq!(register_file.read::<u64>("_PC"), 0x1000);
}

#[ktest]
fn simbench_el_from_spsr() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let spsr = emitter.constant(0x0, Type::Unsigned(64));
    let valid_target_tuple =
        translate(&*model, "ELFromSPSR", &[spsr], &mut emitter, &register_file)
            .unwrap()
            .unwrap();

    let valid = emitter.access_tuple(valid_target_tuple.clone(), 0);
    let target = emitter.access_tuple(valid_target_tuple, 1);
    emitter.write_register(model.reg_offset("R0"), valid);
    emitter.write_register(model.reg_offset("R1"), target);
    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u8>("PSTATE_EL", 1);
    assert_eq!(register_file.read::<u64>("R0"), 0x0);

    translation.execute(&register_file);

    // valid = true
    assert_eq!(register_file.read::<u64>("R0"), 0x1);

    // EL should be 0 afterwards
    assert_eq!(register_file.read::<u64>("R1"), 0x0);
}

#[ktest]
fn simbench_illegal_exception_return() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write::<u8>("PSTATE_EL", 1);
    register_file.write::<u64>("SCR_EL3_bits", 0x430);

    let spsr = emitter.read_register(model.reg_offset("SPSR_EL1_bits"), Type::Unsigned(64));
    let illegal_psr_state = translate(
        &*model,
        "IllegalExceptionReturn",
        &[spsr],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();

    emitter.write_register(model.reg_offset("R0"), illegal_psr_state);
    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    assert_eq!(register_file.read::<u64>("SPSR_EL1_bits"), 0x0);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R0"), 0x0);
}

#[ktest]
fn is_secure_below_el3() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let is_secure_below_el3 = translate(
        &*model,
        "IsSecureBelowEL3",
        &[],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();

    emitter.write_register(model.reg_offset("R0"), is_secure_below_el3);
    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R0"), 1);
}

// todo: re-enable me, absolutely no clue why this is broken when the other
// simbench eret tests pass
//#[ktest]
fn _simbench_elusingaarch32k() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("PSTATE_EL", 1u8);
    register_file.write::<u64>("SCR_EL3_bits", 0x430);

    let target = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(2));
    let is_secure_below_el3 = emitter.read_register(model.reg_offset("R1"), Type::Unsigned(1));

    let tuple = translate(
        &*model,
        "ELStateUsingAArch32K",
        &[target, is_secure_below_el3],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();

    let known = emitter.access_tuple(tuple.clone(), 0);
    emitter.write_register(model.reg_offset("R3"), known);

    let aarch32 = emitter.access_tuple(tuple, 1);
    emitter.write_register(model.reg_offset("R4"), aarch32);

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    // EL0 target
    register_file.write::<u64>("R0", 0);
    // is_secure_below_el3
    register_file.write::<u64>("R1", 1);

    translation.execute(&register_file);

    // known
    assert_eq!(register_file.read::<u64>("R3"), 1);
    // target_el_is_aarch32
    assert_eq!(register_file.read::<u64>("R4"), 0);
}

#[ktest]
fn cbnz() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // execute_aarch64_instrs_branch_conditional_compare
    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x35000080,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);
}

#[ktest]
fn branch_not_taken() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let _res = translate(&*model, "BranchNotTaken", &[], &mut emitter, &register_file).unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);
}

#[ktest]
fn mrs_current_el_1() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write::<u8>("PSTATE_EL", 0x1);

    //      mrs     x0, currentel
    // execute_aarch64_instrs_system_register_system
    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0xd5384240,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R0"), 1 << 2);
}

#[ktest]
fn mrs_current_el_3() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write::<u8>("PSTATE_EL", 0x3);

    //      mrs     x0, currentel
    // execute_aarch64_instrs_system_register_system
    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0xd5384240,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R0"), 3 << 2);
}

#[ktest]
fn ic_ivau() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // d50b7520        ic      ivau, x0
    // decode_sys_aarch64_instrs_system_sysops
    // IC_IVAU_SysOpsWrite_f40d5c6453a840a5
    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0xd50b7520,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);
}

#[ktest]
fn ldurb() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // 385fd001 	ldurb	w1, [x0, #-3]
    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x385fd001,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let mem = alloc::boxed::Box::new(0xdead_c0de_a3a4_a5beu64);

    register_file.write("SEE", -1i64);
    register_file.write::<u64>("R0", (&*mem as *const u64 as u64) + 4);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R1"), 0xa5);
}

#[ktest]
fn ldr_q0() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // 3cdd0d60 	ldr	q0, [x11, #-48]!
    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x3cdd0d60,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let mem = alloc::boxed::Box::new(0xbee5_bee5_feed_feed_dead_c0de_a3a4_a5beu128);

    register_file.write("SEE", -1i64);
    register_file.write::<u64>("R11", (&*mem as *const u128 as u64) + 48);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u128>("_Z"), *mem);
}

#[ktest]
fn tpidr_el0() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write::<u8>("PSTATE_EL", 0x3);

    //  d51bd048        msr     tpidr_el0, x8
    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0xd51bd048,
    )
    .unwrap();

    //  d53bd048 	mrs	x8, tpidr_el0
    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0xd53bd048,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R8", 0xfeed_bee5_babe_cafe);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R8"), 0xfeed_bee5_babe_cafe);
}

#[ktest]
fn sub_sxtw() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    //cb23c083 	sub	x3, x4, w3, sxtw
    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0xcb23c083,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R3", 0);
    register_file.write::<i64>("R4", -16);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R3"), 0xfffffffffffffff0);
}

#[ktest]
fn adcs_fuzzed_0() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // 3a090207        adcs    w7, w16, w9
    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x3a090207,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R16", 0xcb8f8b10b055dfd5);
    register_file.write::<i64>("R9", 0x335c1e9bbd404fb6);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R7"), 0x000000006d962f8b);
}

#[ktest]
fn ldp_q_el1() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    register_file.write("PSTATE_EL", 1u8);

    // ad480400        ldp     q0, q1, [x0, #256]
    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0xad480400,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let mem = alloc::boxed::Box::new((
        u128::from_ne_bytes([0xABu8; 16]),
        u128::from_ne_bytes([0xBAu8; 16]),
    ));

    register_file.write("R0", (&*mem as *const _ as u64) - 256);

    translation.execute(&register_file);

    let z_offset = model.reg_offset("_Z");

    let q0_offset = z_offset;
    let q1_offset = z_offset + 256;

    let q0 = register_file.read_raw::<u128>(q0_offset.try_into().unwrap());
    let q1 = register_file.read_raw::<u128>(q1_offset.try_into().unwrap());

    assert_eq!(q0.to_ne_bytes(), [0xAB; 16]);
    assert_eq!(q1.to_ne_bytes(), [0xBA; 16]);
}

#[ktest]
fn stp_q() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // ad080400        stp     q0, q1, [x0, #256]
    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0xad080400,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let mem = alloc::boxed::Box::new((0u128, 0u128));

    register_file.write("R0", (&*mem as *const _ as u64) - 256);

    let z_offset = model.reg_offset("_Z");

    let q0_offset = z_offset;
    let q1_offset = z_offset + 256;

    register_file.write_raw::<u128>(
        q0_offset.try_into().unwrap(),
        0xdead_c0de_a3a4_a5be_a5be_a3a4_c0de_deadu128,
    );
    register_file.write_raw::<u128>(
        q1_offset.try_into().unwrap(),
        0xbeeb_c0de_b33b_beeb_a5be_a3a4_c0de_deadu128,
    );

    translation.execute(&register_file);

    assert_eq!(
        *mem,
        (
            0xdead_c0de_a3a4_a5be_a5be_a3a4_c0de_deadu128,
            0xbeeb_c0de_b33b_beeb_a5be_a3a4_c0de_deadu128
        )
    );
}

#[ktest]
fn add_8h() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    //  4e7585e8        add     v8.8h, v15.8h, v21.8h
    // execute_aarch64_instrs_vector_arithmetic_binary_uniform_add_wrapping_single_sisd
    translate_instruction(
        &model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x4e7585e8,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let z_offset = model.reg_offset("_Z");

    let q8_offset = z_offset + (8 * 256);
    let q15_offset = z_offset + (15 * 256);
    let q21_offset = z_offset + (21 * 256);

    register_file.write_raw::<u128>(
        q15_offset.try_into().unwrap(),
        0xc81f_ea1d_0c9a_8432_0a30_dc06_5f5a_3cc6,
    );
    register_file.write_raw::<u128>(
        q21_offset.try_into().unwrap(),
        0xba8c_dd67_b6bb_aa72_1b52_79bc_c38e_d791,
    );

    translation.execute(&register_file);

    assert_eq!(
        register_file.read_raw::<u128>(q8_offset.try_into().unwrap()),
        0x82ab_c784_c355_2ea4_2582_55c2_22e8_1457,
    );
}

#[ktest]
fn simd_const() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    translate(
        &model,
        "CheckFPAdvSIMDEnabled64",
        &[],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let _translation = ctx.compile(num_regs);
}

#[ktest]
fn tst_x10_0x7() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    //f240095f        tst     x10, #0x7
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0xf240095f,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let _translation = ctx.compile(num_regs);
}

#[ktest]
fn eon0() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // eon     w1, w14, wzr, lsr #0
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x4a7f01c1,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R1", 0x2bff0ab223a5d276);
    register_file.write::<u64>("R14", 0xc00f72da2c87ce6d);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R1"), 0x00000000d3783192);
}

#[ktest]
fn eon1() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // eon     w17, w1, wzr, asr #0
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x4abf0031,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R1", 0xb6372387c2d7c933);
    register_file.write::<u64>("R17", 0xced92c4a57e3d54c);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R17"), 0x000000003d2836cc);
}

#[ktest]
fn ngc() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // 5a0803f9        ngc     w25, w8
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x5a0803f9,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R8", 0xa989ef1cfc82fcde);
    register_file.write::<u64>("R25", 0x2e0d38b5c2b1f8a0);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R25"), 0x00000000037d0321);
}

#[ktest]
fn extr_fuzz0() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // 93d20383        extr    x3, x28, x18, #0
    // decode_extr_aarch64_instrs_integer_ins_ext_extract_immediate
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x93d20383,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R3", 0xc3c37f76e3192302);
    register_file.write::<u64>("R18", 0x63c68fa3f1af6ec2);
    register_file.write::<u64>("R28", 0xc3b638d9c08fee18);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R3"), 0x63c68fa3f1af6ec2);
}

#[ktest]
fn movi() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // 2f00e413        movi    d19, #0x0
    // (64-bit scalar variant)
    // execute_aarch64_instrs_vector_logical
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x2f00e413,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let z_offset = model.reg_offset("_Z");

    let q19_offset = z_offset + (19 * 256);

    register_file.write_raw::<u128>(
        q19_offset.try_into().unwrap(),
        0x0633_1f8f_bf71_6915_8b38_29bf_0b64_c3fb,
    );

    translation.execute(&register_file);

    assert_eq!(
        register_file.read_raw::<u128>(q19_offset.try_into().unwrap()),
        0x0,
    );
}

#[ktest]
fn v_set_zeroextend() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let value = emitter.constant(0x0, Type::Unsigned(64));

    let n = emitter.constant(3, Type::Signed(64));
    let width = emitter.constant(128, Type::Signed(64));
    translate(
        &*model,
        "V_set",
        &[n, width, value],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let q3_offset = usize::try_from(model.reg_offset("_Z") + 3 * 256).unwrap();

    register_file.write_raw::<u128>(q3_offset, u128::MAX);

    translation.execute(&register_file);

    let q3 = register_file.read_raw::<u128>(q3_offset);

    assert_eq!(q3, 0x0)
}

#[ktest]
fn and_16b_fuzz0() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    //  4e391ef7        and     v23.16b, v23.16b, v25.16b
    // execute_aarch64_instrs_vector_arithmetic_binary_uniform_logical_and_orr
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x4e391ef7,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let z_offset = model.reg_offset("_Z");

    let q23_offset = z_offset + (23 * 256);
    let q25_offset = z_offset + (25 * 256);

    register_file.write_raw::<u128>(
        q23_offset.try_into().unwrap(),
        0x9eb304522041182818bcf95624574a2b,
    );
    register_file.write_raw::<u128>(
        q25_offset.try_into().unwrap(),
        0x25166d3d3314667bd66dd0839c033d5,
    );

    translation.execute(&register_file);

    assert_eq!(
        register_file.read_raw::<u128>(q23_offset.try_into().unwrap()),
        0x2110452000100201824d90020400201,
    );
}

#[ktest]
fn and_16b_fuzz1() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    //  4e291dc9        and     v9.16b, v14.16b, v9.16b
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x4e291dc9,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let z_offset = model.reg_offset("_Z");

    let q9 = z_offset + (9 * 256);
    let q14 = z_offset + (14 * 256);

    register_file.write_raw::<u128>(q9.try_into().unwrap(), 0x77de563527e15eff04aab0566879b64a);
    register_file.write_raw::<u128>(q14.try_into().unwrap(), 0x58ff01ce77a710837235c72a6f6bafca);

    translation.execute(&register_file);

    assert_eq!(
        register_file.read_raw::<u128>(q9.try_into().unwrap()),
        0x50de000427a11083002080026869a64a,
    );
}

#[ktest]
fn ror_fuzz0() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    //  1ac92d30        ror     w16, w9, w9
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x1ac92d30,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R9", 0xe6c9cbd5dd74ba18);
    register_file.write::<u64>("R16", 0xe2642ce94754f140);

    translation.execute(&register_file);

    // ror = lambda val, r_bits, max_bits: \
    //     ((val & (2**max_bits-1)) >> r_bits%max_bits) | \
    //     (val << (max_bits-(r_bits%max_bits)) & (2**max_bits-1))
    //
    // hex(ror(0xdd74ba18, 24, 32)) == 0x74ba18dd
    //
    // but instead we're getting 0xdd74ba18
    // which is
    // hex(ror(0xdd74ba18, 0, 32)) or hex(ror(0xdd74ba18, 32, 32))

    assert_eq!(register_file.read::<u64>("R16"), 0x0000000074ba18dd);
}

#[ktest]
fn ror_fuzz1() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    //   1ac72fb2        ror     w18, w29, w7
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x1ac72fb2,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R7", 0x7e8ef49be3c7f8dd);
    register_file.write::<u64>("R18", 0x2ad8c433a8cd3cf2);
    register_file.write::<u64>("R29", 0x14dbeac7f5f41321);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R18"), 0x00000000afa0990f);
}

#[ktest]
fn ror_fuzz2() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    //  1ac52f1a        ror     w26, w24, w5
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x1ac52f1a,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R5", 0xf2da4c18b6f72872);
    register_file.write::<u64>("R24", 0x09d7b322ae0ae0a6);
    register_file.write::<u64>("R26", 0xba967a6b1fea241c);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R26"), 0x00000000b829ab82);
}

#[ktest]
fn modulo() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let reg = emitter.read_register(model.reg_offset("R9"), Type::Unsigned(32));
    let cast = emitter.cast(reg, Type::Signed(64), CastOperationKind::Reinterpret);
    let _32 = emitter.constant(32, Type::Signed(64));
    let res = emitter.binary_operation(BinaryOperationKind::Modulo(cast, _32));
    emitter.write_register(model.reg_offset("R10"), res);
    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("R9", 0xdd74ba18u32);

    translation.execute(&register_file);

    assert_eq!(24, register_file.read::<u64>("R10"))
}

#[ktest]
fn ror_0() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let x = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(32));
    let shift = emitter.read_register(model.reg_offset("R1"), Type::Signed(64));

    let res = translate(&*model, "ROR", &[x, shift], &mut emitter, &register_file)
        .unwrap()
        .unwrap();

    emitter.write_register(model.reg_offset("R2"), res);
    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("R0", 0xffu32);
    register_file.write("R1", 0x1u32);

    translation.execute(&register_file);

    assert_eq!(0x8000007f, register_file.read::<u64>("R2"))
}

#[ktest]
fn ror_1() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let x = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(32));
    let shift = emitter.read_register(model.reg_offset("R1"), Type::Signed(64));

    let res = translate(&*model, "ROR", &[x, shift], &mut emitter, &register_file)
        .unwrap()
        .unwrap();

    emitter.write_register(model.reg_offset("R2"), res);
    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("R0", 0xffu32);
    register_file.write("R1", 0x8u32);

    translation.execute(&register_file);

    assert_eq!(0xff000000, register_file.read::<u64>("R2"))
}

#[ktest]
fn ror_2() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let x = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(32));
    let shift = emitter.read_register(model.reg_offset("R1"), Type::Signed(64));

    let res = translate(&*model, "ROR", &[x, shift], &mut emitter, &register_file)
        .unwrap()
        .unwrap();

    emitter.write_register(model.reg_offset("R2"), res);
    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("R0", 0xffu32);
    register_file.write("R1", 0x4u32);

    translation.execute(&register_file);

    assert_eq!(0xf000000f, register_file.read::<u64>("R2"))
}

#[ktest]
fn ror_3() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let x = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(32));
    let shift = emitter.read_register(model.reg_offset("R1"), Type::Signed(64));

    let res = translate(&*model, "ROR", &[x, shift], &mut emitter, &register_file)
        .unwrap()
        .unwrap();

    emitter.write_register(model.reg_offset("R2"), res);
    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("R0", 0xffu32);
    register_file.write("R1", 10u32);

    translation.execute(&register_file);

    assert_eq!(0x3fc00000, register_file.read::<u64>("R2"))
}

#[ktest]
fn ror_4() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let x = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(32));
    let shift = emitter.read_register(model.reg_offset("R1"), Type::Signed(64));

    let res = translate(&*model, "ROR", &[x, shift], &mut emitter, &register_file)
        .unwrap()
        .unwrap();

    emitter.write_register(model.reg_offset("R2"), res);
    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("R0", 0xffu32);
    register_file.write("R1", 17u32);

    translation.execute(&register_file);

    assert_eq!(0x7f8000, register_file.read::<u64>("R2"))
}

#[ktest]
fn shiftreg_2() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let reg = emitter.constant(1, Type::Signed(64));
    let shift_type = emitter.constant(3, Type::Signed(32));
    let amount = emitter.constant(17, Type::Signed(64));
    let width = emitter.constant(32, Type::Signed(64));
    let value = translate(
        &*model,
        "ShiftReg",
        &[reg, shift_type, amount, width],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();

    emitter.write_register(model.reg_offset("R0"), value);

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R1", 0xff);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u32>("R0"), 0x7f8000);
}

#[ktest]
fn ror_no_modulo() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let read = emitter.read_register(model.reg_offset("R1"), Type::Unsigned(32));
    let cast_read = emitter.cast(read, Type::Signed(64), CastOperationKind::Reinterpret);
    // let _32 = emitter.constant(32, Type::Signed(64));
    // let amount = emitter.binary_operation(BinaryOperationKind::Modulo(cast_read,
    // _32));

    let x = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(32));

    let value = translate(
        &*model,
        "ROR",
        &[x, cast_read],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();

    emitter.write_register(model.reg_offset("R2"), value);

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R0", 0xff);
    register_file.write::<u64>("R1", 17);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u32>("R2"), 0x7f8000);
}

#[ktest]
fn ror_modulo() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let read = emitter.read_register(model.reg_offset("R1"), Type::Unsigned(32));
    let cast_read = emitter.cast(read, Type::Signed(64), CastOperationKind::Reinterpret);
    let _32 = emitter.constant(32, Type::Signed(64));
    let amount = emitter.binary_operation(BinaryOperationKind::Modulo(cast_read, _32));

    let x = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(32));

    let value = translate(&*model, "ROR", &[x, amount], &mut emitter, &register_file)
        .unwrap()
        .unwrap();

    emitter.write_register(model.reg_offset("R2"), value);

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R0", 0xff);
    register_file.write::<u64>("R1", 17);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u32>("R2"), 0x7f8000);
}

// todo: re-enable me after all non float non SIMD instructions pass
// #[ktest]
// fn cnt_8b() {
//     let model = models::get("aarch64").unwrap();

//     let register_file = RegisterFile::init(&*model);
//     let mut ctx = X86TranslationContext::new(&model, false,
// register_file.global_register_offset());     let mut emitter =
// X86Emitter::new(&mut ctx);

//     //  0e205903        cnt     v3.8b, v8.8b
//     translate_instruction(
//
//         &*model,
//         "__DecodeA64",
//         &mut emitter,
//         &register_file,
//         0x0e205903,
//     )
//     .unwrap();

//     emitter.leave();
//     let num_regs = emitter.next_vreg();
//     let translation = Translation::new(ctx.compile(num_regs));

//     let z_offset = model.reg_offset("_Z");

//     let q3_offset = z_offset + (3 * 256);
//     let q8_offset = z_offset + (8 * 256);

//     register_file.write_raw::<u128>(
//         q8_offset.try_into().unwrap(),
//         0xd697f587e3887926a72f69dbcb976731,
//     );

//     translation.execute(&register_file);

//     assert_eq!(
//         register_file.read_raw::<u128>(q3_offset.try_into().unwrap()),
//         0x0505040605050503,
//     );
// }

// #[ktest]
// fn addp_16b() {
//     let model = models::get("aarch64").unwrap();

//     let register_file = RegisterFile::init(&*model);
//     let mut ctx = X86TranslationContext::new(&model, false,
// register_file.global_register_offset());     let mut emitter =
// X86Emitter::new(&mut ctx);

//     //  4e24bc9b        addp    v27.16b, v4.16b, v4.16b
//     // execute_aarch64_instrs_vector_arithmetic_binary_uniform_add_wrapping_pair
//     translate_instruction(
//
//         &model,
//         "__DecodeA64",
//         &mut emitter,
//         &register_file,
//         0x4e24bc9b,
//     )
//     .unwrap();

//     emitter.leave();
//     let num_regs = emitter.next_vreg();
//     let translation = Translation::new(ctx.compile(num_regs));

//     log::error!("{translation:?}");

//     let z_offset = model.reg_offset("_Z");

//     let q4_offset = z_offset + (4 * 256);
//     let q27_offset = z_offset + (27 * 256);

//     register_file.write_raw::<u128>(
//         q4_offset.try_into().unwrap(),
//         0x69e57f8761d4e752d7e93c9c503648a8,
//     );

//     translation.execute(&register_file);

//     assert_eq!(
//         register_file.read_raw::<u128>(q27_offset.try_into().unwrap()),
//         0x119591d9ddd4d090119591d9ddd4d090,
//     );
// }

// #[ktest]
// fn scvtf() {
//     let model = models::get("aarch64").unwrap();

//     let register_file = RegisterFile::init(&*model);
//     let mut ctx = X86TranslationContext::new(&model, false,
// register_file.global_register_offset());     let mut emitter =
// X86Emitter::new(&mut ctx);

//     //1e22007e        scvtf   s30, w3
//     translate_instruction(
//
//         &*model,
//         "__DecodeA64",
//         &mut emitter,
//         &register_file,
//         0x1e22007e,
//     )
//     .unwrap();

//     emitter.leave();
//     let num_regs = emitter.next_vreg();
//     let translation = Translation::new(ctx.compile(num_regs));

//     let z_offset = model.reg_offset("_Z");

//     let q30 = z_offset + (30 * 256);

//     register_file.write::<u64>("R3", 0xecf92f9fcd4d46f7);

//     translation.execute(&register_file);

//     assert_eq!(
//         register_file.read_raw::<u128>(q30.try_into().unwrap()),
//         0xce4acae4,
//     );
// }

#[ktest]
fn sdiv_test_0_panic() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // 1ad60fee        sdiv    w14, wzr, w22
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x1ad60fee,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R14", 0xacb63d74bafd2349);
    register_file.write::<u64>("R22", 0xe69610c3e9d490df);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R14"), 0x0000000000000000);
}

#[ktest]
fn sdiv_fuzz0() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // 1ad80cf5        sdiv    w21, w7, w24
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x1ad80cf5,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R7", 0xb856ee4bd05ba6f3);
    register_file.write::<u64>("R24", 0x818001ef0030e554);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R21"), 0x00000000ffffff07);
}

#[ktest]
fn sdiv_fuzz1() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // 1ad40ea4        sdiv    w4, w21, w20
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x1ad40ea4,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R20", 0xb74f5a8c25ea6281);
    register_file.write::<u64>("R21", 0x31fc170686f2190e);

    // =  0x86f2190e / 0x25ea6281
    // = -2030954226 / 636117633
    // = -3.192 (rtz)
    // = -3
    // = ffffffd

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R4"), 0x00000000fffffffd);
}

#[ktest]
fn sdiv_fuzz2() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // 1ad70e9e        sdiv    w30, w20, w23
    // decode_sdiv_aarch64_instrs_integer_arithmetic_div
    // execute_aarch64_instrs_integer_arithmetic_div, is_unsigned = false
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x1ad70e9e,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R20", 0x91e38acb99559afc);
    register_file.write::<u64>("R23", 0x9c5ce5b0d73b9402);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R30"), 0x0000000000000002);
}

#[ktest]
fn udiv() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let opcode = emitter.constant(0x9ac10a73, Type::Unsigned(32));
    // decode_udiv_aarch64_instrs_integer_arithmetic_div
    // execute_aarch64_instrs_integer_arithmetic_div, is_unsigned = true
    translate(
        &*model,
        "__DecodeA64",
        &[opcode],
        &mut emitter,
        &register_file,
    )
    .unwrap();

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let x = 0xffffff8008bfffffu64;
    let y = 0x200000u64;

    register_file.write("SEE", -1i64);
    register_file.write("R1", y);
    register_file.write("R19", x);

    // = 0xffff_ff80_08bf_ffffu64 / 0x200000
    // = 8796092760134.0
    // = 0x7fffffc0046 (?? why one less idk)

    // but if we sign extend we get

    // = 0xffff_ffff_ffff_ffff ++ ffff_ff80_08bf_ffff / 0x200000
    // = 0x07ff_ffff_ffff_ffff_ffff_fffc_0045

    translation.execute(&register_file);

    assert_eq!(0x0000_07ff_fffc_0045, register_file.read::<u64>("R19"));
}

#[ktest]
fn fuzz_0b2313e2_59_fixed() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // 0b2313e2        add     w2, wsp, w3, uxtb #4

    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0xb2313e2,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R3", 0x5344d5fdd5205949);
    register_file.write::<u64>("SP_EL3", 0x400058f0);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R2"), 0x40005d80);
}

#[ktest]
fn fuzz_1ac20abb_2645_fixed() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // 1ac20abb        udiv    w27, w21, w2
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x1ac20abb,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R21", 0xb8259bcc103f4a0a);
    register_file.write::<u64>("R2", 0x2c12f99d2af9e7c4);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R27"), 0x0);
}

#[ktest]
fn fuzz_9b487f10_2213_fixed() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    //  9b487f10        smulh   x16, x24, x8
    //  block 0x18d
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x9b487f10,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R8", 0x5721b92f8470d45b);
    register_file.write::<u64>("R24", 0xa6295cbf50297bbd);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R16"), 0xe16c38dd300b71d9);
}

#[ktest]
fn crc() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    //  9ac34c08        crc32x  w8, w0, x3
    // execute_aarch64_instrs_integer_crc
    // Poly32Mod2
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x9ac34c08,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R0", 0x0);
    register_file.write::<u64>("R3", 0x1234_5678_0a0b_cdef);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R8"), 0x39786938);
}

#[ktest]
fn bitreverse_dyn_32() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let data_in = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(32));

    let res = translate(
        &*model,
        "BitReverse",
        &[data_in],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();

    assert_eq!(res.typ().width(), 32);

    emitter.write_register(model.reg_offset("R1"), res);
    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R0", 0x12345678);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R1"), 0x1e6a2c48);
}

#[ktest]
fn bitreverse_const_32() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let data_in = emitter.constant(0x12345678, Type::Unsigned(32));

    let res = translate(
        &*model,
        "BitReverse",
        &[data_in],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();

    assert_eq!(
        res.kind(),
        &NodeKind::Constant {
            value: 0x1e6a2c48,
            width: 32
        }
    );
}

#[ktest]
fn bitinsert_128_0() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let _0 = emitter.constant(0, Type::Unsigned(16));
    let _32 = emitter.constant(32, Type::Unsigned(16));
    let _63 = emitter.constant(63, Type::Unsigned(16));
    let _96 = emitter.constant(96, Type::Unsigned(16));

    let source = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(32));
    let target = emitter.create_bits(_0, _96);

    let res = emitter.bit_insert(target, source, _63, _32);

    let mut boxed = Box::new(0u128);
    let address = emitter.constant(&mut *boxed as *mut u128 as u64, Type::Unsigned(64));

    emitter.write_memory(address, res, false);

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R0", 0x12345678);

    translation.execute(&register_file);

    assert_eq!(*boxed, 0x12345678 << 63)
}

#[ktest]
fn bitinsert_128_1() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let _0 = emitter.constant(0, Type::Unsigned(16));
    let length = emitter.constant(32, Type::Unsigned(16));
    let start = emitter.constant(47, Type::Unsigned(16));
    let _96 = emitter.constant(96, Type::Unsigned(16));

    let source = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(32));
    let target = emitter.create_bits(_0, _96);

    let res = emitter.bit_insert(target, source, start, length);

    let mut boxed = Box::new(0u128);
    let address = emitter.constant(&mut *boxed as *mut u128 as u64, Type::Unsigned(64));

    emitter.write_memory(address, res, false);

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R0", 0xAABB_CCDD_1234_5678);

    translation.execute(&register_file);

    assert_eq!(*boxed, 0x1234_5678 << 47)
}

#[ktest]
fn bitinsert_64() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let _0 = emitter.constant(0, Type::Unsigned(16));
    let _64 = emitter.constant(64, Type::Unsigned(16));

    let source = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(32));
    let target = emitter.create_bits(_0, _64);

    let length = emitter.constant(16, Type::Unsigned(16));
    let start = emitter.constant(32, Type::Unsigned(16));
    let res = emitter.bit_insert(target, source, start, length);

    let mut boxed = Box::new(u64::MAX);
    let address = emitter.constant(&mut *boxed as *mut u64 as u64, Type::Unsigned(64));

    emitter.write_memory(address, res, false);

    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write::<u64>("R0", 0x12345678);

    translation.execute(&register_file);

    assert_eq!(*boxed, 0x5678 << 32)
}

#[ktest]
fn poly32mod2() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let mut boxed = Box::new(0u128);
    let addr = emitter.constant(&mut *boxed as *mut u128 as u64, Type::Unsigned(96));

    let data_in = emitter.read_memory(addr, Type::Unsigned(96));
    let poly = emitter.constant(0x04C11DB7, Type::Unsigned(32));

    let res = translate(
        &*model,
        "Poly32Mod2",
        &[data_in, poly],
        &mut emitter,
        &register_file,
    )
    .unwrap()
    .unwrap();

    assert_eq!(res.typ().width(), 32);

    emitter.write_register(model.reg_offset("R1"), res);
    emitter.leave();

    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R1"), 0x1e6a2c48);
}

#[common::ktest]
fn umov_fuzz0() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    //  0e013c80        umov    w0, v4.b[0]
    // execute_aarch64_instrs_vector_transfer_integer_move_unsigned
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x0e013c80,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    let z_offset = model.reg_offset("_Z");

    let q4_offset = z_offset + (4 * 256);

    register_file.write_raw::<u128>(
        q4_offset.try_into().unwrap(),
        0x9eb304522041182818bcf95624574a2b,
    );

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R0"), 0x2b);
}

#[ktest]
fn cnt_fuzz0() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    // 0e205879        cnt     v25.8b, v3.8b
    // execute_aarch64_instrs_vector_arithmetic_unary_cnt
    translate_instruction(
        &*model,
        "__DecodeA64",
        &mut emitter,
        &register_file,
        0x0e205879,
    )
    .unwrap();

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    log::error!("{translation:?}");

    let z_offset = model.reg_offset("_Z");

    let q3_offset = z_offset + (3 * 256);
    let q25_offset = z_offset + (25 * 256);

    register_file.write_raw::<u128>(
        q3_offset.try_into().unwrap(),
        0xcdac94bff8150e4b85879e1bd76f6503,
    );

    translation.execute(&register_file);

    assert_eq!(
        register_file.read_raw::<u128>(q25_offset.try_into().unwrap()),
        0x304050406060402
    );
}

#[ktest]
fn bitcount_const_0() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let value = emitter.constant(0b0010_1010_1011, Type::Unsigned(32));

    let count = translate(&*model, "BitCount", &[value], &mut emitter, &register_file)
        .unwrap()
        .unwrap();

    assert_eq!(
        count.kind(),
        &NodeKind::Constant {
            value: 6,
            width: 64
        }
    );
}

#[ktest]
fn bitcount_const_max() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let value = emitter.constant(u64::from(u32::MAX), Type::Unsigned(32));

    let count = translate(&*model, "BitCount", &[value], &mut emitter, &register_file)
        .unwrap()
        .unwrap();

    assert_eq!(
        count.kind(),
        &NodeKind::Constant {
            value: 32,
            width: 64
        }
    );
}

#[ktest]
fn bitcount_dyn_0() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let value = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(8));

    let count = translate(&*model, "BitCount", &[value], &mut emitter, &register_file)
        .unwrap()
        .unwrap();

    emitter.write_register(model.reg_offset("R1"), count);

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));

    register_file.write("R0", 0b0);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R1"), 0);
}

#[ktest]
fn bitcount_dyn_ff() {
    let (model, register_file, mut ctx) = setup();
    let mut emitter = X86Emitter::new(&mut ctx);

    let value = emitter.read_register(model.reg_offset("R0"), Type::Unsigned(8));

    let count = translate(&*model, "BitCount", &[value], &mut emitter, &register_file)
        .unwrap()
        .unwrap();

    emitter.write_register(model.reg_offset("R1"), count);

    emitter.leave();
    let num_regs = emitter.next_vreg();
    let translation = Translation::new(ctx.compile(num_regs));
    log::error!("{translation:?}");

    register_file.write("R0", 0xff);

    translation.execute(&register_file);

    assert_eq!(register_file.read::<u64>("R1"), 8);
}
