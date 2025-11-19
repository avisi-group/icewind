use {
    crate::guest::{
        Translation,
        devices::arm::mmu::{TranslationType, guest_translate, take_arm_exception},
        get_current_guest,
        tracing::{
            self, INSTRUCTION_COUNT, trace_instruction_end, trace_instruction_start,
            trace_memory_read, trace_memory_write, trace_register_read, trace_register_write,
        },
    },
    alloc::{
        alloc::alloc_zeroed, borrow::ToOwned, collections::btree_map::BTreeMap, string::String,
        sync::Arc, vec::Vec,
    },
    common::{
        device::{Device, Tickable},
        hashmap::HashMap,
        intern::InternedString,
        rudder::{Model, RegisterCacheType, RegisterDescriptor},
    },
    core::{
        alloc::Layout,
        fmt::{self, Debug},
        sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    dbt::{
        bump_alloc::{BumpAllocator, BumpAllocatorRef},
        emitter::{Emitter, Type},
        register_file::{RegisterFile, WellKnownRegister},
        translate::translate_instruction,
        x86::{
            Callbacks, EMIT_TRACING, X86TranslationContext,
            emitter::{BinaryOperationKind, X86Emitter},
        },
    },
    kernel::{
        arch::x86::{memory::VirtualMemoryArea, safepoint::record_safepoint},
        fs::Filesystem,
    },
    spin::{Lazy, Mutex},
    x86_64::structures::paging::{PageSize, Size4KiB},
};

/// Size in bytes for the per-translation bump allocator
const TRANSLATION_ALLOCATOR_SIZE: usize = 4 * 1024 * 1024 * 1024;

/// Limit blocks to contain only 1 instruction
const SINGLE_STEP: bool = false;

/// Enable the jump table chain cache
const CHAIN_CACHE_ENABLED: bool = true;
pub const CHAIN_CACHE_ENTRY_COUNT: usize = 65536;
const _: () = assert!(CHAIN_CACHE_ENTRY_COUNT.is_power_of_two());

pub static BUMP_ALLOCATOR: Lazy<BumpAllocator> =
    Lazy::new(|| BumpAllocator::new(TRANSLATION_ALLOCATOR_SIZE));

static MODEL_MANAGER: Mutex<BTreeMap<InternedString, Arc<Model>>> = Mutex::new(BTreeMap::new());

pub static LAST_TRANSLATED_OPCODE: AtomicU32 = AtomicU32::new(0);

pub fn register_model(name: InternedString, model: Model) {
    log::info!("registering {name:?} ISA model");
    let model = Arc::new(model);
    MODEL_MANAGER.lock().insert(name.to_owned(), model.clone());
}

pub fn get(name: &str) -> Option<Arc<Model>> {
    MODEL_MANAGER
        .lock()
        .get(&InternedString::from(name))
        .cloned()
}

pub fn load_all<FS: Filesystem>(fs: &mut FS) {
    log::info!("loading models");

    // todo: don't hardcode this, load all .postcards?
    ["aarch64.postcard"]
        .into_iter()
        .map(|path| {
            (
                InternedString::from(path.strip_suffix(".postcard").unwrap()),
                fs.read_to_vec(path).unwrap(),
            )
        })
        .map(|(name, data)| (name, postcard::from_bytes::<Model>(&data).unwrap()))
        .for_each(|(name, mut model)| {
            model.registers_mut().iter_mut().for_each(
                |(name, RegisterDescriptor { cache, .. })| *cache = register_cache_type(*name),
            );
            register_model(name, model);
        });
}

pub struct WellKnownRegisters {
    pc: WellKnownRegister<u64>,
    i: WellKnownRegister<bool>,
}

impl WellKnownRegisters {
    pub fn pc(&self) -> WellKnownRegister<u64> {
        self.pc
    }

    pub fn i(&self) -> WellKnownRegister<bool> {
        self.i
    }
}

pub struct ModelDevice {
    name: String,
    pub model: Arc<Model>,
    pub register_file: RegisterFile,
    pub well_known_registers: WellKnownRegisters,
}

impl Debug for ModelDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ModelDevice({})", self.name)
    }
}

impl Device for ModelDevice {
    fn start(&self) {
        self.block_exec(SINGLE_STEP);
        unreachable!("execution should never terminate here")
    }

    fn stop(&self) {
        todo!()
    }
}

