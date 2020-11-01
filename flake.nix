{
  inputs = {
    flake-utils.url = github:numtide/flake-utils;
    nixpkgs.url = github:NixOS/nixpkgs/nixos-20.09;
    tinybeachthor = {
      url = github:tinybeachthor/nur-packages/master;
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs = { self, flake-utils, nixpkgs, tinybeachthor }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = (import nixpkgs {
          inherit system;
          overlays = [ tinybeachthor.overlay ];
        });
      in { devShell = import ./shell.nix { inherit pkgs; }; });
}
