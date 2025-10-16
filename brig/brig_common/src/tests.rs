use {alloc::vec::Vec, common::TestConfig, linkme::distributed_slice, proc_macro_lib::ktest};

#[distributed_slice]
pub static TESTS: [(&str, fn())];

// could be simpler, but I want a panic if we ever pass in an unknown test
// instead of a silent fail
pub fn run(config: TestConfig) {
    let tests = match config {
        TestConfig::None => {
            log::info!("tests disabled");
            return;
        }
        TestConfig::Include(include) => {
            let mut tests = Vec::new();
            for name in &include {
                if let Some(test) = TESTS.iter().find(|(n, _)| n == name) {
                    tests.push(test)
                } else {
                    panic!("unknown test {name:?}");
                }
            }
            tests
        }
        TestConfig::Exclude(exclude) => {
            let mut tests = TESTS.iter().collect::<Vec<_>>();

            for name in &exclude {
                let Some(idx) = tests
                    .iter()
                    .enumerate()
                    .find(|(_, (n, _))| n == name)
                    .map(|(i, _)| i)
                else {
                    panic!("unknown test {name:?}");
                };
                tests.remove(idx);
            }

            tests
        }
        TestConfig::All => TESTS.iter().collect(),
    };

    log::info!("running {} tests", tests.len());

    for (name, test) in &tests {
        log::trace!("running {name:?}");
        test();
    }

    log::info!("all {} tests ran successfully", tests.len());
}
