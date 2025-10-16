use {
    bootloader::BootConfig,
    bootloader_boot_config::LevelFilter,
    cargo_metadata::{Artifact, Message, TargetKind, diagnostic::DiagnosticLevel},
    clap::{Parser, Subcommand},
    common::{
        TestConfig,
        ringbuffer::{Consumer, MaybeSplitBuffer, RingBuffer},
    },
    elf::{ElfBytes, endian::AnyEndian, section::SectionHeader},
    itertools::Itertools,
    ovmf_prebuilt::{Arch, FileType, Source},
    std::{
        fs::{self, File},
        io::{BufReader, BufWriter, Write},
        path::{Path, PathBuf},
        process::{self, Stdio},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
    },
    tar::Header,
    walkdir::WalkDir,
};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Release profile build of kernel
    #[arg(short, long)]
    release: bool,

    /// Do not run QEMU and start brig
    #[arg(long)]
    no_run: bool,

    /// Do not build brig, use existing UEFI image and guest data
    #[arg(long)]
    no_build: bool,

    /// Run only the supplied tests
    #[arg(long, value_delimiter = ',')]
    include_tests: Option<Vec<String>>,

    /// Run all except the supplied tests
    #[arg(long, value_delimiter = ',')]
    exclude_tests: Option<Vec<String>>,

    #[arg(long, value_delimiter = ',')]
    all_tests: bool,

    /// Enable QEMU GDB server
    #[arg(long)]
    gdb: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    GdbCli,
}

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    if cli.no_build {
        todo!(
            "use existing artifacts somehow, useful for building on one machine and running on another"
        );
    }

    let test_config = match (cli.include_tests, cli.exclude_tests, cli.all_tests) {
        (None, None, false) => TestConfig::None,
        (Some(include), None, false) => TestConfig::Include(include),
        (None, Some(exclude), false) => TestConfig::Exclude(exclude),
        (None, None, true) => TestConfig::All,
        _ => panic!("include, exclude and all test CLI flags are mutually exclusive"),
    };

    let artifacts = build_cargo("../brig", cli.release, cli.verbose);

    if let Some(Command::GdbCli) = cli.command {
        gdb_cli(&artifacts);
    }

    // create TAR file containing guest kernel, plugins, and configuration
    let guest_tar = build_guest_tar("./guest_data", &artifacts, test_config);

    // create an UEFI disk image of kernel
    let kernel_path = get_kernel_from_artifacts(&artifacts);

    if cli.verbose {
        println!("got kernel @ {kernel_path:?}");
    }

    let uefi_kernel_path = kernel_path.parent().unwrap().join("uefi.img");
    let mut config = BootConfig::default();
    config.log_level = LevelFilter::Off;
    bootloader::UefiBoot::new(&kernel_path)
        .set_boot_config(&config)
        .create_disk_image(&uefi_kernel_path)
        .unwrap();

    if cli.verbose {
        println!("built UEFI image @ {uefi_kernel_path:?}");
    }

    if cli.no_run {
        return Ok(());
    }

    // start QEMU with UEFI disk image
    run_brig(&uefi_kernel_path, &guest_tar, cli.gdb);

    Ok(())
}

/// Builds the cargo project at the supplied path, returning the artifacts
/// produced
fn build_cargo<P: AsRef<Path>>(path: P, release: bool, verbose: bool) -> Vec<Artifact> {
    println!(
        "building cargo project {:?}",
        path.as_ref().to_str().unwrap()
    );

    let mut cmd = {
        let mut cmd = process::Command::new("cargo");
        cmd.arg("build");

        if release {
            cmd.arg("--release");
        }

        if !verbose {
            cmd.arg("-F no_logging");
        }

        cmd.arg("--message-format=json")
            .current_dir(path)
            .stdout(Stdio::piped());
        cmd
    };

    let mut handle = cmd.spawn().unwrap();

    let stdout = handle.stdout.as_mut().unwrap();

    let artifacts = Message::parse_stream(BufReader::new(stdout))
        .map(Result::unwrap)
        .map(move |msg| {
            if verbose {
                if let Message::CompilerMessage(ref msg) = msg {
                    if let DiagnosticLevel::Error
                    | DiagnosticLevel::Ice
                    | DiagnosticLevel::FailureNote = msg.message.level
                    {
                        println!("{msg}");
                    }
                }
            }

            msg
        })
        .filter_map(|msg| {
            if let Message::CompilerArtifact(a) = msg {
                Some(a)
            } else {
                None
            }
        })
        .collect();

    assert!(handle.wait().unwrap().success());

    artifacts
}

