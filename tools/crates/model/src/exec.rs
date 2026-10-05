//! Instruction semantics for everything but the vector instructions.
//!
//! Page numbers are those of the CRAY-1 Hardware Reference Manual 2240004
//! rev C.

use crate::event::Event;
use crate::machine::{flag, mode, mode1, ErrorKind, Machine, TestError, A_MASK, P_MASK};
use cray1_fp::{fadd, fmul, frecip, fsub, FpResult, MulKind, Profile};
use cray1_isa::{Cpu, Decoded, Op};

/// The floating point arithmetic used everywhere (a project decision: it
/// currently equals the X-MP arithmetic, see `cray1-fp`).
pub const FP_PROFILE: Profile = Profile::Cray1;

/// 071i3x: 0.75 * 2**48, "0.6 x 2**60 (octal)" on page 4-44.
pub const CONST_0_75_2_48: u64 = 0o40060 << 48 | 0o6 << 45;
/// 071i4x: 0.5, "0.4 x 2**0 (octal)".
pub const CONST_0_5: u64 = 0o40000 << 48 | 0o4 << 45;
/// 071i5x: 1.0.
pub const CONST_1_0: u64 = 0o40001 << 48 | 0o4 << 45;
/// 071i6x: 2.0.
pub const CONST_2_0: u64 = 0o40002 << 48 | 0o4 << 45;
/// 071i7x: 4.0.
pub const CONST_4_0: u64 = 0o40003 << 48 | 0o4 << 45;

fn sign_extend_24(a: u32) -> u64 {
    ((a as i32) << 8 >> 8) as i64 as u64
}

/// 056: the high word of (hi, lo) shifted left `count` places (page 4-37).
pub(crate) fn double_shift_left(hi: u64, lo: u64, count: u32) -> u64 {
    if count > 127 {
        0
    } else {
        ((((hi as u128) << 64 | lo as u128) << count) >> 64) as u64
    }
}

/// 057: the low word of (hi, lo) shifted right `count` places (page 4-37).
pub(crate) fn double_shift_right(hi: u64, lo: u64, count: u32) -> u64 {
    if count > 127 {
        0
    } else {
        (((hi as u128) << 64 | lo as u128) >> count) as u64
    }
}

fn both<T: Copy, U: Copy>(a: Option<T>, b: Option<U>) -> Option<(T, U)> {
    Some((a?, b?))
}

/// A logical product.  A defined zero operand gives zero whatever the other
/// operand is, so that the clears the manual lists (044 with j = 0, `Vi 0`
/// = 140i00) are defined even when the other register is not.
pub(crate) fn and_values(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(0), _) | (_, Some(0)) => Some(0),
        (Some(a), Some(b)) => Some(a & b),
        _ => None,
    }
}

impl Machine {
    // ---- special register values (page 4-5): a register 0 named in the h,
    // j or k field is not read

    pub(crate) fn ah(&self, d: &Decoded) -> Option<u32> {
        if d.h == 0 {
            Some(0)
        } else {
            self.a[d.h as usize]
        }
    }
    pub(crate) fn aj(&self, d: &Decoded) -> Option<u32> {
        if d.j == 0 {
            Some(0)
        } else {
            self.a[d.j as usize]
        }
    }
    pub(crate) fn ak(&self, d: &Decoded) -> Option<u32> {
        if d.k == 0 {
            Some(1)
        } else {
            self.a[d.k as usize]
        }
    }
    pub(crate) fn sj(&self, d: &Decoded) -> Option<u64> {
        if d.j == 0 {
            Some(0)
        } else {
            self.s[d.j as usize]
        }
    }
    pub(crate) fn sk(&self, d: &Decoded) -> Option<u64> {
        if d.k == 0 {
            Some(1 << 63)
        } else {
            self.s[d.k as usize]
        }
    }

