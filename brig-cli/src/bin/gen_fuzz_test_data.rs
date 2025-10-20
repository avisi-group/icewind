use common::fuzz_test::InstructionFuzzTest;

const SCALAR_DATA: &[(u32, usize, [u64; 32], [u64; 32])] = include!("scalar_data.txt");
const VECTOR_DATA: &[(u32, usize, [u64; 32], [u128; 32], [u64; 32], [u128; 32])] =
    include!("vector_data.txt");

fn main() {
    {
        let scalar_tests: Vec<InstructionFuzzTest> = SCALAR_DATA
            .into_iter()
            .copied()
            .map(
                |(instruction, test_number, pre_gprs, post_gprs)| InstructionFuzzTest {
                    test_number,
                    instruction,
                    pre_gprs,
                    post_gprs,
                    pre_fprs: [0u128; 32],
                    post_fprs: [0u128; 32],
                },
            )
            .collect();

        let contents = postcard::to_allocvec(&scalar_tests).unwrap();

        std::fs::write("fuzz_scalar.postcard", contents).unwrap();
    }

    {
        let vector_tests: Vec<InstructionFuzzTest> = VECTOR_DATA
            .into_iter()
            .copied()
            .map(
                |(instruction, test_number, pre_gprs, pre_fprs, post_gprs, post_fprs)| {
                    InstructionFuzzTest {
                        test_number,
                        instruction,
                        pre_gprs,
                        pre_fprs,
                        post_gprs,
                        post_fprs,
                    }
                },
            )
            .collect();

        let contents = postcard::to_allocvec(&vector_tests).unwrap();

        std::fs::write("fuzz_vector.postcard", contents).unwrap();
    }
}
