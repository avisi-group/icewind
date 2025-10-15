#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InstructionFuzzTest {
    pub test_number: usize,
    pub instruction: u32,
    pub initial_state: [u64; 31],
    pub post_state: [u64; 31],
}
