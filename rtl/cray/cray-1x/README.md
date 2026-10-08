# CPU sources from cray-1x

These files started as the X-MP generation of Chris Fenton's CRAY-1 for
FPGAs, from the cray-1x project (googlecode trunk r257, directory
`Verilog/xmp`). That project is no longer maintained, so the files are
maintained here like the rest of the core: `make format` formats them and
`make lint` checks them.

What was repaired and cleaned up is described in `docs/MACHINE.md`. The first
commit of this core has the files as they came, with the repairs but in the
upstream layout, for comparing with the originals.

The upstream directory of X-MP modules (I/O channels, the registers shared by
four CPUs) is gone: what the X-MP needs is built into `func_top.v`.