/// Build the guest tarfile from the data path and artifacts
///
/// Directory structure is recreated in tarfile from data path except:
///
/// * `platform.dts` is converted to `platform.dtb`
/// * `cdylib` artifacts are placed in the `plugins` directory of the tarfile
///   (in addition to any in the guest data file)
fn build_guest_tar<P: AsRef<Path>>(
    guest_data_path: P,
    artifacts: &[Artifact],
    test_config: TestConfig,
) -> PathBuf {
    // todo: rewrite this to process guest_data files in iterator into tar file,
    // some left alone (plugins dir, config.json), others are converted like
    // platform.dts,

    // // // make rootfs.ext2
    // // const ROOTFS_SIZE: usize = 512 * 1024 * 1024;
    // // let volume = File::create(target_dir.join("rootfs.ext2")).unwrap();
    // // volume.set_len(ROOTFS_SIZE);
    // // assert!(Command::new("mkfs.ext2")
    // //     .arg(volume.path(),)
    // //     .output()
    // //     .unwrap()
    // //     .status
    // //     .success());

    // really need to improve this heuristic
    let target_dir = artifacts
        .iter()
        .find(|a| {
            matches!(
                a.executable.as_ref().and_then(|p| p.file_name()),
                Some("brig")
            )
        })
        .unwrap()
        .executable
        .as_ref()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let tar_path = target_dir.canonicalize().unwrap().join("guest.tar");
    let mut tar = tar::Builder::new(File::create(&tar_path).unwrap());

    let plugins = artifacts
        .iter()
        .filter(|a| a.target.kind.contains(&TargetKind::CDyLib))
        .flat_map(|a| a.filenames.iter())
        .map(|path| path.canonicalize().unwrap())
        .map(|source| {
            let dest = PathBuf::from("plugins").join(source.file_name().unwrap());
            (source, dest)
        });

    WalkDir::new(&guest_data_path)
        .into_iter()
        .map(Result::unwrap)
        .filter(|entry| entry.path().is_file())
        .map(
            |entry| match entry.path().extension().and_then(|s| s.to_str()) {
                Some("dts") => build_dtb(&guest_data_path, entry.path(), target_dir),
                _ => (
                    entry.path().to_owned(),
                    entry
                        .path()
                        .strip_prefix(&guest_data_path)
                        .unwrap()
                        .to_owned(),
                ),
            },
        )
        .chain(plugins)
        .for_each(|(src, dest)| {
            let data = fs::read(src).unwrap();

            let mut header = Header::new_gnu();
            header.set_path(dest).unwrap();
            header.set_size(data.len() as u64);
            header.set_cksum();

            tar.append(&header, data.as_slice()).unwrap();
        });

    {
        let data = postcard::to_allocvec(&test_config).unwrap();

        let mut header = Header::new_gnu();
        header.set_path("test_config.postcard").unwrap();
        header.set_size(u64::try_from(data.len()).unwrap());
        header.set_cksum();

        tar.append(&header, data.as_slice()).unwrap();
    }

    tar.into_inner().unwrap().flush().unwrap();

    tar_path
}

/// dtc -I dts -O dtb -o $@ $<
fn build_dtb<P0: AsRef<Path>, P1: AsRef<Path>, P2: AsRef<Path>>(
    guest_data_path: P0,
    dts: P1,
    target_dir: P2,
) -> (PathBuf, PathBuf) {
    // replace extension of DTS file
    let dtb_filename = (dts.as_ref().file_stem().unwrap().to_string_lossy() + ".dtb").into_owned();

    // path to the output DTB
    let dtb_source_path = target_dir.as_ref().join(&dtb_filename);

    let output = process::Command::new("dtc")
        .args(["-I", "dts", "-O", "dtb", "-o"])
        .arg(&dtb_source_path)
        .arg(dts.as_ref())
        .output()
        .unwrap();

    if !output.status.success() {
        panic!(
            "failed to create DTB:\n{}\n{}",
            String::from_utf8(output.stdout).unwrap(),
            String::from_utf8(output.stderr).unwrap()
        )
    }

    let dtb_destination_path = dts
        .as_ref()
        .parent()
        .unwrap()
        .strip_prefix(guest_data_path)
        .unwrap()
        .join(dtb_filename);

    (dtb_source_path, dtb_destination_path)
}

