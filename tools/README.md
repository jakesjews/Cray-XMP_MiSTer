# Cray-1 host tools

One Rust workspace, standard library only.

- `crates/isa`: the Cray-1 instruction table (encode, decode, disassemble).
- `crates/asm`: CAL-subset assembler producing raw memory images.
- `crates/fp`: bit-exact Cray floating-point add, multiply and reciprocal.
- `crates/model`: the reference model of the CPU, and `cray1-run`, which runs a memory image on it.
- `crates/ios`: the reference model of the I/O Subsystem and of a whole system, and `cray1-sys`,
  which runs the I/O Subsystem software and COS on it.
- `crates/cli`: the `cray1` command.

Memory images are raw big-endian 64-bit words. Parcel 0 of a word is bits 63 to 48.

Build and test with `cargo build --release` and `cargo test` from this directory.
