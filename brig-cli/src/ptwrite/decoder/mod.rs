pub mod sync;

pub struct Decoder<'data> {
    main: &'data [u8],
    secondary: Option<&'data [u8]>,
    position: usize,
}

impl<'data> Decoder<'data> {
    pub fn new(main: &'data [u8], secondary: Option<&'data [u8]>) -> Self {
        Self {
            main,
            secondary,
            position: 0,
        }
    }

    pub fn sync_forward(&mut self) {
        // match find_next_sync(&self.data[self.current_pos..]) {
        //     Some(n) => self.current_pos = n,
        //     None => panic!("Failed to find sync point"),
        // }
        todo!()
    }

    pub fn offset(&self) -> usize {
        self.position
    }

    pub fn set_offset(&mut self, offset: usize) {
        self.position = offset;
    }

    // step decoder forward by amount
    fn step(&mut self, amount: usize) {
        self.position += amount;
    }

    fn current_slice(&mut self) -> &[u8] {
        &self.main[self.position..]
    }

    fn current_byte(&mut self) -> u8 {
        *unsafe { self.main.get_unchecked(self.position) }
    }

    fn next_packet(&mut self) -> Result<Option<u64>, Error> {
        if self.position >= self.main.len() {
            return Err(Error::EndOfData);
        }

        let opcode = self.current_byte();
        self.step(1);

        match opcode {
            OPCODE_PAD => {
                //println!("pad");
            }
            OPCODE_EXT => {
                let ext = self.current_byte();
                self.step(1);

                match ext {
                    OPCODE_EXT_PSB => {
                        // println!("psb");
                        self.step(14);
                    }
                    OPCODE_EXT_PSBEND => {
                        //println!("psbend");
                    }
                    OPCODE_EXT_CBR => {
                        // println!("cbr");
                        self.step(PT_PL_CBR_SIZE);
                    }
                    0x32 => {
                        // println!("ptw");
                        let payload = if self.position == self.main.len() {
                            if let Some(secondary) = self.secondary {
                                if secondary.len() >= 8 {
                                    u64::from_ne_bytes(secondary[..8].try_into().unwrap())
                                } else {
                                    self.position -= 2;
                                    return Err(Error::MissingData);
                                }
                            } else {
                                // no secondary but saw PTW header, revert and retry next time
                                self.position -= 2;
                                return Err(Error::MissingData);
                            }
                        } else {
                            u64::from_ne_bytes(self.current_slice()[..8].try_into().unwrap())
                        };

                        self.step(8);
                        return Ok(Some(payload));
                    }
                    _ => return Err(Error::UnknownExtOpcode { ext }),
                }
            }

            _ => {
                if (opcode & 0x01) == 0 {
                    //println!("tnt8");
                } else {
                    return Err(Error::UnknownOpcode { opcode });
                }
            }
        }

        Ok(None)
    }
}

impl<'data> Iterator for Decoder<'data> {
    type Item = u64;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.next_packet() {
                Ok(None) => (),
                Ok(Some(payload)) => break Some(payload),
                Err(_) => break None,
            }
        }
    }
}

#[repr(u8)]
enum Opcode {
    Pad = 0x00,
    Ext = 0x02,
    //    Psb = pt_opc_ext, = 0x02
    Tip = 0x0d,
    //    pt_opc_tnt_8 = 0x00,
    TipPge = 0x11,
    TipPgd = 0x01,
    Fup = 0x1d,
    Mode = 0x99,
    Tsc = 0x19,
    Mtc = 0x59,
    Cyc = 0x03,
    Trig = 0xd9,

    /* A free opcode to trigger a decode fault. */
    Bad = 0xc9,
}

// enum pt_ext_code {
// 	pt_ext_psb		= 0x82,
// 	pt_ext_tnt_64		= 0xa3,
// 	pt_ext_pip		= 0x43,
// 	pt_ext_ovf		= 0xf3,
// 	pt_ext_psbend		= 0x23,
// 	pt_ext_cbr		= 0x03,
// 	pt_ext_tma		= 0x73,
// 	pt_ext_stop		= 0x83,
// 	pt_ext_vmcs		= 0xc8,
// 	pt_ext_ext2		= 0xc3,
// 	pt_ext_exstop		= 0x62,
// 	pt_ext_exstop_ip	= 0xe2,
// 	pt_ext_mwait		= 0xc2,
// 	pt_ext_pwre		= 0x22,
// 	pt_ext_pwrx		= 0xa2,
// 	pt_ext_ptw		= 0x12,
// 	pt_ext_cfe		= 0x13,
// 	pt_ext_evd		= 0x53,

// 	pt_ext_bad		= 0x04
// };

const OPCODE_PAD: u8 = 0x00;
const OPCODE_EXT: u8 = 0x02;
const OPCODE_EXT_PSB: u8 = 0x82;
const OPCODE_EXT_PSBEND: u8 = 0x23;
const OPCODE_EXT_CBR: u8 = 0x03;
const OPCODE_EXT_PTW: u8 = 0x12;

const PT_OPCS_CBR: usize = 2;
const PT_PL_CBR_SIZE: usize = 2;
const PTPS_CBR: usize = PT_OPCS_CBR + PT_PL_CBR_SIZE;

const PT_OPC_CYC: u8 = 0x3;

/// Decoder error
#[derive(displaydoc::Display, thiserror::Error, Debug)]
enum Error {
    /// Unknown packet opcode: {opcode:#x}
    UnknownOpcode { opcode: u8 },
    /// Unknown packet ext opcode: {ext:#x}
    UnknownExtOpcode { ext: u8 },
    /// Encountered missing data, decoding will be reverted and can be retried
    MissingData,
    /// Reached end of supplied buffer(s)
    EndOfData,
}
