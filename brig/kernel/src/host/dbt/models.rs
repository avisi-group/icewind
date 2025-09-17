use {
    crate::{
        host::{
            arch::x86::{
                aarch64_mmu::{self, TranslationType, take_arm_exception},
                memory::VirtualMemoryArea,
                safepoint::record_safepoint,
            },
            dbt::{
                Alloc, Translation,
                emitter::{Emitter, Type},
                register_file::{RegisterFile, WellKnownRegister},
                translate::translate_instruction,
                x86::{
                    X86TranslationContext,
                    emitter::{BinaryOperationKind, X86Emitter},
                },
            },
            devices::manager::SharedDeviceManager,
            fs::Filesystem,
            memory::bump::{BumpAllocator, BumpAllocatorRef},
            objects::{
                Object, ObjectId, ObjectStore, ToIrqController, ToMemoryMappedDevice,
                ToRegisterMappedDevice, ToTickable, device::Device,
            },
        },
        util::{get_current_device, parse_hex_prefix},
    },
    alloc::{
        alloc::alloc_zeroed,
        borrow::ToOwned,
        collections::btree_map::BTreeMap,
        string::{String, ToString},
        sync::Arc,
        vec::Vec,
    },
    common::{
        hashmap::HashMap,
        intern::InternedString,
        rudder::{Model, RegisterCacheType, RegisterDescriptor},
    },
    core::{
        alloc::Layout,
        fmt::{self, Debug, Write},
        sync::atomic::{AtomicBool, AtomicU32, Ordering},
    },
    proc_macro_lib::guest_device_factory,
    spin::Mutex,
    x86_64::structures::paging::{PageSize, Size4KiB},
};

/// Size in bytes for the per-translation bump allocator
const TRANSLATION_ALLOCATOR_SIZE: usize = 4 * 1024 * 1024 * 1024;

/// Limit blocks to contain only 1 instruction
const SINGLE_STEP: bool = true;

/// Enable the jump table chain cache
const CHAIN_CACHE_ENABLED: bool = false;
pub const CHAIN_CACHE_ENTRY_COUNT: usize = 65536;
const _: () = assert!(CHAIN_CACHE_ENTRY_COUNT.is_power_of_two());

static HIT_USERSPACE: AtomicBool = AtomicBool::new(false);

static MODEL_MANAGER: Mutex<BTreeMap<InternedString, Arc<Model>>> = Mutex::new(BTreeMap::new());

pub static LAST_TRANSLATED_OPCODE: AtomicU32 = AtomicU32::new(0);
pub static LAST_EXECUTED_OPCODE: AtomicU32 = AtomicU32::new(0);

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

#[guest_device_factory(core)]
fn create_core(config: &BTreeMap<InternedString, InternedString>) -> Arc<dyn Device> {
    let model_name = config.get(&InternedString::from_static("model")).unwrap();
    let model = get(model_name.as_ref()).unwrap();
    let initial_pc = config
        .get(&InternedString::from_static("initial_pc"))
        .map(parse_hex_prefix)
        .unwrap()
        .unwrap();

    Arc::new(ModelDevice::new(model_name.to_string(), model, initial_pc))
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
    id: ObjectId,
    name: String,
    model: Arc<Model>,
    pub register_file: RegisterFile,
    pub well_known_registers: WellKnownRegisters,
}

impl Debug for ModelDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ModelDevice({})", self.name)
    }
}

impl Object for ModelDevice {
    fn id(&self) -> ObjectId {
        self.id
    }
}

