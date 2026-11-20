
{
  description = "icewind: borealis (Sail -> brig model compiler) and brig (unikernel DBT)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";

    # Provides the exact nightly pinned in the repo's rust-toolchain.toml files.
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # Same commit as the borealis/data/sail-arm submodule. Bump both together.
    sail-arm = {
      url = "github:rems-project/sail-arm/d329002dde33c816a1cf736d874ec0c30535e7c4";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, sail-arm }:
    # brig targets x86_64 (q35 + KVM); borealis builds on either Linux arch.
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        inherit (pkgs) lib;

        # The Dockerfile uses OCaml 5.2 and Sail 0.18, and the OCaml wrapper in
        # borealis/sailrs is written against the Sail 0.18 API.
        ocamlPackages = pkgs.ocaml-ng.ocamlPackages_5_2;

        sail = (ocamlPackages.sail.override { version = "0.18"; }).overrideAttrs (old: {
          src = pkgs.fetchurl {
            url = "https://github.com/rems-project/sail/releases/download/0.18/sail-0.18.tbz";
            hash = "sha256-/NvaFPHtWfow4j2jSr4CVHQW48KoP77uVgbhAKXtzzU=";
          };
          # nixpkgs' menhir patch applies to 0.18 too; only make the cleanup tolerant.
          preBuild = ''
            rm -rf aarch* snapshots
          '';
        });

        # libsail checks each `$option` directive against the option list its
        # caller passes in. The stdlib's are all `-lem_extern_type` and
        # `-coq_extern_type`, which the `sail` binary gets from its Lem and Coq
        # backend plugins. The sailrs wrapper loads no plugins and passes no
        # options (see borealis/sailrs/wrapper/wrapper.ml), so libsail rejects
        # them as unknown flags. They only stop the Lem/Coq backends emitting a
        # definition for a type, so drop them from a copy of the share dir.
        sailDir = pkgs.runCommand "sail-share-sailrs" { } ''
          cp -r ${sail}/share/sail $out
          chmod -R u+w $out
          find $out/lib -name '*.sail' -exec \
            sed -i -E '/^\$option +--?(lem|coq)[-_]extern[-_]type /d' {} +

          # Any other $option would fail the same way, so catch it at build time.
          if grep -rn '^\$option' $out/lib; then
            echo 'sailrs would reject the $option directives above' >&2
            exit 1
          fi
        '';

        # The OCaml wrapper is built with `(modes object)`, i.e. a partial link
        # (`ld -r`) that can only pull in static archives, and zarith asks for
        # -lgmp. nixpkgs' gmp ships only libgmp.so by default, so add libgmp.a
        # (built with --with-pic, so it's fine to link into the Rust binaries).
        # In the Dockerfile this came from opam's `gmp` package.
        gmpStatic = pkgs.gmp.override { withStatic = true; };

        # All four rust-toolchain.toml files pin the same nightly; brig's adds the
        # x86_64-unknown-none target, so it covers every workspace.
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./brig/rust-toolchain.toml;
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

        # The borealis workspace holds both pipeline tools: `sailrs` (Sail model ->
        # JIB archive) and `borealis` (JIB archive -> brig model). Each is its own
        # derivation built from only the sources it needs, so a stage below reruns
        # only when its own tool or inputs change. borealis links sailrs as a
        # library, so both builds compile the OCaml wrapper and share this setup.
        buildBorealisTool = { pname, version, description, fileset, extraPostPatch ? "" }:
          rustPlatform.buildRustPackage {
            inherit pname version;

            # borealis/.cargo/config.toml is left out on purpose: it links with
            # clang and sets -Ctarget-cpu=native, which would tie the binaries to
            # the build machine's CPU.
            src = lib.fileset.toSource { root = ./.; inherit fileset; };

            cargoRoot = "borealis";
            buildAndTestSubdir = "borealis";
            cargoBuildFlags = [ "--package" pname ];
            cargoLock = {
              lockFile = ./borealis/Cargo.lock;
              outputHashes = {
                "isla-lib-0.2.0" = "sha256-iwk9rYFDbhKCO8CgAGRKcR5fxQS+FCv7OOgk6gxOqQU=";
              };
            };

            postPatch = ''
              wrapper=borealis/sailrs/wrapper

              # dune only runs `sail_manifest` when manifest.ml is missing, and that
              # tool leaves the Sail dir unset. Point it at the Nix store instead.
              cat > $wrapper/manifest.ml <<EOF
              let dir = Some "${sailDir}"
              let commit = "nix"
              let branch = "nix"
              EOF

              # `gmp` is opam's packaging of libgmp, which nixpkgs doesn't have;
              # gmpStatic provides the same libgmp.a.
              substituteInPlace $wrapper/dune \
                --replace-fail "(libraries libsail gmp dynlink)" "(libraries libsail dynlink)"
            '' + extraPostPatch;

            nativeBuildInputs = [
              ocamlPackages.ocaml
              ocamlPackages.findlib
              ocamlPackages.dune_3
              pkgs.opam # build.rs only checks `opam --version`
              pkgs.makeWrapper
            ];
            buildInputs = [ sail gmpStatic pkgs.zlib pkgs.z3 ];

            # sailrs/build.rs runs `dune build` for the OCaml wrapper.
            preBuild = ''
              export HOME=$TMPDIR
              export DUNE_CACHE=disabled
            '';

            doCheck = false;

            # Sail calls the z3 executable while type-checking a model.
            postInstall = ''
              wrapProgram $out/bin/${pname} \
                --prefix PATH : ${lib.makeBinPath [ pkgs.z3 ]} \
                --set-default SAIL_DIR ${sailDir}
            '';

            meta = {
              inherit description;
              mainProgram = pname;
            };
          };

        sailrs = buildBorealisTool {
          pname = "sailrs";
          version = "0.1.1";
          description = "Compiles a Sail ISA model to a JIB archive";
          # Cargo loads every workspace member, so borealis/borealis keeps its
          # manifest, and an empty lib.rs gives it a target. Leaving its real
          # sources out means editing them doesn't rerun the Sail stage.
          fileset = lib.fileset.unions [
            ./borealis/Cargo.toml
            ./borealis/Cargo.lock
            ./borealis/sailrs
            ./borealis/borealis/Cargo.toml
            ./common
          ];
          extraPostPatch = ''
            mkdir -p borealis/borealis/src
            touch borealis/borealis/src/lib.rs
          '';
        };

        borealis = buildBorealisTool {
          pname = "borealis";
          version = "0.3.0";
          description = "Compiles a JIB archive to a brig guest model";
          fileset = lib.fileset.unions [
            ./borealis/Cargo.toml
            ./borealis/Cargo.lock
            ./borealis/sailrs
            ./borealis/borealis
            ./common
          ];
        };

        # Stage 1: Armv9.4-A Sail model -> JIB archive. The model sources come from
        # the sail-arm input, so no submodule checkout is needed.
        arm-v9_4-a-jib = pkgs.runCommand "arm94_sail18.rkyv" { nativeBuildInputs = [ sailrs ]; } ''
          mkdir data
          cp ${./borealis/data/arm-v9.4-a.json} data/arm-v9.4-a.json
          ln -s ${sail-arm} data/sail-arm

          sailrs data/arm-v9.4-a.json $out
        '';

        # Stage 2: JIB archive -> brig guest model. $out is the file itself, so
        # `nix build .#aarch64-model -o brig-cli/guest_data/aarch64.postcard`
        # links it straight into brig-cli's guest data.
        aarch64-model = pkgs.runCommand "aarch64.postcard" {
          nativeBuildInputs = [ borealis ];
          # Worker threads overflow Rust's 2 MiB default while building rudder.
          RUST_MIN_STACK = toString (32 * 1024 * 1024);
        } ''
          borealis ${arm-v9_4-a-jib} $out
        '';
      in
      {
        packages = {
          inherit sail sailrs borealis arm-v9_4-a-jib aarch64-model;
          default = aarch64-model;
        };

        apps = {
          default = flake-utils.lib.mkApp { drv = borealis; exePath = "/bin/borealis"; };
          borealis = flake-utils.lib.mkApp { drv = borealis; exePath = "/bin/borealis"; };
          sailrs = flake-utils.lib.mkApp { drv = sailrs; exePath = "/bin/sailrs"; };
        };

        # Everything the Dockerfile installs, for `cargo r` in borealis/ and brig-cli/.
        # brig itself is built by brig-cli at run time, so it lives here rather than
        # as a package.
        devShells.default = pkgs.mkShell {
          packages = [
            rustToolchain
            ocamlPackages.ocaml
            ocamlPackages.findlib
            ocamlPackages.dune_3
            ocamlPackages.ocamlformat
            sail
            pkgs.opam
            pkgs.z3
            gmpStatic
            pkgs.zlib
            pkgs.clang
            pkgs.dtc
            pkgs.qemu
            pkgs.gdb
            pkgs.gcc-arm-embedded
          ] ++ lib.optionals (system == "x86_64-linux") [
            # The Dockerfile's gcc-aarch64-linux-gnu, with only its binaries linked
            # in. As a package, its setup hook would set CC to the aarch64 gcc and
            # break host C builds (e.g. ring via cc-rs).
            (pkgs.symlinkJoin {
              name = "aarch64-unknown-linux-gnu-tools";
              paths = let cc = pkgs.pkgsCross.aarch64-multiplatform.stdenv.cc; in [ cc cc.bintools ];
              postBuild = "rm -rf $out/nix-support";
            })
          ];

          # The wrapper falls back to an opam path unless SAIL_DIR is set.
          SAIL_DIR = sailDir;
        };
      });
}
