with import <nixpkgs> {};
stdenv.mkDerivation rec {
  name = "FantaFS-env";
  env = buildEnv { name = name; paths = buildInputs; };
  builder = builtins.toFile "builder.sh" ''
    source $stdenv/setup; ln -s $env $out
  '';

  buildInputs = [
    cargo
    openssh
    libgit2
    cmake
    openssl
    pkgconfig
    curl
    wget
    unzip
    elfutils
    python
  ];

  LIBGIT2_SYS_USE_PKG_CONFIG=1;

  shellHook = ''
    export SSL_CERT_FILE=${cacert}/etc/ssl/certs/ca-bundle.crt
  '';
}
