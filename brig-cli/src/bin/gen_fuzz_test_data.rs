use common::fuzz_test::InstructionFuzzTest;

const DATA: &[(u32, usize, [u64; 31], [u64; 31])] = include!("const.txt");

fn main() {
    let tests: Vec<InstructionFuzzTest> = DATA
        .into_iter()
        .copied()
        .map(
            |(instruction, test_number, initial_state, post_state)| InstructionFuzzTest {
                test_number,
                instruction,
                initial_state,
                post_state,
            },
        )
        .collect();

    let contents = postcard::to_allocvec(&tests).unwrap();

    std::fs::write("fuzz_tests.postcard", contents).unwrap();
}