    /// Deliver a floating point result.  A range error raises the floating
    /// point error flag if the mode flag is set and monitor mode is not
    /// (page 3-21).  With an undefined operand the result is undefined, and
    /// if an interrupt could have been taken the run cannot go on.
    pub(crate) fn fp_result(&mut self, r: Option<FpResult>) -> Result<Option<u64>, TestError> {
        let armed = self.m & mode::FLOATING_POINT != 0 && !self.monitor_mode();
        // X-MP: the status bit records an error whatever the modes are
        match r {
            Some(r) if r.range_error => self.fps = Some(true),
            None if self.fps != Some(true) => self.fps = None,
            _ => {}
        }
        match r {
            Some(r) => {
                if r.range_error && armed {
                    self.interrupt(flag::FLOATING_POINT);
                }
                Ok(Some(r.value))
            }
            None if armed => Err(self.error(
                ErrorKind::UndefinedValue,
                "floating point operand while the floating point interrupt is enabled",
            )),
            None => Ok(None),
        }
    }

    /// Set P from a branch address.  P holds the low 22 bits; an address
    /// with bit 2**22 or 2**23 set is a program range error (page 4-4).
    /// The X-MP's P takes all 24 bits and the fetch finds any range error.
    fn branch(&mut self, target: u32) {
        self.p = target & self.p_mask();
        if self.cpu == Cpu::Cray1 && target & !P_MASK & A_MASK != 0 {
            self.interrupt(flag::PROGRAM_RANGE);
        }
    }

    /// X-MP: 010 to 017 with the high bit of i set are not branches there
    /// but `Ah exp`, a 24-bit constant, which this machine does not have.
    fn no_long_constant(&self, d: &Decoded) -> Result<(), TestError> {
        if self.cpu == Cpu::Xmp && d.i & 4 != 0 {
            return Err(self.error(
                ErrorKind::NotDefinedByManual,
                "01hijkm with the high bit of i set (the X-MP's 24-bit constant to Ah)",
            ));
        }
        Ok(())
    }

    /// X-MP: the shared registers of the current cluster; `None` in cluster
    /// 0, where "instructions regarding the shared registers become no-ops,
    /// except for the instructions returning values to Ai or Si, which
    /// return a zero value" (CSM-0111000 page 2-17).
    fn cluster_index(&self) -> Option<usize> {
        (self.cln != 0).then(|| self.cln as usize - 1)
    }

    /// 000 and 004: set the flag unless in monitor mode, then exchange in
    /// any mode.  P already points one parcel past the exit (page 4-7).
    fn exit_instruction(&mut self, flag: u16) {
        self.interrupt(flag);
        self.want_exchange = true;
    }

    /// The block length of 034 to 037: the low seven bits of (Ai)
    /// (page 4-30); with i = 0 that is A0, which is also the address.
    fn block(&self, d: &Decoded) -> Result<(u32, usize), TestError> {
        let a0 = self.need(self.a[0], "A0 (block transfer address)")?;
        let count = self.need(self.a[d.i as usize], "Ai (block transfer length)")?;
        Ok((a0, (count & 0x7f) as usize))
    }

    /// 034 and 036: memory to B or T registers, circular after register 77.
    fn block_load(&mut self, d: &Decoded, t: bool) -> Result<(), TestError> {
        let (a0, count) = self.block(d)?;
        let mut lost = false;
        for n in 0..count {
            let reg = (d.jk() as usize + n) & 0o77;
            let value = self.transfer_read(a0.wrapping_add(n as u32) & A_MASK, &mut lost);
            if t {
                self.set_t(reg, value);
            } else {
                self.set_b(reg, value.map(|w| w as u32));
            }
        }
        Ok(())
    }

    /// 035 and 037: B or T registers to memory.
    fn block_store(&mut self, d: &Decoded, t: bool) -> Result<(), TestError> {
        let (a0, count) = self.block(d)?;
        let mut lost = false;
        for n in 0..count {
            let reg = (d.jk() as usize + n) & 0o77;
            let value = if t {
                self.t[reg]
            } else {
                self.b[reg].map(|v| v as u64)
            };
            self.transfer_write(a0.wrapping_add(n as u32) & A_MASK, value, &mut lost)?;
        }
        Ok(())
    }

