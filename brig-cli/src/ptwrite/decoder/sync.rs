use std::{mem::size_of, ops::Range};

const PT_OPC_PSB: u8 = 0x02;
const PT_EXT_PSB: u8 = 0x82;

/// The high and low bytes in the pattern
pub const PT_PSB_HI: u8 = PT_OPC_PSB;
pub const PT_PSB_LO: u8 = PT_EXT_PSB;

/// Various combinations of the above parts
const PT_PSB_LOHI: u16 = (PT_PSB_LO as u16) | (PT_PSB_HI as u16) << 8;
const PT_PSB_HILO: u16 = (PT_PSB_HI as u16) | (PT_PSB_LO as u16) << 8;

/// The repeat count of the payload, not including opc and ext.
pub const PT_PSB_REPEAT_COUNT: usize = 7;

/// The size of the repeated pattern in bytes.
pub const PT_PSB_REPEAT_SIZE: usize = 2;

/* The size of a PSB packet's payload in bytes. */
pub const PT_PL_PSB_SIZE: usize = PT_PSB_REPEAT_COUNT * PT_PSB_REPEAT_SIZE;

pub const PT_OPCS_PSB: usize = 2;

pub const PTPS_PSB: usize = PT_OPCS_PSB + PT_PL_PSB_SIZE;

/// A psb packet contains a unique 2-byte repeating pattern, there are only two
/// ways to fill up a u64 with such a pattern.
pub const PSB_PATTERNS: [u64; 2] = [
    ((PT_PSB_LOHI as u64)
        | (PT_PSB_LOHI as u64) << 16
        | (PT_PSB_LOHI as u64) << 32
        | (PT_PSB_LOHI as u64) << 48),
    ((PT_PSB_HILO as u64)
        | (PT_PSB_HILO as u64) << 16
        | (PT_PSB_HILO as u64) << 32
        | (PT_PSB_HILO as u64) << 48),
];

pub enum ParseError {
    /// Failed to parse slice as no sync points were found
    NoSync,
    /// Failed to parse slice as only a single sync point ({0:?}) was found
    OneSync(usize),
}

/// Finds the syncpoints in the supplied slice, returned as a range
/// containing up to `MAX_SYNCPOINTS` syncpoints
pub fn find_sync_range(slice: &[u8], max: usize) -> Result<Range<usize>, ParseError> {
    //let start_time = std::time::Instant::now();
    let Some(start) = find_next_sync(slice) else {
        // slice did not contain any sync points
        return Err(ParseError::NoSync);
    };

    let mut last_syncpoint = start;
    let mut syncpoint_count = 1;

    loop {
        if syncpoint_count == max {
            break;
        }

        if last_syncpoint + 1 >= slice.len() {
            break;
        }

        match find_next_sync(&slice[last_syncpoint + 1..]) {
            Some(offset) => {
                syncpoint_count += 1;
                last_syncpoint += offset + 1;
            }
            None => break,
        }
    }

    // println!(
    //     "took {}ns",
    //     (std::time::Instant::now() - start_time).as_nanos()
    // );

    if syncpoint_count == 1 {
        // slice only contained a single sync point
        return Err(ParseError::OneSync(start));
    }

    Ok(start..last_syncpoint)
}

pub fn find_next_sync(buf: &[u8]) -> Option<usize> {
    buf.chunks_exact(size_of::<u64>())
        .enumerate()
        .find_map(|(chunk_count, chunk)| {
            // test against the two possible patterns
            if *chunk != PSB_PATTERNS[0].to_ne_bytes() && *chunk != PSB_PATTERNS[1].to_ne_bytes() {
                return None;
            }

            // calculate index of the start of the chunk
            let mut chunk_start_index = chunk_count * 8;

            // adjust if we are not aligned to a PT_PSB_HI
            if buf[chunk_start_index] != PT_PSB_HI {
                chunk_start_index += 1;
            }

            // search forwards to find the end of the PSB packet, then subtract `PTPS_PSB`
            // to get the index of the start of the packet
            buf[chunk_start_index..]
                // check HI-LO byte pairs
                .chunks(2)
                .enumerate()
                // skip the first 3 as those would still be in chunk
                .skip(3)
                // find the first pair that doesn't match
                .find(|(_, chunk)| **chunk != [PT_PSB_HI, PT_PSB_LO])
                // calculate the index
                .map(|(count, _)| count * 2 + chunk_start_index)
                // return None if the end index is less than the size of a PSB packet (indicates
                // `buf` is too small)
                .filter(|psb_end_index| *psb_end_index >= PTPS_PSB)
                // return the start of the PSB packet
                .map(|psb_end_index| psb_end_index - PTPS_PSB)
        })
}

