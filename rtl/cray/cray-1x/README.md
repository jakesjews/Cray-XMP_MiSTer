# CPU sources from cray-1x

These files started as the X-MP generation of Chris Fenton's CRAY-1 for
FPGAs, from the cray-1x project (googlecode trunk r257, directory
`Verilog/xmp`). That project is no longer maintained, so the files are
maintained here like the rest of the core: `make format` formats them and
`make lint` checks them.

What was repaired and cleaned up is described in `docs/CPU.md`. The first
commit of this core has the files as they came, with the repairs but in the
upstream layout, for comparing with the originals.

`xmp/` holds the modules that are only used with `XMP = 1`.
`xmp/vector_pop_parity.v` is not instantiated by any build.
