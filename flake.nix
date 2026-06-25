{
  description = "termsand flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";

    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = inputs@{ self, nixpkgs, flake-parts, rust-overlay, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      perSystem = { config, pkgs, system, ... }: {

        _module.args.pkgs = import nixpkgs {
          inherit system;
          overlays = [
            (import rust-overlay)
          ];
        };

        devShells.default = pkgs.mkShell rec {
          packages = with pkgs; [
            (rust-bin.stable.latest.default.override {
              extensions = [

                "cargo"
                "clippy"
                "rust-src"
                "rust-analyzer"
                "rustc"
                "rustfmt"
              ];
            })

          ];

          buildInputs = with pkgs; [
            libclang
            cmake
          ];

          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;

        };
      };
    };
}
