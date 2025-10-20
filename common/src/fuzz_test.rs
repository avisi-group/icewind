#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InstructionFuzzTest {
    pub test_number: usize,
    pub instruction: u32,
    pub pre_gprs: [u64; 32],
    pub post_gprs: [u64; 32],
    pub pre_fprs: [u128; 32],
    pub post_fprs: [u128; 32],
}
