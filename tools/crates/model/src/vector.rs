//! The vector instructions 140 to 177.
//!
//! # A result register that is also an operand (HRM pages 3-14 to 3-16)
//!
//! The manual: when i is the same as j or k the element counter of the
//! operand/result register is held at zero until the first result arrives,
//! functional unit time + 2 clock periods after the start, and then advances
//! by one each clock period.  So with n = functional unit time + 2, the
//! operation on element e takes, from the operand/result register,
//!
//! * the contents element 0 had before the instruction, for e < n;
//! * the result this instruction delivered to element e - n, for e >= n;
//!
//! and element e of any other operand register.  The functional unit times
//! are those of section 3: `unit_time`.

use crate::exec::{and_values, double_shift_left, double_shift_right, FP_PROFILE};
use crate::machine::{vl_count, Machine, TestError, A_MASK};
use cray1_fp::{fadd, fmul, frecip, fsub, FpResult, MulKind};
use cray1_isa::{Cpu, Decoded, Op};

/// Functional unit times in clock periods (HRM section 3).
pub mod unit_time {
    /// Vector logical unit, 140 to 147 (page 3-17).
    pub const VECTOR_LOGICAL: usize = 2;
    /// Vector shift unit, 150 to 153 (page 3-17).
    pub const VECTOR_SHIFT: usize = 4;
    /// Vector add unit, 154 to 157 (page 3-17).
    pub const VECTOR_ADD: usize = 3;
    /// Floating point add unit, 170 to 173 (page 3-18).
    pub const FP_ADD: usize = 6;
    /// Floating point multiply unit, 160 to 167 (page 3-18).
    pub const FP_MULTIPLY: usize = 7;
    /// Reciprocal approximation unit, 174 (page 3-18).
    pub const FP_RECIPROCAL: usize = 14;
    /// Vector population count unit, 174ij1 and 174ij2.  Rev F page 4-70
    /// gives a chain slot time of 8 clock periods, which is unit time + 2.
    pub const VECTOR_POPULATION: usize = 6;
}

fn plain(value: u64) -> FpResult {
    FpResult {
        value,
        range_error: false,
    }
}

impl Machine {
    /// The number of operations: VL must be defined.
    fn vector_count(&self) -> Result<usize, TestError> {
        Ok(vl_count(self.need(self.vl, "VL (vector length)")?))
    }

    /// The operand that register `r` supplies to the operation on element
    /// `e` of an instruction whose result register is `i`.  `delay` is the
    /// functional unit time + 2 and `old0` what element 0 of the result
    /// register held before the instruction.
    #[inline]
    fn operand(
        &self,
        r: usize,
        i: usize,
        e: usize,
        delay: usize,
        old0: Option<u64>,
    ) -> Option<u64> {
        if r != i {
            self.v[r][e]
        } else if self.cpu == Cpu::Xmp {
            // no recursion on the X-MP: every operand element is read before
            // its result arrives, so the operation sees the old contents
            self.v_before[e]
        } else if e < delay {
            old0
        } else {
            self.v[i][e - delay]
        }
    }

    /// Vi <- (Sj or Vj) op Vk, element by element.
    fn vector_binary(
        &mut self,
        d: &Decoded,
        scalar: bool,
        time: usize,
        float: bool,
        f: impl Fn(u64, u64) -> FpResult,
    ) -> Result<(), TestError> {
        let (i, j, k) = (d.i as usize, d.j as usize, d.k as usize);
        let count = self.vector_count()?;
        let (delay, old0, sj) = (time + 2, self.v[i][0], self.sj(d));
        for e in 0..count {
            let a = if scalar {
                sj
            } else {
                self.operand(j, i, e, delay, old0)
            };
            let b = self.operand(k, i, e, delay, old0);
            let r = match (a, b) {
                (Some(a), Some(b)) => Some(f(a, b)),
                _ => None,
            };
            let value = if float {
                self.fp_result(r)?
            } else {
                r.map(|r| r.value)
            };
            self.set_v(i, e, value);
        }
        Ok(())
    }

    /// 140 and 141: logical products.  As for the scalar product a defined
    /// zero operand gives zero, so `Vi 0` (140i00) clears Vi whatever V0
    /// holds.
    fn vector_and(&mut self, d: &Decoded, scalar: bool) -> Result<(), TestError> {
        let (i, j, k) = (d.i as usize, d.j as usize, d.k as usize);
        let count = self.vector_count()?;
        let (delay, old0, sj) = (unit_time::VECTOR_LOGICAL + 2, self.v[i][0], self.sj(d));
        for e in 0..count {
            let a = if scalar {
                sj
            } else {
                self.operand(j, i, e, delay, old0)
            };
            let b = self.operand(k, i, e, delay, old0);
            self.set_v(i, e, and_values(a, b));
        }
        Ok(())
    }