impl ToTickable for ModelDevice {}
impl ToRegisterMappedDevice for ModelDevice {}
impl ToMemoryMappedDevice for ModelDevice {}
impl ToIrqController for ModelDevice {}

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
            id: ObjectId::new(),
            name,
            model,
            register_file,
            well_known_registers,
        }
    }

    fn block_exec(&self, single_step_mode: bool) {
        let shared = SharedDeviceManager::get()
            .get_device_by_alias("transport00:05.0")
            .unwrap();
        let crate::host::devices::Device::Transport(transport) = &mut *shared.lock() else {
            panic!();
        };

        let mut instructions_executed = 0usize;

        // guest physical PC to translated block cache
        let mut block_cache = HashMap::<u64, TranslatedBlock>::default();

        // guest virtual address
        let mut chain_cache = DirectMappedCache::<CHAIN_CACHE_ENTRY_COUNT, *const u8>::new(1);

        // virtual to physical PCs
        let mut translation_cache = DirectMappedCache::<1024, u64>::new(1);

        let mut allocator = BumpAllocator::new(TRANSLATION_ALLOCATOR_SIZE);

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
            let block_start_virtual_pc = self.well_known_registers.pc().read(); // self.register_file.read::<u64>("_PC");

            let block_start_physical_pc =
                if let Some(pc) = translation_cache.get(block_start_virtual_pc as usize) {
                    pc
                } else {
                    let pc = aarch64_mmu::guest_translate(
                        self,
                        block_start_virtual_pc,
                        TranslationType::Fetch,
                    );

                    translation_cache.insert(block_start_virtual_pc as usize, pc);
                    pc
                };

            let translated_block =
                block_cache
                    .entry(block_start_physical_pc)
                    .or_insert_with(|| {
                        allocator.clear();
                        self.translate_block(
                            BumpAllocatorRef::new(&allocator),
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

            instructions_executed += translated_block.opcodes.len();

            // if block_start_virtual_pc == 0x43b670 {
            //     log::error!("get_meta");
            // }

            // if HIT_USERSPACE.load(Ordering::Relaxed) {

            //     // writeln!(transport,
            //     // "{block_start_virtual_pc:#018x}").unwrap();
            // }

            log::debug!(
                "executing {block_start_virtual_pc:#08x} ({block_start_physical_pc:#08x}): {:08x?} (instr {instructions_executed})",
                translated_block.opcodes,
            );

            // LAST_EXECUTED_OPCODE.store(
            //     *translated_block.opcodes.first().unwrap(),
            //     Ordering::Relaxed,
            // );

            let exec_result = translated_block.translation.execute(&self.register_file);

            if exec_result.need_tlb_invalidate() {
                chain_cache.fill_keys(1);
                translation_cache.fill_keys(1);
                VirtualMemoryArea::current().invalidate_guest_mappings();
            }

            if exec_result.interrupt_pending() {
                let masked = self.well_known_registers.i().read(); //self.register_file.read::<bool>("PSTATE_I");

                if !masked {
                    let pc = self.well_known_registers.pc().read();
                    // log::error!("interrupt pending @ {pc:x}, masked: {masked}");
                    take_arm_exception(self, 1, 255, 0, 0, pc, 0x80);
                    //let pc = self.well_known_registers.pc().read();
                    //log::error!("took arm exception to {pc:x}");
                } else {
                    //   log::error!("masked interrupt pending");
                }
            }
        }
    }

    fn translate_block<A: Alloc>(
        &self,
        allocator: A,
        chain_cache: u64,
        block_start_pc: u64,
        single_step_mode: bool,
    ) -> TranslatedBlock {
        let mut ctx = X86TranslationContext::new_with_allocator(
            allocator,
            &self.model,
            true,
            self.register_file.global_register_offset(),
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

            //#[cfg(feature = "debug_translation")]
            opcodes.push(opcode);

            LAST_TRANSLATED_OPCODE.store(opcode, Ordering::Relaxed);

            let _return_value = translate_instruction(
                allocator,
                &*self.model,
                "__DecodeA64",
                &mut emitter,
                &self.register_file,
                opcode,
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
            if emitter.execution_result.need_tlb_invalidate() {
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

        let translation = ctx.compile(num_regs);

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
    } else if name.as_ref() == "PSTATE_EL"
        || name.as_ref().starts_with("SPE")
        || name.as_ref() == "_MPAM3_EL3_bits"
        || name.as_ref() == "_MPAM2_EL2_bits"
    //     || name.as_ref() == "SCR_EL3_bits" // todo: re-enable me
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

pub extern "C" fn write_to_el(old: u8, new: u8) {
    if old != new {
        log::debug!("EL changed! {old} -> {new}");
        // chain_cache.fill_keys(1);
        // translation_cache.fill_keys(1);
        VirtualMemoryArea::current().invalidate_guest_mappings();
    }
}

pub extern "C" fn svc_debug(value: u64) {
    log::error!("SVC: {value}");
}

pub extern "C" fn prelude_debug(pc: u64, opcode: u64) {
    static ENABLED: AtomicBool = AtomicBool::new(false);

    if pc == 0x400008d0 {
        ENABLED.store(true, Ordering::Relaxed);
    }

    if ENABLED.load(Ordering::Relaxed) {
        log::error!(
            "{pc:#x}: opcode: {opcode:x}, EL: {}",
            get_current_device().register_file.read::<u8>("PSTATE_EL")
        )
    }
}
