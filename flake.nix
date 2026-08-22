{
  description = "atpr.to — Bluesky-backed short-link service";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # Pinned Rust toolchain with cross-targets: x86_64 for local development,
    # aarch64-unknown-linux-gnu for the arm64 Lambda build.
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      ...
    }:
    let
      # Local development happens on x86_64; deployment target is arm64
      # (Lambda provided.al2023).
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      lambdaTarget = "aarch64-unknown-linux-gnu";
      lib = nixpkgs.lib;

      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
              # google-chrome is unfree but explicitly requested for testing.
              config.allowUnfree = true;
            }
          )
        );

      # One stable toolchain everywhere: host target for `cargo run`/tests on
      # x86_64 dev machines, plus the arm64 target so
      # `cargo lambda build --release --arm64` works without rustup.
      rustToolchain =
        pkgs:
        pkgs.rust-bin.stable.latest.minimal.override {
          extensions = [
            "rustfmt"
            "clippy"
            "rust-src"
            "rust-analyzer"
          ];
          targets = [ lambdaTarget ];
        };
    in
    {
      packages = forAllSystems (
        pkgs:
        let
          atpr-to = pkgs.callPackage ./package.nix { };
        in
        rec {
          inherit atpr-to;
          default = atpr-to;

          # Cross-compiled arm64 Linux build of the plain HTTP server.
          # (The actual Lambda artifact comes from `cargo lambda build`; this
          # is for running the release binary under qemu on the dev box.)
          atpr-to-aarch64 = pkgs.pkgsCross.aarch64-multiplatform.callPackage ./package.nix { };
        }
      );

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = "${pkgs.callPackage ./package.nix { }}/bin/atpr-to";
        };
      });

      overlays.default = _final: _prev: { atpr-to = _final.callPackage ./package.nix { }; };

      devShells = forAllSystems (
        pkgs:
        let
          # Browser testing is an x86_64-only concern; the arm64 build target
          # gets no browser at all.
          chrome = lib.optionals pkgs.stdenv.hostPlatform.isx86_64 [ pkgs.google-chrome ];
        in
        {
          default =
            with pkgs;
            mkShell {
              packages =
                [
                  (rustToolchain pkgs)

                  # Justfile workflow
                  just
                  cargo-nextest
                  cargo-deny
                  cargo-llvm-cov

                  # arm64 Lambda builds (cargo-lambda shells out to zig as the
                  # cross linker)
                  cargo-lambda
                  zig

                  # AWS deployment
                  aws-sam-cli
                  amazon-ecr-credential-helper
                ]
                ++ chrome;

              RUST_SRC_PATH = "${(rustToolchain pkgs)}/lib/rustlib/src/rust/library";
              # Point headless browser tooling (playwright/puppeteer-style
              # scripts, `--browser` flags) at the pinned chrome.
              BROWSER_PATH = lib.optionalString (chrome != [ ]) (lib.getExe (builtins.head chrome));
            };
        }
      );

    };
}
