//! Instruction-level reference model of the CRAY-1.
//!
//! This is the yardstick the hardware CPU is checked against by differential
//! testing.  Authority: CRAY-1 Hardware Reference Manual 2240004 rev C
//! (HRM); page numbers in this crate are its page numbers.  Decoding comes
//! from `cray1-isa`, floating point arithmetic from `cray1-fp`
//! (`Profile::Cray1`).
//!
//! # The machine
//!
//! * Memory: 2**20 words of 64 bits.  Word bit 63 is the manual's bit 0;
//!   parcel 0 of a word is bits 63 to 48.  An image is raw big-endian words
//!   loaded at word 0.
//! * The top 16 words are an I/O page that is not in the manual: `CON_STAT`,
//!   `CON_DATA`, `TEST_EXIT`, `CYCLES` (see the constants); the other twelve
//!   read 0 and ignore writes.  The page is addressed like memory, after
//!   base relocation and the limit check.
//! * Registers as in the manual: A (8 x 24), S (8 x 64), B (64 x 24),
//!   T (64 x 64), V (8 x 64 x 64), VL (7 bits), VM, P (22 bits), BA, LA
//!   (18 bits), XA (8 bits), M (4 bits), F (9 bits) and the real-time clock.
//! * No channels are attached: 0010, 0011 and 0012 do nothing and 033
//!   delivers 0.
//! * Dead start: the first step exchanges with the package at word 0 (XA is
//!   forced to 0, HRM page 3-40).  Words 0 to 15 then hold the reset-state
//!   package, which is noise on the real machine and undefined here.
//!
//! # API
//!
//! ```text
//! Machine::new() / Machine::with_image(bytes)
//! m.load_image(bytes) / m.load_words(addr, words)   raw image, word 0 up
//! m.push_input(bytes)                                console input
//! m.set_observer(Some(Box::new(|e: &Event| ...)))    every Event, in order
//! m.step() -> StepResult                             Running | Exit(code) | Error(TestError)
//! m.run(max_steps) -> RunResult                      Exit(code) | Limit | Error(TestError)
//! m.a(i) s(i) b(jk) t(jk) v(i, elem) vl() vm()       Option: None is undefined
//! m.p() ba() la() xa() m() f() rtc() monitor_mode() vector_length()
//! m.mem(addr) mem_written(addr) written_words() console() exit_code()
//! m.exchange_package() translate(rel) steps() instructions()
//! m.set_a(..) set_s set_b set_t set_v set_vl set_vm store(addr, value) start_at(p, ba, la, m)
//! report::trace_line(&Event) / report::state_text(&Machine, &RunResult)
//! mode::*, flag::*                                   bits of M and F
//! ```
//!
//! The binary `cray1-run` (see `src/bin/cray1-run.rs` for the file formats)
//! runs an image and writes the end state and a trace.
//!
//! A step is the dead start, one instruction (with the exchange it causes,
//! if any), or an exchange asked for by a flag that arrived set in a package.
//!
//! # Undefined values
//!
//! Every register and every memory word is either defined or undefined
//! (`None`).
//!
//! * Undefined at reset: A, S, B, T, V, VL, VM, the real-time clock and
//!   every memory word the image does not cover.  The dead start then loads
//!   A, S and VL from the package at 0 and leaves words 0 to 15 undefined.
//! * 072 (read the clock) and a read of `CYCLES` give undefined values.  A
//!   load from outside the field (BA, LA) gives an undefined value.  033
//!   gives a defined 0.
//! * An operation with an undefined input gives an undefined result, whole
//!   words at a time: a store of an undefined register makes the memory word
//!   undefined, a vector operation makes just the elements with an undefined
//!   operand undefined, 175 makes VM undefined if any tested element is.
//!   A merge (146, 147) takes the definedness of the operand it selects.
//!   Results the manual gives as constants are defined whatever the
//!   register holds: a logical product with a defined zero (044 with j = 0,
//!   `Vi 0` = 140i00), 045 with j = k, 046 and 047 with j = k not 0, and a
//!   032 product with a defined zero.
//!   An exchange package word is undefined if an A or S register in it is
//!   (or VL, for word 3).
//! * A test error (`TestError`, `ErrorKind::UndefinedValue`) stops the run
//!   when something that steers the machine is undefined: A0 or S0 in a
//!   conditional branch, Bjk in 005, the index or address register of a
//!   memory reference, the increment of 176/177, a shift count in an A
//!   register, the element number of 076/077, VL in a vector instruction,
//!   the length of a block transfer, (Aj) entered into XA, a write to
//!   `CON_DATA` or `TEST_EXIT`, an instruction parcel, one of words 0 to 3
//!   of an incoming exchange package, or a floating point operand while the
//!   floating point interrupt is enabled outside monitor mode.
//! * `ErrorKind::NotDefinedByManual` stops the run for 0023xx to 0027xx and
//!   for an instruction fetch outside the field in monitor mode.
//!
//! # Where the manual is silent or ambiguous
//!
//! * **Interrupts are precise.**  The manual lets up to two more parcels
//!   issue after a range error or a floating point error (pages 4-30, 4-39,
//!   4-48).  The model exchanges right after the instruction; the saved P is
//!   the address of the next instruction.  For a fetch outside the field
//!   the saved P is the address of the instruction that could not be read.
//! * **Range checks** (pages 3-43, 3-44, 4-3): a reference is inside the
//!   field if relative address + (BA) * 16 is below both (LA) * 16 and
//!   2**20.  The sum is not wrapped.  (Ah) + jkm and (A0) + n * (Ak) are
//!   24-bit two's complement sums.  A store outside the field does not
//!   alter memory in any mode; the flag sets only outside monitor mode.
//! * **Block and vector memory references outside the field**: in a user
//!   program everything from the first such reference to the end of the
//!   instruction is undefined (registers loaded, words stored inside the
//!   field).  In monitor mode, where no interrupt can cut the instruction
//!   short, the references inside the field are done normally.
//! * **Branch addresses** (page 4-4): P gets the low 22 bits.  If bit 2**22
//!   or 2**23 of the address of a taken branch is set (ijkm, or (Bjk) for
//!   005) the program range flag sets outside monitor mode; a branch that is
//!   not taken raises nothing.
//! * **Exchange package** (figure 3-8, page 3-37): the fields the figure
//!   leaves blank and the memory error fields E, S, R and RAB are stored as
//!   zero.  A package that arrives with a flag set in F causes another
//!   exchange at once outside monitor mode (page 3-36); in monitor mode
//!   only the memory error flag does.
//! * **Floating point error flag**: sets only when the floating point mode
//!   bit of M is set and monitor mode is not (page 3-21).  0021 and 0022
//!   work in any mode (page 4-11 has no monitor condition).
//! * **Recursive vector operations** (pages 3-14 to 3-16): see `vector`.
//!   The manual describes one operand register being the result register;
//!   when both are (i = j = k) both operands follow the operand/result rule.
//!   For the double shifts 152 and 153 with i = j the neighbouring element
//!   is the neighbour in the stream of operands the rule produces.
//! * **Real-time clock**: counts clock periods, so its value is undefined
//!   here even after 0014 has entered it.
//! * **154 to 157**: the text of page 4-59 says 155 and 157 subtract; its
//!   heading, the special cases on page 4-60, page 3-17 and Appendix D make
//!   154 and 155 sums and 156 and 157 differences, which is what is done.
//! * **Monitor mode bit**: bit 39 of word 2 of the package (page 3-35 and
//!   figure 3-8), which page 3-36 calls "the highest order bit of the M
//!   register".
//! * **A fetch outside the field in monitor mode** and **0023xx to 0027xx**
//!   stop the model (`ErrorKind::NotDefinedByManual`): the flag cannot set
//!   in monitor mode (page 3-36) and the manual does not say what is
//!   executed; the instructions are not in the manual at all.

mod event;
mod exec;
mod machine;
pub mod report;
pub mod vector;

pub use event::{Event, Observer};
pub use exec::{CONST_0_5, CONST_0_75_2_48, CONST_1_0, CONST_2_0, CONST_4_0, FP_PROFILE};
pub use machine::{
    flag, mode, vl_count, ErrorKind, Machine, RunResult, StepResult, TestError, A_MASK, CON_DATA,
    CON_STAT, CYCLES, IO_PAGE, MEMORY_WORDS, P_MASK, TEST_EXIT,
};

#[cfg(test)]
mod tests;
