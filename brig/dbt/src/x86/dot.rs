use {
    crate::x86::X86Block,
    alloc::{
        format,
        string::{String, ToString},
    },
    common::{
        Alloc,
        arena::{Arena, Ref},
        hashmap::HashSet,
    },
    core::fmt::Write,
    itertools::Itertools,
};

pub fn render<A: Alloc>(arena: &Arena<X86Block<A>, A>, entry: Ref<X86Block<A>>) -> String {
    let mut out = String::new();

    let mut blocks = HashSet::default();
    let mut edges = HashSet::default();

    {
        let mut queue = alloc::vec![entry];
        while let Some(block) = queue.pop() {
            if blocks.contains(&block) {
                continue;
            }

            for next in block.get(arena).next_blocks() {
                edges.insert((block, *next));
                queue.push(*next);
            }

            blocks.insert(block);
        }
    }

    writeln!(&mut out, "digraph X86 {{").unwrap();

    for block in blocks {
        writeln!(
            &mut out,
            "{}[label=\"{{block {:#x}|{}}}\"][shape=\"record\"];",
            ref_label(block),
            block.index(),
            block
                .get(arena)
                .instructions()
                .iter()
                .map(|i| i.to_string())
                .join(r"\l")
        )
        .unwrap();
        //n1[label="{0x1|s0: read-var x:bv\ls1: size-of s0\ls2: cast zx s1 -\>
        // i64\ls5: const #1s : i64\ls6: sub s2 s5\ls13: write-var i:i64 \<=
        // s6:i64\ls16: const #0s : i64\ls17: cmp-lt s6 s16\ls18: branch s17 ?
        // block 0x3 : block 0x5\l}"][shape="record"];
    }

    for (src, dst) in edges {
        writeln!(
            &mut out,
            "{} -> {}[label=\"\"];",
            ref_label(src),
            ref_label(dst)
        )
        .unwrap();
    }

    writeln!(&mut out, "}}").unwrap();

    out
}

fn ref_label<A: Alloc>(r: Ref<X86Block<A>>) -> String {
    format!("n{:x}", r.index())
}
