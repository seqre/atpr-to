{
  lib,
  rustPlatform,
}:

rustPlatform.buildRustPackage {
  pname = "atpr-to";
  version = "0.1.0";

  src = lib.cleanSourceWith {
    src = lib.cleanSource ./.;
    filter =
      path: type:
      let
        relPath = lib.removePrefix (toString ./. + "/") path;
        topDir = lib.head (lib.splitString "/" relPath);
      in
      !builtins.elem topDir [
        "target"
        ".github"
        ".claude"
        ".impeccable"
      ];
  };

  cargoLock.lockFile = ./Cargo.lock;

  # Tests are hermetic: wiremock binds 127.0.0.1 and test_router points
  # Slingshot at an unreachable address, so nothing leaves the sandbox.
  doCheck = true;

  meta = {
    description = "Bluesky-backed short-link service (atpr.to)";
    homepage = "https://atpr.to";
    license = lib.licenses.mit;
    mainProgram = "atpr-to";
    platforms = lib.platforms.unix;
  };
}
