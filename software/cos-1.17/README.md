# COS 1.17 as it was recovered

The Cray Operating System 1.17 for a CRAY X-MP with an I/O Subsystem, from the
disk pack that [Chris Fenton](https://www.chrisfenton.com/cos-recovery/) read
and that [Andras Tantos](https://www.modularcircuits.com/blog/articles/the-cray-files/)
took apart and made run again. These three files are the ready-to-run system
of his [cray-sim](https://github.com/andrastantos/cray-sim) project, unchanged:

- `iop_kern.bin`: the kernel of the I/O Subsystem.
- `boot_tape.tap`: the tape the kernel loads its overlays from.
- `exp_disk.img.gz`: the disk of the Peripheral Expander, packed. It holds
  COS itself, the lists of parameters COS is started with, jobs, and a few
  programs. [tools/py/expdisk.py](../../tools/py/expdisk.py) lists it.

[tools/py/mkcos.py](../../tools/py/mkcos.py) makes the package for the SD
card from them.

The files come with the cray-sim project's licence, for non-commercial use
([LICENSE-cray-sim](LICENSE-cray-sim)). COS is the work of Cray Research.