pub fn find_next_sync_simple(buf: &[u8]) -> Option<usize> {
    let mut data = [0u8; 16];
    for i in 0..buf.len() - 16 {
        if buf[i] == PT_OPC_PSB {
            data.copy_from_slice(&buf[i..i + 16]);
            if u64::from_ne_bytes(data[0..8].try_into().unwrap()) == PSB_PATTERNS[1]
                && u64::from_ne_bytes(data[8..16].try_into().unwrap()) == PSB_PATTERNS[1]
            {
                return Some(i);
            }
        }
    }

    None
}

pub fn find_next_sync_unsafe(data: &[u8]) -> Option<usize> {
    unsafe {
        let start = data.as_ptr();
        let end = start.add(data.len());

        let ptr = start;

        let mut ptr = ptr.add(ptr.align_offset(8)).cast::<u64>();

        // println!(
        //     "ptr: {:p}, start: {:p}, end: {:p}, len: {:x}",
        //     ptr,
        //     start,
        //     end,
        //     data.len()
        // );

        // todo: end-8
        while ptr < end as *const u64 {
            if *ptr == 0x8202820282028202u64 || *ptr == 0x0282028202820282 {
                //println!("found pattern @ {:p}", ptr);

                let mut bptr = if *(ptr as *const u8) == 0x82 {
                    (ptr as *const u8).add(7) as *const u16
                } else {
                    (ptr as *const u8).add(8) as *const u16
                };

                while bptr < end as *const u16 {
                    //  println!("checking {:p}", bptr);
                    if *bptr != 0x8202 {
                        //println!("not found {:p} {:p}", bptr, start);

                        if *bptr.offset(-8) != 0x8202 {
                            return None;
                        }

                        let slice_offset = bptr as usize - start as usize - 16;
                        return Some(slice_offset);
                    }

                    bptr = bptr.add(1);
                }
            }

            ptr = ptr.add(1);
        }

        None
    }
}

// /// Finds the index of the next PSB, if it exists
// pub fn find_next_sync_ipt(slice: &[u8]) -> Option<usize> {
//     let mut_slice =
//         unsafe { std::slice::from_raw_parts_mut(slice.as_ptr() as usize as
// *mut _, slice.len()) };

//     let mut decoder =
// PacketDecoder::new(&ConfigBuilder::new(mut_slice).unwrap().finish()).
// unwrap();

//     decoder
//         .sync_forward()
//         .ok()
//         .map(|_| decoder.sync_offset().unwrap() as usize)
// }

// #[cfg(test)]
// mod tests {
//     use super::{find_next_sync, find_next_sync_ipt, find_next_sync_simple,
// find_next_sync_unsafe};

//     fn harness<A: Fn(&[u8]) -> Option<usize>, B: Fn(&[u8]) ->
// Option<usize>>(a: A, b: B) {         let mut data =
// &include_bytes!("../../ptdata.raw.trunc")[..];

//         loop {
//             let a_res = a(data);
//             let b_res = b(data);
//             assert_eq!(a_res, b_res);

//             let Some(next) = a_res else {
//                 break;
//             };

//             data = &data[next + 16..];
//         }
//     }

//     #[test]
//     fn simple() {
//         harness(find_next_sync_ipt, find_next_sync_simple);
//     }

//     #[test]
//     fn iter() {
//         harness(find_next_sync_ipt, find_next_sync);
//     }

//     #[test]
//     fn notsafe() {
//         harness(find_next_sync_ipt, find_next_sync_unsafe);
//     }
// }
