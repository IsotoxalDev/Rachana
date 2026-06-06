{
  description = "Clapperboard screenplay editor";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, fenix }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      toolchain = fenix.packages.${system}.stable.toolchain;
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        nativeBuildInputs = [
          toolchain
          pkgs.pkg-config
          pkgs.clang
          pkgs.lld
          pkgs.mold
        ];

        buildInputs = [
          pkgs.openssl
          pkgs.fontconfig
          pkgs.freetype
          pkgs.libxkbcommon
          pkgs.wayland
          pkgs.wayland-protocols
          pkgs.vulkan-loader
          pkgs.vulkan-headers
          pkgs.vulkan-validation-layers
          pkgs.libGL
          pkgs.mesa
          pkgs.alsa-lib
          pkgs.libxcb
          pkgs.xorg.libX11
          pkgs.xorg.libXcursor
          pkgs.xorg.libXrandr
          pkgs.xorg.libXi
          pkgs.xorg.libxcb
          pkgs.xorg.xcbutilimage
          pkgs.xorg.xcbutilkeysyms
          pkgs.xorg.xcbutilwm
          pkgs.xorg.xcbutilrenderutil
        ];

        shellHook = ''
          export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath [
            pkgs.vulkan-loader
            pkgs.libGL
            pkgs.wayland
            pkgs.libxkbcommon
            pkgs.libxcb
            pkgs.xorg.libX11
          ]}:$LD_LIBRARY_PATH"
          export LIBCLANG_PATH="${pkgs.llvmPackages.libclang.lib}/lib"
          export VULKAN_SDK="${pkgs.vulkan-headers}"
        '';
      };
    };
}
