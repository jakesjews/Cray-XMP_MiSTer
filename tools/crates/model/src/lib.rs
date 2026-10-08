//! Instruction-level reference model of the CPU of a one-processor CRAY X-MP.
//!
//! This is the yardstick the hardware CPU is checked against by differential
//! testing.  Authorities, and how this crate cites them:
//!
//! * "X" and a page: the CRAY X-MP Series Model 14 mainframe reference
//!   manual CSM-0111000, for the machine and everything it has of its own.
//!   `docs/spec/machine-spec.md` in the core's repository has what the
//!   operating system COS was seen to use of it.
//! * A bare page number ("page 4-25", "HRM"): the CRAY-1 Hardware Reference
//!   Manual 2240004 rev C, whose text for the instructions the two machines
//!   share is the fuller one.
//! * "rev F": revision F of that manual (HR-0004, May 1982), for the
//!   programmable clock and the vector population instructions.
//!
//! Decoding comes from `cray-xmp-isa`, floating point arithmetic from
//! `cray-xmp-fp`.
//!
//! # The machine
//!
//! * Memory: 2**22 words of 64 bits (X 2-9).  Word bit 63 is the manual's
//!   bit 0; parcel 0 of a word is bits 63 to 48.  An image is raw big-endian
//!   words loaded at word 0.
//! * The top 16 words are an I/O page that is not in the manual: `CON_STAT`,
//!   `CON_DATA`, `TEST_EXIT`, `CYCLES` (see the constants); the other twelve
//!   read 0 and ignore writes.  The page is addressed like memory, after
//!   base relocation and the limit check.  `Machine::set_system` makes the
//!   words plain memory, for a machine that is part of a system.
//! * Registers: A (8 x 24), S (8 x 64), B (64 x 24), T (64 x 64),
//!   V (8 x 64 x 64), VL (7 bits), VM, P (24 bits), the base and limit
//!   addresses of the instruction field (IBA, ILA) and of the data field
//!   (DBA, DLA), 19 bits each in units of 32 words, XA (8 bits), the modes,
//!   the flags, the cluster number and the real-time clock.
//! * Three clusters of shared registers: eight SB, eight ST and 32
//!   semaphores each.
//! * The 6 Mbyte channels 10 to 17 octal (see `channel`).
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
//! m.p() ba() la() data_field() xa() m() m1() f() cluster() rtc()
//! m.sb(cln, j) st(cln, j) sm(cln, n)                 the shared registers
//! m.monitor_mode() vector_length()
//! m.mem(addr) mem_written(addr) written_words() console() exit_code()
//! m.exchange_package() translate(rel) translate_fetch(word) steps() instructions()
//! m.set_a(..) set_s set_b set_t set_v set_vl set_vm store(addr, value)
//! m.start_at(p, ba, la, m) set_data_field(dba, dla) set_cluster(cln)
//! m.channel(n) channel_input(n, parcel) channel_disconnect(n) channel_output(n)
//! m.set_timing(clock_periods) request_mcu_interrupt() set_io_request(level)
//! report::trace_line(&Event) / report::state_text(&Machine, &RunResult)
//! mode::*, mode1::*, flag::*                         bits of the modes and flags
//! ```
//!
//! The binary `cray-xmp-run` (see `src/bin/cray-xmp-run.rs` for the file formats)
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
//! * Undefined at reset: A, S, B, T, V, VL, VM, the shared registers and
//!   semaphores, the real-time clock and every memory word the image does
//!   not cover.  The dead start then loads A, S and VL from the package at 0
//!   and leaves words 0 to 15 undefined.
//! * 072 (read the clock) and a read of `CYCLES` give undefined values.
//! * An operation with an undefined input gives an undefined result, whole
//!   words at a time: a store of an undefined register makes the memory word
//!   undefined, a vector operation makes just the elements with an undefined
//!   operand undefined, 175 makes VM undefined if any tested element is.
//!   A merge (146, 147) takes the definedness of the operand it selects.
//!   Results the manual gives as constants are defined whatever the
//!   register holds: a logical product with a defined zero (044 with j = 0,
//!   `Vi 0` = 140i00), 045 with j = k, 046 and 047 with j = k not 0, the
//!   differences 031 and 061 with j = k not 0, the vector forms 145 and 157
//!   with j = k (`Vi Vj\Vj` clears a register in later CAL), and a 032
//!   product with a defined zero.
//!   An exchange package word is undefined if an A or S register in it is
//!   (or VL, for word 3).
//! * A test error (`TestError`, `ErrorKind::UndefinedValue`) stops the run
//!   when something that steers the machine is undefined: A0 or S0 in a
//!   conditional branch, Bjk in 005, the index or address register of a
//!   memory reference, the increment of 176/177, a shift count in an A
//!   register, the element number of 076/077, VL in a vector instruction,
//!   the length of a block transfer, (Aj) entered into XA or naming a
//!   channel, a semaphore tested by 0034, a write to `CON_STAT`, `CON_DATA`
//!   or `TEST_EXIT`, an instruction parcel, one of words 0 to 5 of an
//!   incoming exchange package, or a floating point operand while the
//!   floating point interrupt is enabled outside monitor mode.
//! * `ErrorKind::NotDefinedByManual` stops the run for an instruction fetch
//!   outside the field in monitor mode, for a test and set of a set
//!   semaphore in monitor mode, and for 010 to 017 with the high bit of i
//!   set.
//! * `ErrorKind::TimeDependent` stops the run when a program outside
//!   monitor mode could be interrupted by the programmable clock.
//!
//! # Where the manuals are silent or ambiguous
//!
//! * **Interrupts are precise.**  The manual lets up to two more parcels
//!   issue after a range error or a floating point error (pages 4-30, 4-39,
//!   4-48).  The model exchanges right after the instruction; the saved P is
//!   the address of the next instruction.  For a fetch outside the field
//!   the saved P is the address of the instruction that could not be read.
//! * **Range checks** (X 3-19 to 3-21): a data reference is inside the field
//!   if the low 22 bits of the relative address plus (DBA) * 32 are below
//!   both (DLA) * 32 and 2**22; a fetch likewise with IBA and ILA.  The sum
//!   is not wrapped.  (Ah) + jkm and (A0) + n * (Ak) are 24-bit two's
//!   complement sums.  A store outside the field does not alter memory and a
//!   load gives zero, in any mode.  The operand range flag sets only outside
//!   monitor mode and only with the mode bit IOR (0023 sets it, 0024 clears
//!   it).
//! * **Block and vector memory references outside the field** go on to the
//!   end of the instruction: each such read "issues and completes, but a
//!   zero value is transferred", each such write does nothing (X 3-19).
//! * **Branch addresses** (X 3-6, 5-21): P gets all 24 bits and a branch
//!   raises no flag itself; a fetch outside the instruction field is the
//!   program range error.
//! * **Exchange package** (X figure 3-3 and table 3-1): the processor
//!   number, the memory error fields, VNU, ESVL and EAM are stored as zero.
//!   A package that arrives with a flag set causes another exchange at once
//!   outside monitor mode (page 3-36); in monitor mode only the memory error
//!   flag does.
//! * **Floating point error flag**: sets only when the floating point mode
//!   bit is set and monitor mode is not (page 3-21).  The status bit FPS
//!   records an error whatever the modes are; 0021 and 0022 clear it and
//!   work in any mode (X 5-15).
//! * **A vector register that is operand and result** of one instruction
//!   (X 3-33): every operation takes the elements as they were before the
//!   instruction.  See `vector`.
//! * **Shared registers and semaphores** (X 2-17): in cluster 0 the stores
//!   do nothing and the loads give zero.  A test and set (0034) of a set
//!   semaphore cannot issue: outside monitor mode the deadlock flag sets and
//!   the package stored has P at the instruction and the WS bit; in monitor
//!   mode, where nothing on a one-processor machine can clear the
//!   semaphore, the model stops.
//! * **The status register** (073i01, X 5-60) has ones in its low half and
//!   shows the cluster number only in monitor mode.
//! * **0025, 0026 and 0027**: the first two switch the BDM bit and the third
//!   passes; none has another effect here.
//! * **Real-time clock**: counts clock periods, so its value is undefined
//!   here even after 0014 has entered it.
//! * **Programmable clock** (rev F pages 4-10, 6-23): the interval and the
//!   countdown are clock period counts and are not modelled.  The model
//!   keeps the enable (0014j6, 0014j7) and whether a request may be set
//!   (from 0014j6 until a 0014j5 that follows a 0014j7).  In monitor mode
//!   that changes nothing; a program outside monitor mode stops the model
//!   while a request may be set.  Enable and request start cleared.
//!   In timed mode (`Machine::set_timing`) both clocks count.
//! * **Encodings the manuals leave undefined**: 0014jk with k = 1 or 2 is a
//!   pass; 026ijk with k = 2 to 6 is the population count; 174ijk with k = 3
//!   to 7 is the reciprocal.
//! * **154 to 157**: the text of page 4-59 says 155 and 157 subtract; its
//!   heading, the special cases on page 4-60, page 3-17 and Appendix D make
//!   154 and 155 sums and 156 and 157 differences, which is what is done.
//! * **A fetch outside the field in monitor mode** stops the model
//!   (`ErrorKind::NotDefinedByManual`): the flag cannot set in monitor mode
//!   (page 3-36) and the manual does not say what is executed.
//! * **010 to 017 with the high bit of i set** are `Ah exp`, a 24-bit
//!   constant, on an X-MP with extended addressing, which this machine is
//!   not: the model stops.

mod channel;
mod event;
mod exec;
mod machine;
pub mod report;
pub mod vector;

pub use channel::ChannelState;
pub use event::{Event, Observer};
pub use exec::{CONST_0_5, CONST_0_75_2_48, CONST_1_0, CONST_2_0, CONST_4_0};
pub use machine::{
    flag, mode, mode1, vl_count, ErrorKind, Machine, RunResult, StepResult, TestError, A_MASK,
    CON_DATA, CON_STAT, CYCLES, IO_PAGE, MEMORY_WORDS, TEST_EXIT,
};

#[cfg(test)]
mod tests;
