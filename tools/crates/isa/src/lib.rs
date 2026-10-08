//! The instruction set of a one-processor CRAY X-MP as one table.
//!
//! Authority: for the instructions the X-MP shares with the CRAY-1, the
//! CRAY-1 Hardware Reference Manual 2240004 rev C, section 4 and Appendix D
//! (serial 3 onward), whose page numbers the rows carry bare.  The
//! programmable clock (0014j4 to 0014j7) and the vector population
//! instructions (026ij1, 174ij1, 174ij2) came as options of the CRAY-1 of
//! 1982: their rows carry `flag::OPTION` and a page of revision F of that
//! manual (HR-0004, May 1982), starting with `F`.  What the X-MP added
//! (channel master clear, the cluster number, the mode switches 0023 to
//! 0027, the semaphores, the shared registers and the status register)
//! carries `flag::XMP` and a page of the CRAY X-MP Series Model 14 mainframe
//! reference manual CSM-0111000, starting with `X`.
//!
//! A field the manual marks `x` is ignored whatever it holds.  Where an
//! encoding is left undefined the table keeps the meaning of its
//! neighbours: 026ijk with k = 2 to 6 is the population count, 174ijk with
//! k = 3 to 7 the reciprocal, 003 with i other than 4, 6 and 7 is `VM Sj`.
//! 0014jk with k = 1 or 2 is a pass.
//!
//! # Instruction format
//!
//! A parcel is 16 bits: `g`(4) `h`(3) `i`(3) `j`(3) `k`(3), written as six
//! octal digits.  The 7-bit opcode is `gh`.  Two-parcel instructions add the
//! parcel `m`: `jkm` is a 22-bit constant or address, `ijkm` a branch
//! address.  Parcel addresses are word * 4 + parcel; parcel 0 of a word is
//! bits 63 to 48.
//!
//! # The table
//!
//! [`FORMS`] has one [`Form`] per line of Appendix D: opcode pattern, length,
//! CAL result and operand templates, description, and (through methods) the
//! functional unit, flags and registers read and written.  Rows come in three
//! kinds ([`Kind`]):
//!
//! * `Base`: the general instruction.  Each [`Op`] has exactly one and
//!   [`decode`] always returns it.
//! * `Special`: a special syntax form (dagger in Appendix D), the same
//!   instruction with fields fixed, such as `Ai Ak` for 030i0k.
//! * `Alt`: another spelling the assembler accepts, such as `ERR exp`.
//!
//! Rows that are not in Appendix D carry `flag::NOT_IN_APPENDIX_D`.  Two
//! exist so that every parcel decodes: 0014jk for k = 1 and 2
//! ([`Op::ClockPass`]) and 001ixx for i = 5 to 7 ([`Op::MonitorPass`], a
//! pass per section 4).  The others are
//! spellings from the CAL manual examples (`Ai #exp`, `Si #exp`, `Bjk,Ai ,`
//! and `Vi ,,Ak`) and `PASS` for 001000.
//!
//! # API
//!
//! ```text
//! decode(parcel0: u16, parcel1: Option<u16>) -> Decoded
//! length(parcel0: u16) -> usize                      // 1 or 2 parcels
//! encode(form: &Form, fields: Fields) -> Encoding    // canonical parcels
//! assemble(result: &str, operand: &str, eval) -> Result<Assembled, String>
//! assemble_numeric(result: &str, operand: &str) -> Result<Assembled, String>
//! disassemble(&Decoded) -> String                    // "RESULT    OPERAND"
//! disassemble_fields(&Decoded) -> (String, String)
//! base_form(op: Op) -> &'static Form
//! FORMS: &[Form]
//! ```
//!
//! [`Decoded`] has `op: Op`, `form: &'static Form` (the base row),
//! `parcels` (1 or 2), `parcel0`, the raw fields `g h i j k m`, and
//! `exp: Option<i64>`, the expression operand as CAL would write it: the
//! 24-bit parcel address of a branch (the low two bits of i and jkm), the
//! constant of
//! 020/040 (negative for 021/041), the signed displacement of 10h to 13h,
//! the constant of 022, or the count of a mask or shift.  The raw fields are
//! also combined by `jk()`, `jkm()` and `ijkm()`.  `reads()` and `writes()`
//! list the registers touched as [`Reg`] values with numbers filled in;
//! `canonical()` re-encodes with ignored fields zero and `is_canonical()`
//! tells whether that changes anything.
//!
//! [`Form`] has the public columns `op`, `kind`, `pattern`, `parcels`,
//! `result`, `operand`, `exp`, `sel`, `page`, `desc`, `mask`, `bits`,
//! `dont_care` and the methods `unit()`, `flags()`, `reads()`, `writes()`,
//! `is_branch()`, `is_conditional_branch()`, `reads_memory()`,
//! `writes_memory()`, `is_memory_reference()`, `is_monitor_only()`,
//! `is_exit()`, `is_vector()`, `uses_h()` to `uses_m()`, `exp_range()`,
//! `matches()`, `base()`, `example()` and `example_fields()`.  A special or
//! alternate row answers the semantic questions for its instruction.
//!
//! [`Fields`] is `h i j k m` with `set_jk`, `set_jkm` and
//! `set_exp(kind, value)`; [`Encoding`] is `parcel0` and `parcel1`.
//! `assemble` takes the expression evaluator as
//! `&mut dyn FnMut(&str, ExpUse) -> Result<ExpValue, String>`.
//!
//! # Special register values
//!
//! Register 0 named in the h, j or k field is not read; the machine uses a
//! constant instead: (Ah) = 0, (Aj) = 0, (Ak) = 1, (Sj) = 0, (Sk) = 2**63.
//! In the i field A0 and S0 are ordinary registers.  `Decoded::reads()`
//! leaves the substituted registers out.  CAL still writes them as `A0` or
//! `S0` (`S1 S0&S7` is 044107), which is why Appendix D has spellings such as
//! `Aj+1` and `Sj&SB`.
//!
//! # Round trips
//!
//! `disassemble` produces text that `assemble` turns back into the canonical
//! encoding of the same instruction.  CAL cannot always say which encoding
//! it wants, so the disassembler uses, in order: a special form, the general
//! form, an alternate spelling (`A1 #-6` forces the two-parcel 020 for a
//! constant that would fit 022), and as a last resort `VWD D'16/O'pppppp`,
//! which the assembler emits as a raw parcel.  The last resort is needed for
//! the rows without a CAL spelling and for 054/055 with i = 0, whose text
//! `S0 S0<n` belongs to 052/053.

mod code;
mod syntax;
mod table;

pub use code::{base_form, decode, encode, length, signed_octal, Decoded, Encoding, Fields, Reg};
pub use syntax::{
    assemble, assemble_numeric, disassemble, disassemble_fields, eval_number, is_register_name,
    is_reserved_name, is_symbol_char, quoted_mask, Assembled, ExpUse, ExpValue,
};
pub use table::{flag, ExpKind, Form, Kind, Op, RegRef, Sel, Unit, FORMS, OP_COUNT};

#[cfg(test)]
mod tests;
