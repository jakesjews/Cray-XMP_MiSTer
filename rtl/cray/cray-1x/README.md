# cray-1x CPU sources

These files come from the cray-1x project, Chris Fenton's CRAY-1 and X-MP for
FPGAs (googlecode trunk r257, directory `Verilog/xmp`). They are the base the
CPU of this core is built on.

Carrying repairs made for this core, described in `docs/CPU.md`:
`a_regfile.v`, `brancher.v`, `func_top.v`, `i_buf.v`, `imm_gen.v`,
`s_regfile.v`, `s_res_lut.v`, `s_scheduler.v`, `scalar_pop_lz.v`,
`scalar_shift.v`, `v_scheduler.v`.

Unchanged apart from line endings: `a_res_lut.v`, `a_scheduler.v`,
`addr_add.v`, `b_regfile.v`, `cray_types.vh`, `fast_addr_mult.v`, `lz_sub.v`,
`s_const_gen.v`, `scalar_add.v`, `scalar_logical.v`, `t_regfile_hard.v` and
everything in `xmp/`. The `xmp/` modules are only instantiated with `XMP = 1`,
and `vector_pop_parity.v` is not built at all.

The files keep their upstream layout so they can be compared with the
originals (`diff -w --strip-trailing-cr`). `make format` and `make lint` leave
them alone; see `docs/DEVELOPMENT.md`.
