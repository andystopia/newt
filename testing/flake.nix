{
  description = "An environment for building a compiling typst documents with vscode, typst, and typst-lsp";

  # Nixpkgs / NixOS version to use.
  inputs.nixpkgs.url = "github:NixOS/nixpkgs";

  outputs = {
    self,
    nixpkgs,
  }: let
    # System types to support.
    supportedSystems = ["x86_64-linux" "x86_64-darwin" "aarch64-linux" "aarch64-darwin"];

    # Helper function to generate an attrset '{ x86_64-linux = f "x86_64-linux"; ... }'.
    forAllSystems = nixpkgs.lib.genAttrs supportedSystems;

    # Nixpkgs instantiated for supported system types.
    nixpkgsFor = forAllSystems (system: import nixpkgs {inherit system;});
  in rec
  {
    # Provide some binary packages for selected system types.
    packages = forAllSystems (system: let
      pkgs = nixpkgsFor.${system};
    in {
      # grab the font
      atkinson = pkgs.stdenv.mkDerivation {
        buildInputs = with pkgs; [unzip];
        name = "atkinson";
        # https://brailleinstitute.org/wp-content/uploads/atkinson-hyperlegible-font/Atkinson-Hyperlegible-Font-Print-and-Web-2020-0514.zip
        src = builtins.fetchurl {
          url = "https://brailleinstitute.org/wp-content/uploads/atkinson-hyperlegible-font/Atkinson-Hyperlegible-Font-Print-and-Web-2020-0514.zip";
          sha256 = "sha256:1qskrsxlck1b3g4sjphmyq723fjspw3qm58yg59q5p6s7pana6ly";
        };

        phases = ["unpackPhase" "installPhase"];
        unpackPhase = "mkdir extract && unzip -d extract/ $src";
        installPhase = "ls extract/ && mkdir -p $out/share/fonts && cp -R extract/Atkinson-Hyperlegible-Font-Print-and-Web-2020-0514/Web\\ Fonts/TTF $out/share/fonts";
      };
      # maybe I could use this to build the PDF?? not sure.
    });

    # Add dependencies that are only needed for development
    devShells = forAllSystems (system: let
      pkgs = nixpkgsFor.${system};
    in {
      default = pkgs.mkShell {
        # typst for compilation.
        # starship for
        buildInputs = with pkgs; [typst starship watchexec];

        shellHook = "
              eval \"$(starship init bash)\";
            ";
      };

      full = pkgs.mkShell {
        # typst for compilation.
        # starship for nice shells

        buildInputs = with pkgs;
          [typst starship watchexec packages.${system}.atkinson]
          ++ [
            (vscode-with-extensions.override {
              vscode = vscodium;
              vscodeExtensions = with vscode-extensions; [
                nvarner.typst-lsp
                bbenoist.nix
                tomoki1207.pdf
                mgt19937.typst-preview
              ];
            })
          ];

        shellHook = "
              eval \"$(starship init bash)\";
              export TYPST_FONT_PATHS=\"${packages.${system}.atkinson}/share/fonts/TTF\"
            ";
      };
    });

    formatter = forAllSystems (
      system:
        nixpkgsFor.${system}.alejandra
    );
  };
}