    /// The address (Ah) + jkm of 10h to 13h: 24-bit two's complement
    /// arithmetic on the sign-extended displacement (page 4-47).
    fn scalar_address(&self, d: &Decoded) -> Result<u32, TestError> {
        let ah = self.need(self.ah(d), "Ah (memory address index)")?;
        Ok(ah.wrapping_add(d.exp.unwrap_or(0) as u32) & A_MASK)
    }

    pub(crate) fn execute(&mut self, d: &Decoded) -> Result<(), TestError> {
        let (i, j) = (d.i as usize, d.j as usize);
        let jk = d.jk() as usize;
        let monitor = self.monitor_mode();
        match d.op {
            // ---- 000 to 004 (pages 4-7 to 4-13)
            Op::Err => self.exit_instruction(flag::ERROR_EXIT),
            // The CRAY-1 setting has no channels attached.  On the X-MP these
            // work channels 10 to 17 (see `channel`); a pass outside monitor
            // mode, with j = 0, and for any other channel number.
            Op::SetCa | Op::SetCl | Op::ClearCi | Op::ChanMc => {
                if monitor && self.cpu == Cpu::Xmp && d.j != 0 {
                    let number = self.need(self.aj(d), "Aj (channel number)")?;
                    if let Some(n) = self.channel_index(number) {
                        match d.op {
                            Op::SetCa => {
                                let ak = self.need(self.ak(d), "Ak (channel address)")?;
                                self.channel_activate(n, ak);
                            }
                            Op::SetCl => {
                                let ak = self.need(self.ak(d), "Ak (channel limit)")?;
                                self.channels[n].cl = ak & 0x3f_ffff;
                            }
                            _ => self.channel_clear(n, d.op == Op::ChanMc),
                        }
                    }
                }
            }
            Op::ClockPass | Op::MonitorPass => {}
            Op::SetXa => {
                if monitor {
                    // bits 2**11 to 2**4 of (Aj); cleared if j = 0 (page 4-8)
                    let aj = self.need(self.aj(d), "Aj entered into XA")?;
                    self.set_xa((aj >> 4) as u8);
                }
            }
            Op::SetRt => {
                if monitor {
                    let value = self.sj(d);
                    if self.clock_step.is_some() {
                        let value = self.need(value, "Sj entered into the real-time clock")?;
                        self.rtc_offset = value.wrapping_sub(self.time);
                    }
                    self.emit(Event::Rtc(value));
                }
            }
            // ---- the programmable clock (rev F pages 4-10 and 6-23).  The
            // interval and the countdown are clock period counts and are not
            // modelled; what is kept is whether a request may be set.
            Op::SetPci => {
                if monitor && self.clock_step.is_some() {
                    // the interval and the countdown take the low 32 bits;
                    // the countdown reaches zero that many clock periods on
                    // and the request is made in the one after
                    let value = self.need(self.sj(d), "Sj entered into the interrupt interval")?;
                    self.clock_interval = value as u32;
                    self.clock_next = self.time + self.clock_interval as u64 + 1;
                }
            }
            Op::Cci => {
                if monitor {
                    // while enabled the next request can follow at once
                    self.clock_request_possible = self.clock_enabled;
                    self.clock_request = false;
                }
            }
            Op::Eci => {
                if monitor {
                    self.clock_enabled = true;
                    self.clock_request_possible = true;
                }
            }
            Op::Dci => {
                if monitor {
                    // a request that is already set stays until 0014j5
                    self.clock_enabled = false;
                }
            }
            Op::SetVl => {
                // the low seven bits of (Ak), 1 if k = 0 (page 4-10)
                let value = self.ak(d).map(|a| a as u8 & 0x7f);
                self.set_vl(value);
            }
            Op::Efi | Op::Dfi => {
                if d.op == Op::Efi {
                    self.set_m(self.m | mode::FLOATING_POINT);
                } else {
                    self.set_m(self.m & !mode::FLOATING_POINT);
                }
                // X-MP: both also clear the floating point error status
                self.fps = Some(false);
            }

            // ---- X-MP only (CSM-0111000 pages 5-11 to 5-17, 5-32, 5-34, 5-59)
            Op::SetCln => {
                if monitor {
                    self.cln = d.j & 3;
                }
            }
            Op::Eri => self.set_m(self.m | mode::OPERAND_RANGE),
            Op::Dri => self.set_m(self.m & !mode::OPERAND_RANGE),
            Op::Ebm => self.m1 |= mode1::BIDIRECTIONAL,
            Op::Dbm => self.m1 &= !mode1::BIDIRECTIONAL,
            // memory references are complete after every instruction here
            Op::Cmr => {}
            Op::SemTestSet => {
                if let Some(c) = self.cluster_index() {
                    let n = jk & 0o37;
                    let set = self.need(self.sm[c][n], "semaphore tested by 0034")?;
                    if !set {
                        self.sm[c][n] = Some(true);
                    } else if monitor {
                        // nothing can clear it on a one-processor machine
                        return Err(self.error(
                            ErrorKind::NotDefinedByManual,
                            "test and set of a set semaphore in monitor mode: it waits for good",
                        ));
                    } else {
                        // the instruction does not issue: deadlock interrupt
                        // with P at this instruction
                        self.p = self.cur_p;
                        self.waiting_semaphore = true;
                        self.interrupt(flag::DEADLOCK);
                    }
                }
            }
            Op::SemClear | Op::SemSet => {
                if let Some(c) = self.cluster_index() {
                    self.sm[c][jk & 0o37] = Some(d.op == Op::SemSet);
                }
            }
            Op::AFromSb => {
                let value = match self.cluster_index() {
                    Some(c) => self.sb[c][j],
                    None => Some(0),
                };
                self.set_a(i, value);
            }
            Op::SbFromA => {
                if let Some(c) = self.cluster_index() {
                    self.sb[c][j] = self.a[i];
                }
            }
            Op::SFromSt => {
                let value = match self.cluster_index() {
                    Some(c) => self.st[c][j],
                    None => Some(0),
                };
                self.set_s(i, value);
            }
            Op::StFromS => {
                if let Some(c) = self.cluster_index() {
                    self.st[c][j] = self.s[i];
                }
            }
            Op::SFromSm => {
                // SM0 is the sign bit; the low 32 bits are zero
                let value = match self.cluster_index() {
                    Some(c) => self.sm[c]
                        .iter()
                        .enumerate()
                        .try_fold(0u64, |word, (n, bit)| {
                            bit.map(|b| word | (b as u64) << (63 - n))
                        }),
                    None => Some(0),
                };
                self.set_s(i, value);
            }
            Op::SmFromS => {
                if let Some(c) = self.cluster_index() {
                    for n in 0..32 {
                        self.sm[c][n] = self.s[i].map(|s| s >> (63 - n) & 1 != 0);
                    }
                }
            }
            Op::SFromSr => {
                // the status register: ones in the low half; the processor
                // number (always 0) and the cluster number only in monitor mode
                let cln = if monitor { self.cln as u64 } else { 0 };
                let value = self.fps.map(|fps| {
                    ((self.cln != 0) as u64) << 63
                        | (self.ps as u64) << 57
                        | (fps as u64) << 51
                        | ((self.m & mode::FLOATING_POINT != 0) as u64) << 50
                        | ((self.m & mode::OPERAND_RANGE != 0) as u64) << 49
                        | ((self.m1 & mode1::BIDIRECTIONAL != 0) as u64) << 48
                        | cln << 32
                        | 0xffff_ffff
                });
                self.set_s(i, value);
            }
            Op::Undefined => {
                return Err(self.error(
                    ErrorKind::NotDefinedByManual,
                    "instruction 0023xx to 0027xx",
                ));
            }
            Op::SetVm => {
                let value = self.sj(d);
                self.set_vm(value);
            }
            Op::Ex => self.exit_instruction(flag::NORMAL_EXIT),

            // ---- 005 to 017: branches (pages 4-14 to 4-18)
            Op::JumpB => {
                let target = self.need(self.b[jk], "Bjk (branch address)")?;
                self.branch(target);
            }
            Op::Jump => self.branch(d.ijkm() & A_MASK),
            Op::ReturnJump => {
                // B00 gets the address of the following parcel
                self.set_b(0, Some(self.p));
                self.branch(d.ijkm() & A_MASK);
            }
            Op::Jaz | Op::Jan | Op::Jap | Op::Jam => {
                self.no_long_constant(d)?;
                let a0 = self.need(self.a[0], "A0 (branch condition)")?;
                let taken = match d.op {
                    Op::Jaz => a0 == 0,
                    Op::Jan => a0 != 0,
                    Op::Jap => a0 & 0x80_0000 == 0,
                    _ => a0 & 0x80_0000 != 0,
                };
                if taken {
                    self.branch(d.ijkm() & A_MASK);
                }
            }
            Op::Jsz | Op::Jsn | Op::Jsp | Op::Jsm => {
                self.no_long_constant(d)?;
                let s0 = self.need(self.s[0], "S0 (branch condition)")?;
                let taken = match d.op {
                    Op::Jsz => s0 == 0,
                    Op::Jsn => s0 != 0,
                    Op::Jsp => s0 >> 63 == 0,
                    _ => s0 >> 63 != 0,
                };
                if taken {
                    self.branch(d.ijkm() & A_MASK);
                }
            }

            // ---- 020 to 033: A registers (pages 4-19 to 4-28)
            Op::ImmA => self.set_a(i, Some(d.jkm())),
            Op::ImmANot => self.set_a(i, Some(!d.jkm() & A_MASK)),
            Op::ImmAShort => self.set_a(i, Some(jk as u32)),
            Op::AFromS => {
                let value = self.sj(d).map(|s| s as u32 & A_MASK);
                self.set_a(i, value);
            }
            Op::AFromB => self.set_a(i, self.b[jk]),
            Op::BFromA => self.set_b(jk, self.a[i]),
            Op::PopCount => {
                let value = self.sj(d).map(|s| s.count_ones());
                self.set_a(i, value);
            }
            Op::PopParity => {
                // only the low bit of the count (rev F page 4-25)
                let value = self.sj(d).map(|s| s.count_ones() & 1);
                self.set_a(i, value);
            }
            Op::LeadingZeros => {
                let value = self.sj(d).map(|s| s.leading_zeros());
                self.set_a(i, value);
            }
            Op::AddA => {
                let value = both(self.aj(d), self.ak(d)).map(|(a, b)| a.wrapping_add(b) & A_MASK);
                self.set_a(i, value);
            }
            Op::SubA => {
                // a register less itself is zero whatever it holds
                let value = if d.j == d.k && d.j != 0 {
                    Some(0)
                } else {
                    both(self.aj(d), self.ak(d)).map(|(a, b)| a.wrapping_sub(b) & A_MASK)
                };
                self.set_a(i, value);
            }
            Op::MulA => {
                // "(Ai) = 0 if j = 0" (page 4-26), whatever (Ak) is
                let value = match (self.aj(d), self.ak(d)) {
                    (Some(0), _) | (_, Some(0)) => Some(0),
                    (Some(a), Some(b)) => Some(a.wrapping_mul(b) & A_MASK),
                    _ => None,
                };
                self.set_a(i, value);
            }
            // 033.  No channels on the CRAY-1 setting: no interrupt request,
            // current address 0, no error.  Not privileged.
            Op::ChanInt => self.set_a(i, Some(self.channel_interrupting())),
            Op::ChanAddr | Op::ChanErr => {
                let value = if self.cpu == Cpu::Xmp {
                    let number = self.need(self.aj(d), "Aj (channel number)")?;
                    match self.channel_index(number) {
                        Some(n) if d.op == Op::ChanAddr => self.channels[n].ca,
                        Some(n) => self.channels[n].error as u32,
                        None => 0,
                    }
                } else {
                    0
                };
                self.set_a(i, Some(value));
            }

            // ---- 034 to 037: block transfers (pages 4-29, 4-30)
            Op::BLoad => self.block_load(d, false)?,
            Op::BStore => self.block_store(d, false)?,
            Op::TLoad => self.block_load(d, true)?,
            Op::TStore => self.block_store(d, true)?,

            // ---- 040 to 051: S immediates, masks, logical (pages 4-31 to 4-35)
            Op::ImmS => self.set_s(i, Some(d.jkm() as u64)),
            Op::ImmSNot => self.set_s(i, Some(!(d.jkm() as u64))),
            Op::MaskRight => self.set_s(i, Some(u64::MAX >> jk)),
            Op::MaskLeft => self.set_s(i, Some(!(u64::MAX >> jk))),
            Op::AndS | Op::AndNotS | Op::XorS | Op::EqvS | Op::OrS => {
                let (a, b) = (self.sj(d), self.sk(d));
                // The constants the manual lists for equal designators
                // (pages 4-33, 4-34) do not depend on the register, so they
                // are defined even when it is not.
                let same = d.j == d.k;
                let value = match d.op {
                    Op::AndS => and_values(a, b),
                    Op::AndNotS if same => Some(0),
                    Op::AndNotS => and_values(a, b.map(|b| !b)),
                    Op::XorS if same && d.j != 0 => Some(0),
                    Op::EqvS if same && d.j != 0 => Some(u64::MAX),
                    Op::XorS => both(a, b).map(|(a, b)| a ^ b),
                    Op::EqvS => both(a, b).map(|(a, b)| !(a ^ b)),
                    _ => both(a, b).map(|(a, b)| a | b),
                };
                self.set_s(i, value);
            }
            Op::MergeS => {
                let value = match (self.s[i], self.sj(d), self.sk(d)) {
                    (Some(si), Some(sj), Some(sk)) => Some(sj & sk | si & !sk),
                    _ => None,
                };
                self.set_s(i, value);
            }

            // ---- 052 to 057: S shifts (pages 4-36, 4-37)
            Op::ShlS0 => self.set_s(0, self.s[i].map(|s| s << jk)),
            Op::ShrS0 => self.set_s(
                0,
                self.s[i].map(|s| if jk == 0 { 0 } else { s >> (64 - jk) }),
            ),
            Op::ShlS => self.set_s(i, self.s[i].map(|s| s << jk)),
            Op::ShrS => self.set_s(
                i,
                self.s[i].map(|s| if jk == 0 { 0 } else { s >> (64 - jk) }),
            ),
            Op::ShlDS | Op::ShrDS => {
                let count = self.need(self.ak(d), "Ak (shift count)")?;
                let value = both(self.s[i], self.sj(d)).map(|(si, sj)| {
                    if d.op == Op::ShlDS {
                        double_shift_left(si, sj, count)
                    } else {
                        double_shift_right(sj, si, count)
                    }
                });
                self.set_s(i, value);
            }

            // ---- 060 to 070: S arithmetic (pages 4-38 to 4-42)
            Op::AddS => {
                let value = both(self.sj(d), self.sk(d)).map(|(a, b)| a.wrapping_add(b));
                self.set_s(i, value);
            }
            Op::SubS => {
                // a register less itself is zero whatever it holds
                let value = if d.j == d.k && d.j != 0 {
                    Some(0)
                } else {
                    both(self.sj(d), self.sk(d)).map(|(a, b)| a.wrapping_sub(b))
                };
                self.set_s(i, value);
            }
            Op::FAddS | Op::FSubS | Op::FMulS | Op::HMulS | Op::RMulS | Op::IMulS => {
                let r = both(self.sj(d), self.sk(d)).map(|(a, b)| match d.op {
                    Op::FAddS => fadd(a, b),
                    Op::FSubS => fsub(a, b),
                    Op::FMulS => fmul(a, b, MulKind::Full, FP_PROFILE),
                    Op::HMulS => fmul(a, b, MulKind::HalfRounded, FP_PROFILE),
                    Op::RMulS => fmul(a, b, MulKind::Rounded, FP_PROFILE),
                    _ => fmul(a, b, MulKind::TwoMinus, FP_PROFILE),
                });
                let value = self.fp_result(r)?;
                self.set_s(i, value);
            }
            Op::RecipS => {
                let r = self.sj(d).map(|a| frecip(a, FP_PROFILE));
                let value = self.fp_result(r)?;
                self.set_s(i, value);
            }

            // ---- 071 to 077: S transmits (pages 4-43 to 4-46)
            Op::SFromA => self.set_s(i, self.ak(d).map(|a| a as u64)),
            Op::SFromASigned => self.set_s(i, self.ak(d).map(sign_extend_24)),
            Op::SFromAFloat => {
                // exponent 40060, the magnitude of (Ak) as coefficient, the
                // sign of (Ak) as sign (page 4-43)
                let value = self.ak(d).map(|a| {
                    let negative = a & 0x80_0000 != 0;
                    let magnitude = if negative {
                        a.wrapping_neg() & A_MASK
                    } else {
                        a
                    };
                    (negative as u64) << 63 | 0o40060 << 48 | magnitude as u64
                });
                self.set_s(i, value);
            }
            Op::SConstPoint6 => self.set_s(i, Some(CONST_0_75_2_48)),
            Op::SConstPoint4 => self.set_s(i, Some(CONST_0_5)),
            Op::SConst1 => self.set_s(i, Some(CONST_1_0)),
            Op::SConst2 => self.set_s(i, Some(CONST_2_0)),
            Op::SConst4 => self.set_s(i, Some(CONST_4_0)),
            // the clock counts clock periods: not predictable at this level
            Op::SFromRt => self.set_s(i, self.rtc()),
            Op::SFromVm => self.set_s(i, self.vm),
            Op::SFromT => self.set_s(i, self.t[jk]),
            Op::TFromS => self.set_t(jk, self.s[i]),
            Op::SFromV => {
                // the low six bits of (Ak) select the element (page 4-46)
                let elem = self.need(self.ak(d), "Ak (vector element number)")? as usize & 0o77;
                self.set_s(i, self.v[j][elem]);
            }
            Op::VFromS => {
                let elem = self.need(self.ak(d), "Ak (vector element number)")? as usize & 0o77;
                let value = self.sj(d);
                self.set_v(i, elem, value);
            }

            // ---- 10h to 13h: scalar memory references (page 4-47)
            Op::LoadA => {
                let addr = self.scalar_address(d)?;
                let value = self.read_data(addr).map(|w| w as u32 & A_MASK);
                self.set_a(i, value);
            }
            Op::StoreA => {
                let addr = self.scalar_address(d)?;
                self.write_data(addr, self.a[i].map(|a| a as u64))?;
            }
            Op::LoadS => {
                let addr = self.scalar_address(d)?;
                let value = self.read_data(addr);
                self.set_s(i, value);
            }
            Op::StoreS => {
                let addr = self.scalar_address(d)?;
                self.write_data(addr, self.s[i])?;
            }

            // ---- 140 to 177: vector instructions
            _ => self.execute_vector(d)?,
        }
        Ok(())
    }
}
