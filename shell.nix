{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell {
  nativeBuildInputs = with pkgs; [ pkg-config ];
  buildInputs = with pkgs; [
    fontconfig freetype
    vulkan-loader libGL
    wayland libxkbcommon xkeyboard_config
    libx11 libxcursor libxi libxrandr
  ];
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [
    vulkan-loader libGL wayland libxkbcommon fontconfig
    libx11 libxcursor libxi libxrandr
  ]);
  # Wayland+winit needs xkb data files at runtime
  XKB_CONFIG_ROOT = "${pkgs.xkeyboard_config}/share/X11/xkb";
  # Nixpkgs vulkan-loader picks ICDs from this by default, but be explicit
  # so systems with /run/opengl-driver still get their real GPU drivers.
  shellHook = ''
    if [ -d /run/opengl-driver/share/vulkan/icd.d ]; then
      export VK_ICD_FILENAMES=$(ls /run/opengl-driver/share/vulkan/icd.d/*.json 2>/dev/null | paste -sd:)
    fi
  '';
}