    /// 146 and 147: Vi <- (Sj or Vj) where the VM bit is 1, Vk where it is 0
    /// (page 4-50).  Bit 0 of VM, the leftmost, belongs to element 0.
    fn vector_merge(&mut self, d: &Decoded, scalar: bool) -> Result<(), TestError> {
        let (i, j, k) = (d.i as usize, d.j as usize, d.k as usize);
        let count = self.vector_count()?;
        let (delay, old0, sj) = (unit_time::VECTOR_LOGICAL + 2, self.v[i][0], self.sj(d));
        let vm = self.vm;
        for e in 0..count {
            let a = if scalar {
                sj
            } else {
                self.operand(j, i, e, delay, old0)
            };
            let b = self.operand(k, i, e, delay, old0);
            let value = match vm {
                Some(vm) if vm >> (63 - e) & 1 != 0 => a,
                Some(_) => b,
                None => None,
            };
            self.set_v(i, e, value);
        }
        Ok(())
    }

    /// 150 to 153 (pages 4-53 to 4-58).  The double shifts join each element
    /// with its neighbour in the stream of operands: the next one for a left
    /// shift (zeros after the last), the one before for a right shift (zeros
    /// before the first).
    fn vector_shift(&mut self, d: &Decoded) -> Result<(), TestError> {
        let (i, j) = (d.i as usize, d.j as usize);
        let count = self.vector_count()?;
        let shift = self.need(self.ak(d), "Ak (shift count)")?;
        let (delay, old0) = (unit_time::VECTOR_SHIFT + 2, self.v[i][0]);
        for e in 0..count {
            let x = self.operand(j, i, e, delay, old0);
            let value = match d.op {
                Op::ShlV => x.map(|x| if shift > 63 { 0 } else { x << shift }),
                Op::ShrV => x.map(|x| if shift > 63 { 0 } else { x >> shift }),
                Op::ShlDV => {
                    let next = if e + 1 < count {
                        self.operand(j, i, e + 1, delay, old0)
                    } else {
                        Some(0)
                    };
                    match (x, next) {
                        (Some(x), Some(next)) => Some(double_shift_left(x, next, shift)),
                        _ => None,
                    }
                }
                _ => {
                    let before = if e > 0 {
                        self.operand(j, i, e - 1, delay, old0)
                    } else {
                        Some(0)
                    };
                    match (before, x) {
                        (Some(before), Some(x)) => Some(double_shift_right(before, x, shift)),
                        _ => None,
                    }
                }
            };
            self.set_v(i, e, value);
        }
        Ok(())
    }

    /// 175: VM bit e is the result of testing element e of Vj for e below
    /// the vector length; the other bits are zero (page 4-68).
    fn vector_mask(&mut self, d: &Decoded) -> Result<(), TestError> {
        let count = self.vector_count()?;
        let mut mask = Some(0u64);
        for e in 0..count {
            let hit = self.v[d.j as usize][e].map(|x| match d.op {
                Op::VmZero => x == 0,
                Op::VmNonzero => x != 0,
                Op::VmPositive => x >> 63 == 0,
                _ => x >> 63 != 0,
            });
            mask = match (mask, hit) {
                (Some(m), Some(hit)) => Some(m | (hit as u64) << (63 - e)),
                _ => None,
            };
        }
        self.set_vm(mask);
        Ok(())
    }

    /// The addresses of 176 and 177: (A0), then (Ak) added for each
    /// successive word; (Ak) is a signed increment (page 4-70).
    fn vector_addresses(&self, d: &Decoded) -> Result<(usize, u32, u32), TestError> {
        let count = self.vector_count()?;
        let a0 = self.need(self.a[0], "A0 (vector memory address)")?;
        let step = self.need(self.ak(d), "Ak (vector address increment)")?;
        Ok((count, a0, step))
    }

    fn vector_load(&mut self, d: &Decoded) -> Result<(), TestError> {
        let (count, a0, step) = self.vector_addresses(d)?;
        let mut lost = false;
        for e in 0..count {
            let rel = a0.wrapping_add(step.wrapping_mul(e as u32)) & A_MASK;
            let value = self.transfer_read(rel, &mut lost);
            self.set_v(d.i as usize, e, value);
        }
        Ok(())
    }

    fn vector_store(&mut self, d: &Decoded) -> Result<(), TestError> {
        let (count, a0, step) = self.vector_addresses(d)?;
        let mut lost = false;
        for e in 0..count {
            let rel = a0.wrapping_add(step.wrapping_mul(e as u32)) & A_MASK;
            let value = self.v[d.j as usize][e];
            self.transfer_write(rel, value, &mut lost)?;
        }
        Ok(())
    }