impl ModelDevice {
    pub fn new(name: String, model: Arc<Model>, initial_pc: u64) -> Self {
        let register_file = RegisterFile::init(&*model);
        let well_known_registers = WellKnownRegisters {
            pc: register_file.as_wellknown::<u64>("_PC"),
            i: register_file.as_wellknown::<bool>("PSTATE_I"),
        };

        // interpret(
        //     &model,
        //     "__SetConfig",
        //     &[
        //         Value::String("cpu.cpu0.RVBAR".into()),
        //         Value::UnsignedInteger {
        //             value: 0x8000_0000,
        //             width: 64,
        //         },
        //     ],
        //     register_file.as_mut_ptr(),
        // );
        // interpret(
        //     &model,
        //     "__SetConfig",
        //     &[
        //         Value::String("cpu.has_tlb".into()),
        //         Value::UnsignedInteger {
        //             value: 0x0,
        //             width: 64,
        //         },
        //     ],
        //     register_file.as_mut_ptr(),
        // );
        // // from boot.sh command line args to `armv9` binary
        // u__SetConfig(&mut state, &NoopTracer, "cpu.cpu0.RVBAR", 0x8000_0000);
        // u__SetConfig(&mut state, &NoopTracer, "cpu.has_tlb", 0x0);

        register_file.write("_PC", initial_pc);

        Self {
            name,
            model,
            register_file,
            well_known_registers,
        }
    }

    fn block_exec(&self, single_step_mode: bool) {
        // guest physical PC to translated block cache
        let mut block_cache = HashMap::<u64, TranslatedBlock>::default();

        // guest virtual address PC to host virtual ptr to executable code
        let mut chain_cache = DirectMappedCache::<CHAIN_CACHE_ENTRY_COUNT, *const u8>::new(1);

        // virtual to physical PCs
        let mut translation_cache = DirectMappedCache::<1024, u64>::new(1);

        let _status = record_safepoint();

        log::debug!(
            "after record safepoint: status: {}, guest PC: {:#018x}, ELR_EL1: {:#018x}, FAR_EL1: {:#018x}",
            _status,
            self.well_known_registers.pc().read(),
            self.register_file.read::<u64>("ELR_EL1"),
            self.register_file.read::<u64>("FAR_EL1"),
        );

        // block translation/execution loop
        loop {
            // // only trace userspace
            // tracing::ENABLED.store(
            //     self.register_file.read::<u8>("PSTATE_EL") == 0,
            //     Ordering::Relaxed,
            // );

            let block_start_virtual_pc = self.well_known_registers.pc().read();

            let block_start_physical_pc =
                if let Some(pc) = translation_cache.get(block_start_virtual_pc as usize) {
                    pc
                } else {
                    let pc = guest_translate(self, block_start_virtual_pc, TranslationType::Fetch);
                    translation_cache.insert(block_start_virtual_pc as usize, pc);
                    pc
                };

            let translated_block =
                block_cache
                    .entry(block_start_physical_pc)
                    .or_insert_with(|| {
                        BUMP_ALLOCATOR.clear();
                        self.translate_guest_block(
                            BumpAllocatorRef::new(&BUMP_ALLOCATOR),
                            chain_cache.table as u64,
                            block_start_virtual_pc,
                            single_step_mode,
                        )
                    });

            // block_freq_hist
            //     .entry(block_start_virtual_pc)
            //     .and_modify(|(freq, _)| *freq += 1)
            //     .or_insert_with(|| (1, translated_block.translation.code.len()));
            // if instructions_executed > 50_000_000 {
            //     block_freq_hist
            //         .into_iter()
            //         .sorted_by_key(|(_, (_, size))| *size)
            //         .sorted_by_key(|(_, (freq, _))| *freq)
            //         .for_each(|(addr, (freq, size))| {
            //             crate::println!("{addr:x}: {freq} ({})", bytes(size))
            //         });
            //     panic!()
            // }

            if CHAIN_CACHE_ENABLED {
                chain_cache.insert(
                    block_start_virtual_pc as usize,
                    translated_block.translation.as_ptr(),
                );
            }

            // if block_start_virtual_pc == 0x43b670 {
            //     log::error!("get_meta");
            // }

            // if HIT_USERSPACE.load(Ordering::Relaxed) {

            //     // writeln!(transport,
            //     // "{block_start_virtual_pc:#018x}").unwrap();
            // }

            // if GLOBAL_CLOCK.now() > Nanoseconds::new(7u64 * 1_000_000_000) {
            //     tracing::ENABLED.store(true, Ordering::Relaxed);
            // }

            let before = INSTRUCTION_COUNT.load(Ordering::Relaxed);

            log::debug!(
                "executing {block_start_virtual_pc:#08x} ({block_start_physical_pc:#08x}): {:08x?} (instr {before})",
                translated_block.opcodes,
            );

            // LAST_EXECUTED_OPCODE.store(
            //     *translated_block.opcodes.first().unwrap(),
            //     Ordering::Relaxed,
            // );

            let exec_result = translated_block.translation.execute(&self.register_file);

            if EMIT_TRACING {
                INSTRUCTION_COUNT
                    .fetch_add(translated_block.opcodes.len() as u64, Ordering::Relaxed);
            }

            // // deterministic clock
            // {
            //     let guest = get_current_guest();
            //     let timer = guest.timer.clone().unwrap();
            //     timer.tick(Nanoseconds::new(
            //         (INSTRUCTION_COUNT.load(Ordering::Relaxed) - before) * 50,
            //     ));
            // }

            if exec_result.need_tlb_invalidate() {
                chain_cache.fill_keys(1);
                translation_cache.fill_keys(1);
                VirtualMemoryArea::current().invalidate_guest_mappings();
            }

            if exec_result.need_code_cache_flush() {
                chain_cache.fill_keys(1);
                block_cache.clear();
                translation_cache.fill_keys(1);
            }

            if exec_result.interrupt_pending() {
                let masked = self.well_known_registers.i().read();

                if !masked {
                    let pc = self.well_known_registers.pc().read();
                    log::trace!("interrupt pending @ {pc:x}, masked: {masked}");
                    take_arm_exception(self, 1, 255, 0, 0, pc, 0x80);
                    let pc = self.well_known_registers.pc().read();
                    log::trace!("took arm exception to {pc:x}");
                } else {
                    log::trace!("masked interrupt pending");
                }
            }
        }
    }

