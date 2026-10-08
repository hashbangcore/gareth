{
  description = "A CLI for LLMs, for advanced GNU/Linux users";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
  let
    system = "x86_64-linux";
    pkgs = import nixpkgs { inherit system; };
  in {
    packages.${system}.default = pkgs.rustPlatform.buildRustPackage {
      pname = "gareth";
      version = "0.1.2";
      src = ./.;
      cargoLock = {
        lockFile = ./Cargo.lock;
      };
    };

    apps.${system}.default = {
      type = "app";
      program = "${self.packages.${system}.default}/bin/gareth";
    };
  };
}