    pub(crate) fn execute_vector(&mut self, d: &Decoded) -> Result<(), TestError> {
        use unit_time::*;
        if self.cpu == Cpu::Xmp {
            self.v_before = self.v[d.i as usize];
        }
        let mul = |kind: MulKind| move |a: u64, b: u64| fmul(a, b, kind, FP_PROFILE);
        match d.op {
            // ---- 145 and 157 with j = k: an element less itself, or
            // differing from itself, is zero whatever it holds.  `Vi Vi\Vi`
            // is how later CAL clears a vector register, also one never
            // written.  When i is that register too, both operands still
            // come from the same element.
            Op::XorVV | Op::SubVV if d.j == d.k => {
                let count = self.vector_count()?;
                for e in 0..count {
                    self.set_v(d.i as usize, e, Some(0));
                }
                Ok(())
            }
            // ---- 140 to 147: vector logical (pages 4-49 to 4-52)
            Op::AndSV => self.vector_and(d, true),
            Op::AndVV => self.vector_and(d, false),
            Op::OrSV => self.vector_binary(d, true, VECTOR_LOGICAL, false, |a, b| plain(a | b)),
            Op::OrVV => self.vector_binary(d, false, VECTOR_LOGICAL, false, |a, b| plain(a | b)),
            Op::XorSV => self.vector_binary(d, true, VECTOR_LOGICAL, false, |a, b| plain(a ^ b)),
            Op::XorVV => self.vector_binary(d, false, VECTOR_LOGICAL, false, |a, b| plain(a ^ b)),
            Op::MergeSV => self.vector_merge(d, true),
            Op::MergeVV => self.vector_merge(d, false),
            // ---- 150 to 153: vector shifts
            Op::ShlV | Op::ShrV | Op::ShlDV | Op::ShrDV => self.vector_shift(d),
            // ---- 154 to 157: vector integer add (pages 4-59, 4-60).  The text of
            // page 4-59 calls 155 a subtraction; its heading, its special cases
            // and page 3-17 make 154 and 155 sums, 156 and 157 differences.
            Op::AddSV => {
                self.vector_binary(d, true, VECTOR_ADD, false, |a, b| plain(a.wrapping_add(b)))
            }
            Op::AddVV => {
                self.vector_binary(d, false, VECTOR_ADD, false, |a, b| plain(a.wrapping_add(b)))
            }
            Op::SubSV => {
                self.vector_binary(d, true, VECTOR_ADD, false, |a, b| plain(a.wrapping_sub(b)))
            }
            Op::SubVV => {
                self.vector_binary(d, false, VECTOR_ADD, false, |a, b| plain(a.wrapping_sub(b)))
            }
            // ---- 160 to 167: vector floating multiply (pages 4-61 to 4-63)
            Op::FMulSV => self.vector_binary(d, true, FP_MULTIPLY, true, mul(MulKind::Full)),
            Op::FMulVV => self.vector_binary(d, false, FP_MULTIPLY, true, mul(MulKind::Full)),
            Op::HMulSV => self.vector_binary(d, true, FP_MULTIPLY, true, mul(MulKind::HalfRounded)),
            Op::HMulVV => {
                self.vector_binary(d, false, FP_MULTIPLY, true, mul(MulKind::HalfRounded))
            }
            Op::RMulSV => self.vector_binary(d, true, FP_MULTIPLY, true, mul(MulKind::Rounded)),
            Op::RMulVV => self.vector_binary(d, false, FP_MULTIPLY, true, mul(MulKind::Rounded)),
            Op::IMulSV => self.vector_binary(d, true, FP_MULTIPLY, true, mul(MulKind::TwoMinus)),
            Op::IMulVV => self.vector_binary(d, false, FP_MULTIPLY, true, mul(MulKind::TwoMinus)),
            // ---- 170 to 173: vector floating add (pages 4-64, 4-65)
            Op::FAddSV => self.vector_binary(d, true, FP_ADD, true, fadd),
            Op::FAddVV => self.vector_binary(d, false, FP_ADD, true, fadd),
            Op::FSubSV => self.vector_binary(d, true, FP_ADD, true, fsub),
            Op::FSubVV => self.vector_binary(d, false, FP_ADD, true, fsub),
            // ---- 174: reciprocal approximation (page 4-66)
            Op::RecipV => {
                let (i, j) = (d.i as usize, d.j as usize);
                let count = self.vector_count()?;
                let (delay, old0) = (FP_RECIPROCAL + 2, self.v[i][0]);
                for e in 0..count {
                    let r = self
                        .operand(j, i, e, delay, old0)
                        .map(|x| frecip(x, FP_PROFILE));
                    let value = self.fp_result(r)?;
                    self.set_v(i, e, value);
                }
                Ok(())
            }
            // ---- 174ij1, 174ij2: population count and its parity (rev F
            // page 4-70).  The other bits of each element are zero.
            Op::PopV | Op::ParityV => {
                let (i, j) = (d.i as usize, d.j as usize);
                let count = self.vector_count()?;
                let (delay, old0) = (VECTOR_POPULATION + 2, self.v[i][0]);
                let low_bit = d.op == Op::ParityV;
                for e in 0..count {
                    let value = self.operand(j, i, e, delay, old0).map(|x| {
                        let n = x.count_ones() as u64;
                        if low_bit {
                            n & 1
                        } else {
                            n
                        }
                    });
                    self.set_v(i, e, value);
                }
                Ok(())
            }
            // ---- 175 to 177
            Op::VmZero | Op::VmNonzero | Op::VmPositive | Op::VmNegative => self.vector_mask(d),
            Op::VLoad => self.vector_load(d),
            Op::VStore => self.vector_store(d),
            _ => unreachable!("{:?} is not a vector instruction", d.op),
        }
    }
}
