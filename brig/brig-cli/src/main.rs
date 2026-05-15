use {
    cargo_metadata::{diagnostic::DiagnosticLevel, Artifact, Message},
    clap::Parser,
    itertools::Itertools,
    std::{
        fs::File,
        io::BufReader,
        path::{Path, PathBuf},
        process::{Command, Stdio},
    },
    walkdir::WalkDir,
};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Enable QEMU GDB server
    #[arg(long)]
    gdb: bool,

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
}

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    if cli.no_build {
        todo!("use existing artifacts somehow, useful for building on one machine and running on another");
    }

    let artifacts = build_cargo("../brig", cli.release, cli.verbose);

    // create TAR file containing guest kernel, plugins, and configuration
    let guest_tar = build_guest_tar("./guest_data", &artifacts);

    // create an UEFI disk image of kernel
    let uefi_path = {
        let kernel_path = artifacts
            .iter()
            .filter_map(|a| a.executable.as_ref())
            .filter(|p| matches!(p.file_name(), Some("kernel")))
            .exactly_one()
            .unwrap()
            .canonicalize()
            .unwrap();

        if cli.verbose {
            println!("got kernel @ {kernel_path:?}");
        }

        let uefi_path = kernel_path.parent().unwrap().join("uefi.img");
        bootloader::UefiBoot::new(&kernel_path)
            .create_disk_image(&uefi_path)
            .unwrap();

        if cli.verbose {
            println!("built UEFI image @ {uefi_path:?}");
        }

        uefi_path
    };

    if cli.no_run {
        return Ok(());
    }

    // start QEMU with UEFI disk image
    run_brig(&uefi_path, &guest_tar, cli.gdb);

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
        let mut cmd = Command::new("cargo");
        cmd.arg("build");

        if release {
            cmd.arg("--release");
        }

        cmd.arg("-Zjson-target-spec");

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
fn build_guest_tar<P: AsRef<Path>>(guest_data_path: P, artifacts: &[Artifact]) -> PathBuf {
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
                Some("kernel")
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
        .filter(|a| a.target.kind == ["cdylib"])
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
            tar.append_file(dest, &mut File::open(src).unwrap())
                .unwrap();
        });

    tar.finish().unwrap();
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

    let output = Command::new("dtc")
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

fn run_brig(uefi_path: &Path, guest_tar_path: &Path, gdb: bool) {
    println!("starting QEMU");
    let mut cmd = std::process::Command::new("qemu-system-x86_64");
    cmd.arg("-bios").arg(ovmf_prebuilt::ovmf_pure_efi());
    cmd.arg("-drive")
        .arg(format!("format=raw,file={}", uefi_path.to_str().unwrap()));
    cmd.arg("-nographic");
    cmd.arg("-enable-kvm");
    cmd.arg("-no-reboot");

    if gdb {
        cmd.arg("-gdb");
        cmd.arg("tcp::1234");
        cmd.arg("-S"); //  freeze CPU at startup
    }

    cmd.arg("-m");
    cmd.arg("24g");
    cmd.arg("-device");
    cmd.arg("virtio-blk-pci,drive=drive0,id=virtblk0,num-queues=4");
    cmd.arg("-drive");
    cmd.arg(format!(
        "file={},if=none,format=raw,id=drive0",
        guest_tar_path.display()
    ));
    // cmd.arg("-device");
    // cmd.arg("virtio-blk-pci,drive=drive1,id=virtblk1,num-queues=4");
    // cmd.arg("-drive");
    // cmd.arg("file=../../brig-programs/rootfs.ext2,if=none,format=raw,id=drive1");
    cmd.arg("-M");
    cmd.arg("q35");
    cmd.arg("-cpu");
    cmd.arg("host");

    let mut child = cmd.spawn().unwrap();
    child.wait().unwrap();
}