    fn translate_guest_block(
        &self,
        allocator: BumpAllocatorRef,
        chain_cache: u64,
        block_start_pc: u64,
        single_step_mode: bool,
    ) -> TranslatedBlock {
        let mut ctx = X86TranslationContext::new_with_allocator(
            allocator,
            &self.model,
            true,
            self.register_file.global_register_offset(),
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

        let mut current_pc = block_start_pc;

        let mut opcodes = Vec::new();

        // block prologue
        emitter.prologue();

        // reset BranchTaken
        let _false = emitter.constant(0 as u64, Type::Unsigned(1));
        emitter.write_register(self.model.reg_offset("__BranchTaken") as u64, _false);

        // instruction translation loop
        let was_end_of_block = loop {
            // read opcode
            let opcode = unsafe { *((current_pc & 0xFF_FFFF_FFFF) as *const u32) };

            log::debug!("translating {opcode:#08x} @ {current_pc:#08x}");
            log::debug!("{}", disarm64::decoder::decode(opcode).unwrap());

            opcodes.push(opcode);

            LAST_TRANSLATED_OPCODE.store(opcode, Ordering::Relaxed);

            let _return_value = translate_instruction(
                &*self.model,
                "__DecodeA64",
                &mut emitter,
                &self.register_file,
                opcode,
                current_pc,
            )
            .unwrap();

            // hit a maybe-PC modifying instruction
            if emitter.ctx().get_pc_write_flag() {
                // end of block
                break true;
            } else {
                // emit code to increment PC register by 4
                let pc_offset = self.model.reg_offset("_PC");
                let pc = emitter.read_register(pc_offset as u64, Type::Unsigned(64));
                let _4 = emitter.constant(4, Type::Unsigned(64));
                let pc_inc = emitter.binary_operation(BinaryOperationKind::Add(pc, _4));
                emitter.write_register(pc_offset as u64, pc_inc);

                // increase our local pc by 4
                current_pc += 4;

                // did we cross a page boundary?
                if current_pc & !0xFFF != block_start_pc & !0xFFF {
                    break false;
                }
            }

            // if we have a TLB invalidation or other non-zero status in that instruction,
            // do not translate the rest of the block
            if emitter.execution_result.need_tlb_invalidate()
                || emitter.execution_result.need_code_cache_flush()
            {
                break false;
            }

            // only translate single instruction in single_step_mode
            if single_step_mode {
                break false;
            }
        };

        // if we didn't jump anywhere at the end of the block (IE. branch was not
        // taken), increment PC by 4 bytes
        if was_end_of_block {
            let branch_taken = emitter.read_register(
                self.model.reg_offset("__BranchTaken") as u64,
                Type::Unsigned(1),
            );

            let _0 = emitter.constant(0, Type::Unsigned(64));
            let _4 = emitter.constant(4, Type::Unsigned(64));
            let addend = emitter.select(branch_taken, _0, _4);

            let pc_offset = self.model.reg_offset("_PC");
            let pc = emitter.read_register(pc_offset, Type::Unsigned(64));
            let new_pc = emitter.binary_operation(BinaryOperationKind::Add(pc, addend));
            emitter.write_register(pc_offset, new_pc);
        }

        log::trace!("compiling");
        emitter.leave_with_cache(chain_cache);
        let num_regs = emitter.next_vreg();

        let translation = Translation::new(ctx.compile(num_regs));

        // if block_start_pc == 0xffffffc00811c584 {
        //     log::error!("WARNING! Large block @ {block_start_pc:x}");

        //     log::error!("INPUT ASM:");
        //     for opcode in &opcodes {
        //         log::error!("  {}", disarm64::decoder::decode(*opcode).unwrap());
        //     }

        //     log::error!("\nOUTPUT ASM:");
        //     log::error!("{translation:?}");
        //     panic!();
        // }

        log::trace!("finished");

        TranslatedBlock {
            translation,
            opcodes,
        }
    }
}

pub struct TranslatedBlock {
    translation: Translation,
    opcodes: Vec<u32>,
}

fn register_cache_type(name: InternedString) -> RegisterCacheType {
    if name.as_ref() == "FeatureImpl"
        || name.as_ref().ends_with("IMPLEMENTED")
        || name.as_ref() == "EL0"
        || name.as_ref() == "EL1"
        || name.as_ref() == "EL2"
        || name.as_ref() == "EL3"
        || name.as_ref() == "MPAMIDR_EL1_bits"
    {
        RegisterCacheType::Constant
    } else if name.as_ref() == "SEE"
        || name.as_ref() == "have_exception"
        || name.as_ref().starts_with("current_exception")
    {
        RegisterCacheType::ReadWrite
    } else if name.as_ref().starts_with("SPE")
        || [
            "PSTATE_EL",
            "_MPAM1_EL1_bits",
            "_MPAM3_EL3_bits",
            "_MPAM3_EL3_bits",
            "MPAM0_EL1_bits",
            "MPAM2_EL2_bits", // not a bug, the underscores are correct
            "SCR_EL3_bits",
            "CPTR_EL2_bits",
            "CPTR_EL3_bits",
            "_EDSCR_bits",
            "MDCCSR_EL0_bits",
            "SMCR_EL1_bits",
            "SMCR_EL2_bits",
            "SMCR_EL3_bits",
            "SCTLR_EL1_bits",
            "SCTLR_EL2_bits",
            "SCTLR_EL3_bits",
            "CPACR_EL1_bits",
        ]
        .contains(&name.as_ref())
    {
        RegisterCacheType::Read
    } else {
        RegisterCacheType::None
    }
}

#[repr(C)]
struct ChainCacheEntry<V> {
    key: usize,
    value: V,
}

#[repr(C)]
struct DirectMappedCache<const N: usize, V> {
    table: *mut ChainCacheEntry<V>,
}

impl<const N: usize, V: Copy> DirectMappedCache<N, V> {
    pub fn new(initial_keys: usize) -> Self {
        let ptr = unsafe {
            alloc_zeroed(
                Layout::from_size_align(
                    N * size_of::<ChainCacheEntry<V>>(),
                    Size4KiB::SIZE.try_into().unwrap(),
                )
                .unwrap(),
            )
        };

        let mut celf = Self {
            table: ptr as *mut ChainCacheEntry<V>,
        };

        celf.fill_keys(initial_keys);

        celf
    }

