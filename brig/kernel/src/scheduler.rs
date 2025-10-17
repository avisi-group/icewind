use {
    crate::{
        {arch::CoreStorage, devices::lapic::LAPIC},
        tasks::{Task, TaskControlBlock, create_idle_task},
    },
    alloc::collections::LinkedList,
    log::trace,
    x86::current::segmentation::wrgsbase,
};

pub const TIMER_FREQUENCY: u32 = 1000;

pub struct Scheduler {
    run_queue: LinkedList<Task>,
    idle_task: Task,
}

impl Scheduler {
    pub fn new() -> Self {
        let idle = create_idle_task();
        Self::activate(&idle);
        Self {
            run_queue: LinkedList::new(),
            idle_task: idle,
        }
    }

    pub fn get_local_mut() -> &'static mut Self {
        CoreStorage::this_mut().get::<Self>().unwrap()
    }

    pub fn add_to_runqueue(&mut self, task: Task) {
        self.run_queue.push_back(task);
    }

    pub fn remove_from_runqueue(&mut self, task: &Task) {
        self.run_queue = self
            .run_queue
            .clone()
            .into_iter()
            .filter(|t| t != task)
            .collect();
    }

    pub fn activate(t: &Task) {
        // TODO: cannot be called remotely
        // TODO: abstract to arch-specific
        unsafe {
            wrgsbase(t.get_tcb() as *const TaskControlBlock as u64);
        }
    }
}

pub fn local_run() -> ! {
    trace!("scheduler started");

    LAPIC.get().unwrap().lock().start_periodic(TIMER_FREQUENCY);

    // The idle task is active at this point, and because its RFLAGS
    // enables interrupts, the following iretq will also enable
    // interrupts, and therefore start the scheduler.
    unsafe {
        core::arch::asm!(
            "
            mov %gs:0, %rsp
            pop %gs:0
            pop %r15
            pop %r14
            pop %r13
            pop %r12
            pop %r11
            pop %r10
            pop %r9
            pop %r8
            pop %rdi
            pop %rsi
            pop %rbp
            pop %rbx
            pop %rdx
            pop %rcx
            pop %rax
            add $8, %rsp
            iretq
            ",
            options(att_syntax, noreturn)
        );
    }
}

pub fn schedule() {
    // TODO: Fix and think about this deadlock -- maybe need to disable local
    // interrupts when adding/removing from runqueue?
    let scheduler = Scheduler::get_local_mut();

    // TODO: some sort of priority queue based on vruntimes

    if scheduler.run_queue.is_empty() {
        Scheduler::activate(&scheduler.idle_task);
    } else {
        let next = if scheduler.run_queue.len() == 1 {
            scheduler.run_queue.front().unwrap()
        } else {
            let front = scheduler.run_queue.pop_front().unwrap();

            scheduler.run_queue.push_back(front);

            scheduler.run_queue.back().unwrap()
        };

        Scheduler::activate(next);
    }
}
