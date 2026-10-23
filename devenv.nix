{ pkgs, lib, ... }:

let
  runtimeLibs = with pkgs; [
    udev
    alsa-lib
    wayland
    libxkbcommon
    vulkan-loader
    libGL
    libx11
    libxcursor
    libxrandr
    libxi
  ];
in
{
  packages = with pkgs; [
    pkg-config
    cargo
    rustc
    rustfmt
    clippy
    wiimms-iso-tools
  ] ++ runtimeLibs;

  env.LD_LIBRARY_PATH = lib.makeLibraryPath runtimeLibs;
}
