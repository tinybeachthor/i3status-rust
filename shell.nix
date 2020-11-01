{ pkgs ? import <nixpkgs> { } }:

with pkgs;

mkShell {
  buildInputs = [
    rustc
    cargo

    dbus
    libpulseaudio
  ];
  nativeBuildInputs = [
    pkgconfig
  ];
  shellHook = ''
  '';
}
