use {
    crate::host::dbt::x86::encoder::registers::{
        PhysicalRegister, PhysicalRegisterGeneral, PhysicalRegisterXmm,
    },
    common::hashmap::HashSet,
};

/// Struct for tracking which physical registers are in use at the current time
#[derive(Debug, Clone, Default)]
pub struct PhysicalUsed {
    general: HashSet<PhysicalRegisterGeneral>,
    xmm: HashSet<PhysicalRegisterXmm>,
}

impl PhysicalUsed {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn remove(&mut self, reg: &PhysicalRegister) {
        match reg {
            PhysicalRegister::General(general) => {
                assert!(self.general.remove(general));
            }
            PhysicalRegister::Xmm(xmm) => {
                assert!(self.xmm.remove(xmm));
            }
        }
    }

    pub fn contains(&self, reg: &PhysicalRegister) -> bool {
        match reg {
            PhysicalRegister::General(general) => self.general.contains(general),
            PhysicalRegister::Xmm(xmm) => self.xmm.contains(xmm),
        }
    }

    pub fn insert(&mut self, reg: PhysicalRegister) {
        match reg {
            PhysicalRegister::General(general) => {
                self.general.insert(general);
            }
            PhysicalRegister::Xmm(xmm) => {
                self.xmm.insert(xmm);
            }
        }
    }
}
