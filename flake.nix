{
  description = "atpr.to";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, fenix, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        # Nightly, with rust-src for rust-analyzer and the llvm tools for
        # coverage.
        #
        # Note the mismatch: CI gates on stable (with a nightly matrix leg
        # allowed to fail). Clippy lints move between channels, so a local
        # `just lint` can disagree with the one that blocks a merge in either
        # direction. Swap in `fenix.packages.${system}.stable.toolchain` when
        # that bites; it is kept on nightly because rust-analyzer wants it.
        toolchain = fenix.packages.${system}.complete.toolchain;
        llvmToolsBin = "${toolchain}/lib/rustlib/${pkgs.stdenv.hostPlatform.rust.rustcTarget}/bin";
      in {
        devShells.default = pkgs.mkShell {
          packages = [
            toolchain
            pkgs.pkg-config
            pkgs.openssl

            # Everything the Justfile invokes. All of these were missing, so
            # most recipes failed in a shell that was supposed to provide them.
            pkgs.just
            pkgs.cargo-lambda # just build / just local
            pkgs.cargo-nextest # the runner CI uses
            pkgs.cargo-llvm-cov # just coverage
            pkgs.cargo-deny # just deny
            pkgs.aws-sam-cli # just deploy / just logs
          ];

          # rust-analyzer reads this to resolve std
          RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";

          # cargo-llvm-cov normally locates these via rustup; point it at the
          # fenix toolchain instead
          LLVM_COV = "${llvmToolsBin}/llvm-cov";
          LLVM_PROFDATA = "${llvmToolsBin}/llvm-profdata";
        };
      });
}
