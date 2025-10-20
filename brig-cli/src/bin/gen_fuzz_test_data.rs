use common::fuzz_test::InstructionFuzzTest;

fn main() {
    write_postcard(include!("fuzz_scalar.txt"), "fuzz_scalar.postcard");
    write_postcard(include!("fuzz_vector.txt"), "fuzz_vector.postcard");
    write_postcard(include!("fuzz_float.txt"), "fuzz_float.postcard");
}

fn write_postcard(data: &[(u32, usize, [u64; 32], [u128; 32], [u64; 32], [u128; 32])], name: &str) {
    let tests = data
        .iter()
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
        .collect::<Vec<InstructionFuzzTest>>();

    let contents = postcard::to_allocvec(&tests).unwrap();

    std::fs::write(name, contents).unwrap();
}
