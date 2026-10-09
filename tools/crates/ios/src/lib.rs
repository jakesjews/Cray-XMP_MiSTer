//! Reference model of the Cray I/O Subsystem (IOS), and with the CPU model
//! of a whole system: the machine the surviving COS 1.17 software runs on.
//!
//! * `Iop` is one I/O Processor, at instruction level (below).
//! * `System` is up to four of them with their channels, Buffer Memory,
//!   the Peripheral Expander with its tape and disk, the DD-29 disk drives,
//!   the consoles, and the mainframe (`cray_xmp_model::Machine` with the X-MP
//!   features) on the MIOP's channel pair and on the BIOP's 100 Mbyte
//!   channel.  The program `cray-xmp-sys` runs one from the files of the
//!   software and a script of what the operator types.
//!
//! Authority: HR-0030 rev B, the I/O Subsystem Model B Hardware Reference
//! Manual of May 1986; `[HW n-m]` in this crate is its printed page n-m.
//! `docs/spec/iop-cpu-spec.md` ("the spec") and
//! `docs/spec/ios-devices-spec.md` ("the devices spec") in the core's
//! repository condense it, compare it with the cray-sim simulator and list
//! what the COS 1.17 IOS software needs.  Where cray-sim and the manual
//! differ the manual is followed, except for the register protocols of the
//! Peripheral Expander's devices, which no manual at hand describes.
//!
//! # The processor
//!
//! * A 16-bit accumulator A with a carry bit C that is bit 2**16 of A in
//!   add, subtract and shift; B (9 bits); 512 operand registers; P (16
//!   bits); a 16-entry program exit stack addressed by E (4 bits); the
//!   System Interrupt Enable flag.
//! * Local Memory: 65,536 parcels of 16 bits.  An instruction is one
//!   parcel, f in bits 15 to 9 and d in bits 8 to 0, or two with a constant
//!   k in the second.
//! * Channels 0 to 4 are part of the processor: 0 IOR (interrupt request),
//!   1 PFR (program fetch request), 2 PXS (program exit stack), 3 LME
//!   (Local Memory error, which never reports one), 4 RTC (real-time
//!   clock).  Channels 5 to 47 octal are reached through the `Channels`
//!   trait, which also owns their Busy, Done and Interrupt Enable flags.
//! * Time is counted in clock periods of 12.5 ns.  `step` returns what the
//!   instruction took and the real-time clock counts it; its Done flag sets
//!   every 80,000 clock periods.  The times are estimates from the manual's
//!   sequence charts (see `CLOCK_PERIODS` in `iop.rs`).
//!
//! # API
//!
//! ```text
//! Iop::new()                                   power-up: all zero, Master Clear applied
//! iop.master_clear()                           held until the first interrupt
//! iop.memory_mut()                             Local Memory, for the dead start load and DMA
//! iop.step(&mut channels) -> u32               one instruction or one interrupt; clock periods
//! iop.advance(clock_periods)                   time passes, nothing executes
//! iop.p() a() b() c() e() operand(d) operands() exit_stack() memory()
//! iop.interrupt_enable() interrupt_enable_delayed() held() rtc() pfr_register()
//! iop.done(channel) enabled(channel)           flags of channels 0 to 4
//! iop.interrupt_request(&mut channels)         what IOR : 10 would read
//! iop.counters()                               instructions, interrupts, ...
//! iop.set_p(..) set_a set_c set_b set_operand(d, ..) start_at(p)   for tests
//! disassemble(parcel, k) / parcels(parcel)     one instruction as text; its length
//! ```
//!
//! # Dead start
//!
//! `master_clear` holds the processor with System Interrupt Enable set.
//! The system then loads Local Memory through `memory_mut` (the Buffer
//! Memory channel copies 65,536 parcels from Buffer Memory address 0) and
//! makes channel 5 request an interrupt: its Done and Interrupt Enable set,
//! so that `Channels::interrupt` returns 5.  The next `step` takes that
//! interrupt: it clears System Interrupt Enable, advances E to 1, stores P
//! there and starts the program at the address in exit stack location 0,
//! which Master Clear made 0 [HW 5-11, 5-15].
//!
//! # Where the manual is silent
//!
//! The choices are those of the spec, part 11, and each is noted where it
//! is made in `iop.rs`:
//!
//! * An EXIT with E = 0 goes to the address in location 0 with E still 0
//!   and the Exit Stack Boundary flag set; nothing is pushed and System
//!   Interrupt Enable is not changed (Q1, Q2).
//! * Every return jump that advances E to 14 sets the boundary flag, not
//!   only 076 and 077 (Q3).  An interrupt never does.
//! * Shift counts are the low 5 bits of d or B.  A circular shift of 18 to
//!   31 places turns the 17 bits by the count modulo 17 (Q4, Q5).
//! * After 003, System Interrupt Enable sets at the completion of the next
//!   instruction that is not 001, 003, 040 to 043, 070 to 137 or 140 to 177
//!   (Q6).
//! * The Program Fetch Request flag sets only if the branch is taken, and
//!   the branch completes before the interrupt (Q7, Q8).
//! * A channel number above 47 octal is an empty channel: flags clear,
//!   functions ignored, zero read (Q9).
//! * Functions 10 to 13 clear the carry (Q10).
//! * State after Master Clear: see `Iop::master_clear` (Q11, Q12).
//! * Instructions are always fetched from Local Memory; the instruction
//!   stack is not modelled, so a store into the next parcels takes effect
//!   (Q13).
//! * Interrupts are taken between instructions, and 002 is atomic: the
//!   skip the manual warns of after 002 [HW 6-5] does not happen (Q15).

mod devices;
mod disasm;
mod expander;
mod image;
mod iop;
pub mod replay;
mod screen;
mod system;

pub use devices::DD29_SECTOR_BYTES;
pub use disasm::{disassemble, parcels};
pub use expander::DISK_SECTOR_BYTES;
pub use image::{Image, Tape, TapeState};
pub use iop::{
    channel, Channels, Counters, Iop, NoChannels, CHANNELS, EXIT_STACK, MEMORY_PARCELS,
    OPERAND_REGISTERS, RTC_PERIOD,
};
pub use screen::{Cell, Screen, Switches};
pub use system::{Config, Flags, System, Timing};

#[cfg(test)]
mod tests;
