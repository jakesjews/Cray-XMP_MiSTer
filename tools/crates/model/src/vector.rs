//! The vector instructions 140 to 177.
//!
//! # A result register that is also an operand
//!
//! Every operand element is read before its result arrives, so when i is
//! the same as j or k the operation sees what the register held before the
//! instruction (CSM-0111000 page 3-33; the CRAY-1's recursive use of such a
//! register is gone).  `v_before` keeps those contents.

use crate::exec::{and_values, double_shift_left, double_shift_right};
use crate::machine::{vl_count, Machine, TestError, A_MASK};
use cray_xmp_fp::{fadd, fmul, frecip, fsub, FpResult, MulKind};
use cray_xmp_isa::{Decoded, Op};

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
    /// `e` of an instruction whose result register is `i`: for the result
    /// register itself, what it held before the instruction.
    #[inline]
    fn operand(&self, r: usize, i: usize, e: usize) -> Option<u64> {
        if r != i {
            self.v[r][e]
        } else {
            self.v_before[e]
        }
    }

    /// Vi <- (Sj or Vj) op Vk, element by element.
    fn vector_binary(
        &mut self,
        d: &Decoded,
        scalar: bool,
        float: bool,
        f: impl Fn(u64, u64) -> FpResult,
    ) -> Result<(), TestError> {
        let (i, j, k) = (d.i as usize, d.j as usize, d.k as usize);
        let count = self.vector_count()?;
        let sj = self.sj(d);
        for e in 0..count {
            let a = if scalar { sj } else { self.operand(j, i, e) };
            let b = self.operand(k, i, e);
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
        let sj = self.sj(d);
        for e in 0..count {
            let a = if scalar { sj } else { self.operand(j, i, e) };
            let b = self.operand(k, i, e);
            self.set_v(i, e, and_values(a, b));
        }
        Ok(())
    }

    /// 146 and 147: Vi <- (Sj or Vj) where the VM bit is 1, Vk where it is 0
    /// (page 4-50).  Bit 0 of VM, the leftmost, belongs to element 0.
    fn vector_merge(&mut self, d: &Decoded, scalar: bool) -> Result<(), TestError> {
        let (i, j, k) = (d.i as usize, d.j as usize, d.k as usize);
        let count = self.vector_count()?;
        let sj = self.sj(d);
        let vm = self.vm;
        for e in 0..count {
            let a = if scalar { sj } else { self.operand(j, i, e) };
            let b = self.operand(k, i, e);
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
        for e in 0..count {
            let x = self.operand(j, i, e);
            let value = match d.op {
                Op::ShlV => x.map(|x| if shift > 63 { 0 } else { x << shift }),
                Op::ShrV => x.map(|x| if shift > 63 { 0 } else { x >> shift }),
                Op::ShlDV => {
                    let next = if e + 1 < count {
                        self.operand(j, i, e + 1)
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
                        self.operand(j, i, e - 1)
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
    /// the vector length; the other bits are zero (page 4-68).  With the
    /// high bit of k (X 5-87) the numbers of the elements that pass also go
    /// to Vi, one behind the other from element 0; the elements of Vi
    /// behind them stay.  Which elements those are cannot be known once
    /// an element tested is undefined.
    fn vector_mask(&mut self, d: &Decoded) -> Result<(), TestError> {
        let count = self.vector_count()?;
        let compress = matches!(
            d.op,
            Op::VmZeroIdx | Op::VmNonzeroIdx | Op::VmPositiveIdx | Op::VmNegativeIdx
        );
        let mut mask = Some(0u64);
        let mut next = 0;
        for e in 0..count {
            let hit = self.v[d.j as usize][e].map(|x| match d.op {
                Op::VmZero | Op::VmZeroIdx => x == 0,
                Op::VmNonzero | Op::VmNonzeroIdx => x != 0,
                Op::VmPositive | Op::VmPositiveIdx => x >> 63 == 0,
                _ => x >> 63 != 0,
            });
            if compress {
                match hit {
                    Some(true) => {
                        self.set_v(d.i as usize, next, Some(e as u64));
                        next += 1;
                    }
                    Some(false) => {}
                    None => {
                        return Err(self.error(
                            crate::ErrorKind::UndefinedValue,
                            format!("element {} of V{} (tested by a compress index)", e, d.j),
                        ))
                    }
                }
            }
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
        for e in 0..count {
            let rel = a0.wrapping_add(step.wrapping_mul(e as u32)) & A_MASK;
            let value = self.read_data(rel);
            self.set_v(d.i as usize, e, value);
        }
        Ok(())
    }

    fn vector_store(&mut self, d: &Decoded) -> Result<(), TestError> {
        let (count, a0, step) = self.vector_addresses(d)?;
        for e in 0..count {
            let rel = a0.wrapping_add(step.wrapping_mul(e as u32)) & A_MASK;
            let value = self.v[d.j as usize][e];
            self.transfer_write(rel, value)?;
        }
        Ok(())
    }

    /// The address of element e of a gather or scatter: (A0) plus the low
    /// 24 bits of element e of Vk (X 5-91).
    fn indexed_address(&self, d: &Decoded, a0: u32, e: usize) -> Result<u32, TestError> {
        let index = self.need(
            self.v[d.k as usize][e],
            "an element of Vk (the index of a gather or scatter)",
        )?;
        Ok(a0.wrapping_add(index as u32) & A_MASK)
    }

    /// 176i1k.  Vi may be Vk: an element is read as an index before the
    /// word it names is put in its place.
    fn vector_gather(&mut self, d: &Decoded) -> Result<(), TestError> {
        let count = self.vector_count()?;
        let a0 = self.need(self.a[0], "A0 (vector memory address)")?;
        for e in 0..count {
            let rel = self.indexed_address(d, a0, e)?;
            let value = self.read_data(rel);
            self.set_v(d.i as usize, e, value);
        }
        Ok(())
    }

    /// 1771jk.  The words are stored in the order of the elements, so of
    /// two with the same index the later one stays.
    fn vector_scatter(&mut self, d: &Decoded) -> Result<(), TestError> {
        let count = self.vector_count()?;
        let a0 = self.need(self.a[0], "A0 (vector memory address)")?;
        for e in 0..count {
            let rel = self.indexed_address(d, a0, e)?;
            let value = self.v[d.j as usize][e];
            self.transfer_write(rel, value)?;
        }
        Ok(())
    }

    pub(crate) fn execute_vector(&mut self, d: &Decoded) -> Result<(), TestError> {
        self.v_before = self.v[d.i as usize];
        let mul = |kind: MulKind| move |a: u64, b: u64| fmul(a, b, kind);
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
            Op::OrSV => self.vector_binary(d, true, false, |a, b| plain(a | b)),
            Op::OrVV => self.vector_binary(d, false, false, |a, b| plain(a | b)),
            Op::XorSV => self.vector_binary(d, true, false, |a, b| plain(a ^ b)),
            Op::XorVV => self.vector_binary(d, false, false, |a, b| plain(a ^ b)),
            Op::MergeSV => self.vector_merge(d, true),
            Op::MergeVV => self.vector_merge(d, false),
            // ---- 150 to 153: vector shifts
            Op::ShlV | Op::ShrV | Op::ShlDV | Op::ShrDV => self.vector_shift(d),
            // ---- 154 to 157: vector integer add (pages 4-59, 4-60).  The text of
            // page 4-59 calls 155 a subtraction; its heading, its special cases
            // and page 3-17 make 154 and 155 sums, 156 and 157 differences.
            Op::AddSV => self.vector_binary(d, true, false, |a, b| plain(a.wrapping_add(b))),
            Op::AddVV => self.vector_binary(d, false, false, |a, b| plain(a.wrapping_add(b))),
            Op::SubSV => self.vector_binary(d, true, false, |a, b| plain(a.wrapping_sub(b))),
            Op::SubVV => self.vector_binary(d, false, false, |a, b| plain(a.wrapping_sub(b))),
            // ---- 160 to 167: vector floating multiply (pages 4-61 to 4-63)
            Op::FMulSV => self.vector_binary(d, true, true, mul(MulKind::Full)),
            Op::FMulVV => self.vector_binary(d, false, true, mul(MulKind::Full)),
            Op::HMulSV => self.vector_binary(d, true, true, mul(MulKind::HalfRounded)),
            Op::HMulVV => self.vector_binary(d, false, true, mul(MulKind::HalfRounded)),
            Op::RMulSV => self.vector_binary(d, true, true, mul(MulKind::Rounded)),
            Op::RMulVV => self.vector_binary(d, false, true, mul(MulKind::Rounded)),
            Op::IMulSV => self.vector_binary(d, true, true, mul(MulKind::TwoMinus)),
            Op::IMulVV => self.vector_binary(d, false, true, mul(MulKind::TwoMinus)),
            // ---- 170 to 173: vector floating add (pages 4-64, 4-65)
            Op::FAddSV => self.vector_binary(d, true, true, fadd),
            Op::FAddVV => self.vector_binary(d, false, true, fadd),
            Op::FSubSV => self.vector_binary(d, true, true, fsub),
            Op::FSubVV => self.vector_binary(d, false, true, fsub),
            // ---- 174: reciprocal approximation (page 4-66)
            Op::RecipV => {
                let (i, j) = (d.i as usize, d.j as usize);
                let count = self.vector_count()?;
                for e in 0..count {
                    let r = self.operand(j, i, e).map(frecip);
                    let value = self.fp_result(r)?;
                    self.set_v(i, e, value);
                }
                Ok(())
            }
            // ---- 174ij1, 174ij2: population count and its parity.  The
            // other bits of each element are zero.
            Op::PopV | Op::ParityV => {
                let (i, j) = (d.i as usize, d.j as usize);
                let count = self.vector_count()?;
                let low_bit = d.op == Op::ParityV;
                for e in 0..count {
                    let value = self.operand(j, i, e).map(|x| {
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
            Op::VmZero
            | Op::VmNonzero
            | Op::VmPositive
            | Op::VmNegative
            | Op::VmZeroIdx
            | Op::VmNonzeroIdx
            | Op::VmPositiveIdx
            | Op::VmNegativeIdx => self.vector_mask(d),
            Op::VLoad => self.vector_load(d),
            Op::VStore => self.vector_store(d),
            Op::VGather => self.vector_gather(d),
            Op::VScatter => self.vector_scatter(d),
            _ => unreachable!("{:?} is not a vector instruction", d.op),
        }
    }
}