    pub fn insert(&mut self, key: usize, value: V) {
        self.table()[Self::index(key)] = ChainCacheEntry { key, value };
    }

    pub fn get(&mut self, key: usize) -> Option<V> {
        let entry = &self.table()[Self::index(key)];

        if entry.key == key {
            Some(entry.value)
        } else {
            None
        }
    }

    fn index(key: usize) -> usize {
        (key >> 2) & (N - 1)
    }

    fn table(&mut self) -> &mut [ChainCacheEntry<V>] {
        unsafe { core::slice::from_raw_parts_mut(self.table, N) }
    }

    pub fn fill_keys(&mut self, key: usize) {
        self.table().iter_mut().for_each(|e| e.key = key);
    }
}

pub extern "sysv64" fn svc_debug(value: u64) {
    log::error!("SVC: {value}");
}

pub extern "sysv64" fn prelude_debug(pc: u64, opcode: u64) {
    static ENABLED: AtomicBool = AtomicBool::new(false);

    if pc == 0x400008d0 {
        ENABLED.store(true, Ordering::Relaxed);
    }

    if ENABLED.load(Ordering::Relaxed) {
        log::error!(
            "{pc:#x}: opcode: {opcode:x}, EL: {}",
            get_current_guest()
                .core
                .register_file
                .read::<u8>("PSTATE_EL")
        )
    }
}

pub extern "sysv64" fn write_to_el(old: u8, new: u8) {
    if old != new {
        log::debug!("EL changed! {old} -> {new}");
        // chain_cache.fill_keys(1);
        // translation_cache.fill_keys(1);
        VirtualMemoryArea::current().invalidate_guest_mappings();
    }
}
