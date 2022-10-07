{
  description = "A Nix flake containing the standard fuel-core optimized build environment";

    inputs = {
        nixpkgs = {
          url = "github:NixOS/nixpkgs/nixos-unstable";
        };
        rust-overlay = {
          url = "github:oxalica/rust-overlay/master";
          inputs.nixpkgs.follows = "nixpkgs";
        };
        flake-utils.url = "github:numtide/flake-utils";
    };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
      in
      with pkgs;
      {
        devShell = pkgs.mkShell {
            buildInputs = [
                pkgs.grpc-tools
                pkgs.clang
                pkgs.pkg-config
                rust-bin.stable."1.64.0".default
            ];
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
            PROTOC = "${pkgs.grpc-tools}/bin/protoc";
            ROCKSDB_LIB_DIR = "${pkgs.rocksdb}/lib";
        };
      });
}
