{ lib
, rustPlatform
}:

rustPlatform.buildRustPackage (finalAttrs: let
  toml = builtins.fromTOML (builtins.readFile "${finalAttrs.src}/Cargo.toml");
in {
  pname = toml.package.name;
  inherit (toml.package) version;

  src = ./.;

  cargoLock.lockFile = "${finalAttrs.src}/Cargo.lock";

  meta = with lib; {
    description = "Falling sand simulation for tmux";
    homepage = "https://github.com/cgsdev0/termsand";
    license = licenses.mit;
    maintainers = [ ];
    mainProgram = "termsand";
  };
})
