{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    rust-overlay.url = "github:oxalica/rust-overlay";
    pre-commit-hooks-nix.url = "github:cachix/pre-commit-hooks.nix";
  };

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems =
        [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      imports = [ inputs.pre-commit-hooks-nix.flakeModule ];
      perSystem = { config, self', pkgs, lib, system, ... }:
        let
          runtimeDeps = with pkgs; [ python3 ];
          buildDeps = with pkgs;
            [ python3 ] ++ lib.optionals stdenv.hostPlatform.isDarwin
            [ pkgs.darwin.apple_sdk.frameworks.CoreServices ];
          devDeps = with pkgs; [
            mdbook
            cargo-deny
            cargo-udeps
            cargo-dist
            cargo-deb
            python3
            ruff
            pyright
            maturin
            multitail
            vhs
            tokio-console
            gdb
            valgrind
            heaptrack
          ];

          workspaceCargoToml =
            builtins.fromTOML (builtins.readFile ./Cargo.toml);
          mudpuppyCargoToml =
            builtins.fromTOML (builtins.readFile ./mudpuppy/Cargo.toml);
          msrv = workspaceCargoToml.workspace.package.rust-version;

          # Note: cross-compiling needs some work. Pyo3 requires care.
          rustTargets = workspaceCargoToml.workspace.metadata.dist.targets;

          rustPackage = features:
            (pkgs.makeRustPlatform {
              cargo = pkgs.rust-bin.stable.latest.minimal;
              rustc = pkgs.rust-bin.stable.latest.minimal;
            }).buildRustPackage {
              inherit (mudpuppyCargoToml.package) name version;
              src = ./.;
              buildAndTestSubdir = "mudpuppy";
              cargoLock.lockFile = ./Cargo.lock;
              buildFeatures = features;
              buildInputs = runtimeDeps;
              nativeBuildInputs = buildDeps;
            };

          mkDevShell = rustc:
            pkgs.mkShell {
              # Note: We set PYO3_PYTHON to avoid excessive rebuilds from Pyo3 picking
              #       up the python dep at runtime from the $PATH.
              PYO3_PYTHON = "${pkgs.python3}/bin/python";
              shellHook = ''
                ${config.pre-commit.installationScript}
                export RUST_SRC_PATH=${pkgs.rustPlatform.rustLibSrc}
                echo 1>&2 "🦎🕹️  MUD Puppy Dev  🕹️🦎"
              '';
              buildInputs = runtimeDeps;
              nativeBuildInputs = buildDeps ++ devDeps ++ [ rustc ];
            };

          rustNightly = (pkgs.rust-bin.selectLatestNightlyWith
            (toolchain: toolchain.default.override { targets = rustTargets; }));

          cargo-check = name: check: {
            enable = true;
            name = name;
            files = "\\.rs$";
            pass_filenames = false;
            entry = "${rustNightly}/bin/cargo ${check}";
          };

        in {
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ (import inputs.rust-overlay) ];
          };

          packages.default = self'.packages.mudpuppy;
          devShells.default = self'.devShells.nightly;

          packages.mudpuppy = (rustPackage [ ]);

          devShells.nightly = (mkDevShell rustNightly);
          devShells.stable = (mkDevShell
            (pkgs.rust-bin.stable.latest.default.override {
              targets = rustTargets;
            }));
          devShells.msrv = (mkDevShell
            (pkgs.rust-bin.stable.${msrv}.default.override {
              targets = rustTargets;
            }));

          pre-commit = {
            settings = {
              hooks = {
                nixfmt-classic.enable = true;
                cargo-check.enable = true;
                yamllint = {
                  settings.configPath = ".yamllint.yml";
                  enable = true;
                };
                ruff.enable = true;
                pyright.enable = false; # TODO(XXX): Restore after fixing stubs.
                nightly-fmt = (cargo-check "cargo-fmt" "fmt --check");
                nightly-clippy = (cargo-check "cargo-clippy"
                  "clippy --all-targets --all-features -- -D warnings");
              };
            };
            # Don't run pre-commit hooks in 'nix flake check'
            check.enable = false;
          };
        };
    };
}
