// Copyright 2021 Amazon.com, Inc. or its affiliates. All Rights Reserved.
//
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause
//
// COPIED FROM https://raw.githubusercontent.com/rust-vmm/vm-virtio/refs/heads/main/virtio-queue/src/defs.rs

//! Virtio queue related constant definitions

/// Size of used ring header: flags (u16) + idx (u16)
pub const VIRTQ_USED_RING_HEADER_SIZE: u64 = 4;

/// Size of the used ring metadata: header + avail_event (le16).
///
/// The total size of the used ring is:
/// VIRTQ_USED_RING_META_SIZE + VIRTQ_USED_ELEMENT_SIZE * queue_size.
pub const VIRTQ_USED_RING_META_SIZE: u64 = VIRTQ_USED_RING_HEADER_SIZE + 2;

/// Size of one element in the used ring, id (le32) + len (le32).
pub const VIRTQ_USED_ELEMENT_SIZE: u64 = 8;

/// Size of available ring header: flags(u16) + idx(u16)
pub const VIRTQ_AVAIL_RING_HEADER_SIZE: u64 = 4;

/// Size of the available ring metadata: header + used_event (le16).
///
/// The total size of the available ring is:
/// VIRTQ_AVAIL_RING_META_SIZE + VIRTQ_AVAIL_ELEMENT_SIZE * queue_size.
pub const VIRTQ_AVAIL_RING_META_SIZE: u64 = VIRTQ_AVAIL_RING_HEADER_SIZE + 2;

/// Size of one element in the available ring (le16).
pub const VIRTQ_AVAIL_ELEMENT_SIZE: u64 = 2;

/// Default guest physical address for descriptor table.
pub const DEFAULT_DESC_TABLE_ADDR: u64 = 0x0;

/// Default guest physical address for available ring.
pub const DEFAULT_AVAIL_RING_ADDR: u64 = 0x0;

/// Default guest physical address for used ring.
pub const DEFAULT_USED_RING_ADDR: u64 = 0x0;

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct BlkReq {
    pub typ: BlkReqType,
    pub ioprio: u32,
    pub sector: u64,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum BlkReqType {
    In = 0,
    Out = 1,
    Flush = 4,
    GetId = 8,
    GetLifetime = 10,
    Discard = 11,
    WriteZeroes = 13,
    SecureErase = 14,
}