fn run_brig(kernel_path: &Path, guest_tar_path: &Path, gdb: bool) {
    let prebuilt = ovmf_prebuilt::Prebuilt::fetch(
        Source::LATEST,
        guest_tar_path.parent().unwrap().join("ovmf"),
    )
    .expect("failed to update prebuilt");

    println!("starting QEMU");
    let mut cmd = std::process::Command::new("qemu-system-x86_64");
    cmd.args([
        "-drive",
        &format!(
            "if=pflash,unit=0,format=raw,readonly=on,file={}",
            prebuilt
                .get_file(Arch::X64, FileType::Code)
                .to_str()
                .unwrap()
        ),
    ]);
    cmd.args([
        "-drive",
        &format!(
            "if=pflash,unit=1,format=raw,readonly=on,file={}",
            prebuilt
                .get_file(Arch::X64, FileType::Vars)
                .to_str()
                .unwrap()
        ),
    ]);
    cmd.args([
        "-drive",
        &format!("format=raw,file={}", kernel_path.to_str().unwrap()),
    ]);

    cmd.arg("-nographic");

    #[cfg(target_arch = "x86_64")]
    {
        cmd.args(["-enable-kvm", "-cpu", "host"]);
    }

    cmd.arg("-no-reboot");

    if gdb {
        cmd.args(["-gdb", "tcp::1234", "-S"]); //  freeze CPU at startup
    }

    cmd.args(["-m", "16g"]);

    cmd.args([
        "-device",
        "virtio-blk-pci,drive=drive0,id=virtblk0,num-queues=4",
    ]);
    cmd.args([
        "-drive",
        &format!(
            "file={},if=none,format=raw,id=drive0",
            guest_tar_path.display()
        ),
    ]);

    cmd.args([
        "-device",
        "virtio-blk-pci,drive=drive1,id=virtblk1,num-queues=4",
    ]);
    cmd.args([
        "-drive",
        &format!("file=guest_data/hello_rs.ext2,if=none,format=raw,id=drive1",),
    ]);

    cmd.args(["-debugcon", "file:/tmp/debugcon"]);

    cmd.args(["-M", "q35"]);

    cmd.args(["-qmp", "unix:/tmp/qmp.sock,server,nowait"]);

    let mem_path = "/dev/shm/brig-shared-mem";

    cmd.args(["-device", "ivshmem-plain,memdev=ivshmem"]);
    cmd.args([
        "-object",
        &format!("memory-backend-file,id=ivshmem,share=on,mem-path={mem_path},size=64M"),
    ]);

    let ready = Arc::new(AtomicBool::new(false));
    let terminate = Arc::new(AtomicBool::new(false));

    let ready_clone = ready.clone();
    let terminate_clone = terminate.clone();

    let handle = thread::spawn(move || {
        hyperport_reader(
            mem_path,
            "/tmp/hyperport.trace",
            ready_clone,
            terminate_clone,
        )
    });

    while ready.load(Ordering::Relaxed) {}

    let mut child = cmd.spawn().unwrap();
    child.wait().unwrap();

    terminate.store(true, Ordering::Relaxed);
    handle.join().unwrap();
}

fn get_kernel_from_artifacts(artifacts: &[Artifact]) -> PathBuf {
    artifacts
        .iter()
        .filter_map(|a| a.executable.as_ref())
        .filter(|p| matches!(p.file_name(), Some("brig")))
        .exactly_one()
        .unwrap()
        .canonicalize()
        .unwrap()
}

fn gdb_cli(artifacts: &[Artifact]) {
    let kernel_path = get_kernel_from_artifacts(artifacts);
    let data = fs::read(&kernel_path).unwrap();
    let file = ElfBytes::<AnyEndian>::minimal_parse(data.as_slice()).expect("open kernel ELF");

    let text_header: SectionHeader = file
        .section_header_by_name(".text")
        .expect("section table should be parseable")
        .expect("file should have a .text section");

    let offset = 0xffff800000000000 + text_header.sh_addr;

    let mut gdb = process::Command::new("gdb")
        .stdin(Stdio::inherit()) // Use terminal's stdin
        .stdout(Stdio::inherit()) // Use terminal's stdout
        .stderr(Stdio::inherit()) // Use terminal's stderr
        .args(
            // todo: layout split source and regs
            format!(
                r#"
                    set trace-commands on
                    tui enable
                    layout split

                    # offset to kernel load bias + start of .text section
                    add-symbol-file {} {:#x}

                    target remote :1234

                    hbreak kernel::host::dbt::trampoline::trampoline
                "#,
                kernel_path.to_string_lossy(),
                offset,
            )
            .lines()
            .map(|cmd| format!("--eval-command={cmd}")),
        )
        .spawn()
        .unwrap();

    gdb.wait().expect("Child process wasn't running properly");
    std::process::exit(0);
}

fn hyperport_reader<P1: AsRef<Path>, P2: AsRef<Path>>(
    shared_mem_path: P1,
    destination_path: P2,
    ready: Arc<AtomicBool>,
    terminate: Arc<AtomicBool>,
) {
    println!(
        "starting hyperport reader @ {:?}, writing to {:?}",
        shared_mem_path.as_ref(),
        destination_path.as_ref()
    );

    let mut dest = BufWriter::new(
        File::options()
            .create(true)
            .truncate(true)
            .write(true)
            .open(destination_path)
            .unwrap(),
    );

    let shared_file = File::options()
        .write(true)
        .read(true)
        .open(shared_mem_path)
        .unwrap();
    let mut mem = unsafe { memmap2::MmapMut::map_mut(&shared_file) }.unwrap();

    let mut rb = RingBuffer::<Consumer>::init(&mut mem);

    ready.store(true, Ordering::Relaxed);

    while !terminate.load(Ordering::Relaxed) {
        rb.read(|buffer| {
            match buffer {
                MaybeSplitBuffer::Single(buf) => dest.write_all(buf).unwrap(),
                MaybeSplitBuffer::Split(a, b) => {
                    dest.write_all(a).unwrap();
                    dest.write_all(b).unwrap();
                }
            };
            buffer.len()
        });
    }
}
