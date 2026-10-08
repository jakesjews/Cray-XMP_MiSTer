//! Encoding and decoding of instruction parcels against the table.

use crate::table::*;
use std::sync::OnceLock;

/// The operand fields of an instruction.  `m` is the second parcel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fields {
    pub h: u8,
    pub i: u8,
    pub j: u8,
    pub k: u8,
    pub m: u16,
}

impl Fields {
    /// The 6-bit jk field (B and T register numbers, shift and mask counts).
    pub fn jk(&self) -> u8 {
        ((self.j & 7) << 3) | (self.k & 7)
    }
    /// The 9-bit ijk field.
    pub fn ijk(&self) -> u16 {
        ((self.i as u16 & 7) << 6) | self.jk() as u16
    }
    /// The 22-bit jkm field.
    pub fn jkm(&self) -> u32 {
        ((self.jk() as u32) << 16) | self.m as u32
    }
    /// The 25-bit ijkm field.
    pub fn ijkm(&self) -> u32 {
        ((self.i as u32 & 7) << 22) | self.jkm()
    }
    pub fn set_jk(&mut self, jk: u8) {
        self.j = (jk >> 3) & 7;
        self.k = jk & 7;
    }
    pub fn set_jkm(&mut self, jkm: u32) {
        self.set_jk((jkm >> 16) as u8 & 0o77);
        self.m = jkm as u16;
    }
    /// Store an expression value in the fields the way `kind` says.
    pub fn set_exp(&mut self, kind: ExpKind, value: i64) -> Result<(), String> {
        let check = |lo: i64, hi: i64, what: &str| -> Result<(), String> {
            if value < lo || value > hi {
                Err(format!(
                    "{} {} is out of range ({} to {})",
                    what,
                    signed_octal(value),
                    signed_octal(lo),
                    signed_octal(hi)
                ))
            } else {
                Ok(())
            }
        };
        match kind {
            ExpKind::None => return Err("the form takes no expression".to_string()),
            ExpKind::Ijk => {
                check(0, 0o777, "value")?;
                self.i = (value >> 6) as u8 & 7;
                self.set_jk(value as u8 & 0o77);
            }
            ExpKind::J => {
                check(0, 7, "value")?;
                self.j = value as u8;
            }
            ExpKind::Jk => {
                check(0, 63, "value")?;
                self.set_jk(value as u8);
            }
            ExpKind::JkRev => {
                check(1, 64, "count")?;
                self.set_jk((64 - value) as u8);
            }
            ExpKind::Jkm => {
                check(0, (1 << 22) - 1, "value")?;
                self.set_jkm(value as u32);
            }
            ExpKind::JkmNot => {
                check(-(1 << 22), -1, "value")?;
                self.set_jkm(!(value as u32) & 0x3f_ffff);
            }
            ExpKind::JkmSigned => {
                check(-(1 << 21), (1 << 22) - 1, "address")?;
                self.set_jkm(value as u32 & 0x3f_ffff);
            }
            ExpKind::Ijkm => {
                check(0, (1 << 24) - 1, "parcel address")?;
                self.i = (value >> 22) as u8 & 3;
                self.set_jkm(value as u32 & 0x3f_ffff);
            }
            ExpKind::Ijkm24 => {
                check(0, (1 << 24) - 1, "value")?;
                self.i = 4 | ((value >> 22) as u8 & 3);
                self.set_jkm(value as u32 & 0x3f_ffff);
            }
        }
        Ok(())
    }
    /// The expression value the fields hold under `kind`.
    pub fn exp(&self, kind: ExpKind) -> Option<i64> {
        Some(match kind {
            ExpKind::None => return None,
            ExpKind::Ijk => self.ijk() as i64,
            ExpKind::J => (self.j & 7) as i64,
            ExpKind::Jk => self.jk() as i64,
            ExpKind::JkRev => 64 - self.jk() as i64,
            ExpKind::Jkm => self.jkm() as i64,
            ExpKind::JkmNot => -(self.jkm() as i64) - 1,
            ExpKind::JkmSigned => {
                let v = self.jkm() as i64;
                if v >= 1 << 21 {
                    v - (1 << 22)
                } else {
                    v
                }
            }
            ExpKind::Ijkm | ExpKind::Ijkm24 => (self.ijkm() & 0xff_ffff) as i64,
        })
    }
}

/// Format a value the way the disassembler prints expressions: octal digits
/// with a leading `-` for negative values.
pub fn signed_octal(value: i64) -> String {
    if value < 0 {
        format!("-{:o}", value.unsigned_abs())
    } else {
        format!("{:o}", value)
    }
}

/// An encoded instruction: one or two parcels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Encoding {
    pub parcel0: u16,
    pub parcel1: Option<u16>,
}

impl Encoding {
    /// Length in parcels.
    pub fn len(&self) -> usize {
        1 + self.parcel1.is_some() as usize
    }
    pub fn is_empty(&self) -> bool {
        false
    }
    /// The parcels in memory order.
    pub fn parcels(&self) -> Vec<u16> {
        let mut v = vec![self.parcel0];
        v.extend(self.parcel1);
        v
    }
}

/// A register an instruction reads or writes, with its number resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Reg {
    A(u8),
    S(u8),
    V(u8),
    B(u8),
    T(u8),
    /// (Ai) B registers starting at this one, wrapping after B77.
    BBlock(u8),
    /// (Ai) T registers starting at this one, wrapping after T77.
    TBlock(u8),
    Vl,
    Vm,
    Rtc,
    Xa,
}

struct Index {
    /// Rows whose pattern can match each 7-bit gh opcode, in table order.
    by_gh: Vec<Vec<u16>>,
    /// The base row of each `Op`.
    base: Vec<u16>,
}

fn index() -> &'static Index {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut by_gh = vec![Vec::new(); 128];
        let mut base = vec![u16::MAX; OP_COUNT];
        for (n, f) in FORMS.iter().enumerate() {
            for (gh, list) in by_gh.iter_mut().enumerate() {
                let p0 = (gh as u16) << 9;
                if p0 & f.mask & 0xfe00 == f.bits & 0xfe00 {
                    list.push(n as u16);
                }
            }
            if f.kind == Kind::Base && base[f.op as usize] == u16::MAX {
                base[f.op as usize] = n as u16;
            }
        }
        Index { by_gh, base }
    })
}

/// The rows of `FORMS` whose pattern can match first parcels with the given
/// 7-bit opcode, in table order.
pub(crate) fn rows_for_gh(gh: u8) -> impl Iterator<Item = (usize, &'static Form)> {
    index().by_gh[gh as usize & 0x7f]
        .iter()
        .map(|&n| (n as usize, &FORMS[n as usize]))
}

/// The base (general) row of an instruction.
pub fn base_form(op: Op) -> &'static Form {
    &FORMS[index().base[op as usize] as usize]
}

impl Form {
    /// The base row of this row's instruction (itself for a base row).
    pub fn base(&self) -> &'static Form {
        base_form(self.op)
    }
    /// The functional unit the instruction occupies.
    pub fn unit(&self) -> Unit {
        self.base().unit
    }
    /// All flag bits (see the `flag` module): those of the instruction plus
    /// the spelling flags of this row.
    pub fn flags(&self) -> u16 {
        self.base().flags | self.flags
    }
    /// Registers the instruction reads, as named in the manual.  A0 and S0
    /// appear as `RegRef::A0`/`RegRef::S0` only where their real contents are
    /// used implicitly; see `Decoded::reads` for the special values of
    /// register 0 in the h, j and k fields.
    pub fn reads(&self) -> &'static [RegRef] {
        self.base().reads
    }
    /// Registers the instruction writes.
    pub fn writes(&self) -> &'static [RegRef] {
        self.base().writes
    }
    pub fn is_branch(&self) -> bool {
        self.flags() & flag::BRANCH != 0
    }
    pub fn is_conditional_branch(&self) -> bool {
        self.flags() & flag::COND != 0
    }
    pub fn reads_memory(&self) -> bool {
        self.flags() & flag::MEM_READ != 0
    }
    pub fn writes_memory(&self) -> bool {
        self.flags() & flag::MEM_WRITE != 0
    }
    pub fn is_memory_reference(&self) -> bool {
        self.flags() & (flag::MEM_READ | flag::MEM_WRITE) != 0
    }
    /// Acts only in monitor mode and is a no-op otherwise.
    pub fn is_monitor_only(&self) -> bool {
        self.flags() & flag::MONITOR != 0
    }
    /// Error exit or normal exit.
    pub fn is_exit(&self) -> bool {
        self.flags() & flag::EXIT != 0
    }
    /// Operates on (VL) elements of vector registers.
    pub fn is_vector(&self) -> bool {
        self.flags() & flag::VECTOR != 0
    }
    /// True if CAL has a spelling for this row.
    pub fn has_syntax(&self) -> bool {
        !self.result.is_empty()
    }
    /// True if the h field is an operand of this row (10h to 13h).
    pub fn uses_h(&self) -> bool {
        self.var & VAR_H != 0
    }
    /// True if the i field is an operand of this row.
    pub fn uses_i(&self) -> bool {
        self.var & VAR_I != 0
    }
    /// True if the j field is an operand of this row.
    pub fn uses_j(&self) -> bool {
        self.var & VAR_J != 0
    }
    /// True if the k field is an operand of this row.
    pub fn uses_k(&self) -> bool {
        self.var & VAR_K != 0
    }
    /// True if the second parcel is an operand of this row.
    pub fn uses_m(&self) -> bool {
        self.var & VAR_M != 0
    }
    /// True if the first parcel (and, for a row that fixes it, the second)
    /// fits this row's pattern.
    pub fn matches(&self, parcel0: u16, parcel1: Option<u16>) -> bool {
        parcel0 & self.mask == self.bits
            && (self.flags & flag::J_NONZERO == 0 || (parcel0 >> 3) & 7 != 0)
            && (self.var & M_ZERO == 0 || parcel1.unwrap_or(0) == 0)
    }
    /// The inclusive range of values `exp` may take in this row, if it has
    /// an expression operand.
    pub fn exp_range(&self) -> Option<(i64, i64)> {
        Some(match self.exp {
            ExpKind::None => return None,
            ExpKind::Ijk => (0, 0o777),
            ExpKind::J => (0, 7),
            ExpKind::Jk => (0, 63),
            ExpKind::JkRev => (1, 64),
            ExpKind::Jkm => (0, (1 << 22) - 1),
            ExpKind::JkmNot => (-(1 << 22), -1),
            ExpKind::JkmSigned => (-(1 << 21), (1 << 22) - 1),
            ExpKind::Ijkm | ExpKind::Ijkm24 => (0, (1 << 24) - 1),
        })
    }
}

/// Encode an instruction from a table row and operand fields.
///
/// Only the fields the row's pattern names are used; fixed digits come from
/// the row and ignored fields are encoded as zero, so the result is the
/// canonical encoding.  Field values are masked to their width.
pub fn encode(form: &Form, fields: Fields) -> Encoding {
    let mut p0 = form.bits;
    if form.var & VAR_H != 0 {
        p0 |= (fields.h as u16 & 7) << 9;
    }
    if form.var & VAR_I != 0 {
        p0 |= (fields.i as u16 & 7) << 6;
    }
    if form.var & VAR_J != 0 {
        p0 |= (fields.j as u16 & 7) << 3;
    }
    if form.var & VAR_K != 0 {
        p0 |= fields.k as u16 & 7;
    }
    p0 &= !form.dont_care;
    let parcel1 = match form.parcels {
        2 if form.var & VAR_M != 0 => Some(fields.m),
        2 => Some(0),
        _ => None,
    };
    Encoding {
        parcel0: p0,
        parcel1,
    }
}

/// Length in parcels (1 or 2) of the instruction that starts with `parcel0`.
pub fn length(parcel0: u16) -> usize {
    decode_form(parcel0).parcels as usize
}

fn decode_form(parcel0: u16) -> &'static Form {
    rows_for_gh((parcel0 >> 9) as u8)
        .map(|(_, f)| f)
        .find(|f| f.kind == Kind::Base && f.matches(parcel0, None))
        .expect("the table gives every parcel a base form")
}

/// A decoded instruction.
#[derive(Clone, Copy, Debug)]
pub struct Decoded {
    /// The base table row of the instruction.
    pub form: &'static Form,
    /// The instruction (same as `form.op`).
    pub op: Op,
    /// Length in parcels, 1 or 2.
    pub parcels: u8,
    /// The first parcel as given.
    pub parcel0: u16,
    /// The 4-bit g field.
    pub g: u8,
    /// The 3-bit h field.
    pub h: u8,
    /// The 3-bit i field.
    pub i: u8,
    /// The 3-bit j field.
    pub j: u8,
    /// The 3-bit k field.
    pub k: u8,
    /// The second parcel; 0 for a one-parcel instruction or when it was not
    /// supplied.
    pub m: u16,
    /// The assembled expression operand, if the instruction has one: the
    /// 24-bit parcel address of a branch (2**22 or more is a program range
    /// error), the 22-bit constant of 020/040 (negative for 021/041), the
    /// signed 22-bit displacement of 10h to 13h, the 6-bit constant of 022,
    /// or the shift count or mask length of 042, 043 and 052 to 055.
    pub exp: Option<i64>,
}

impl PartialEq for Decoded {
    fn eq(&self, other: &Decoded) -> bool {
        self.parcel0 == other.parcel0 && self.parcels == other.parcels && self.m == other.m
    }
}

impl Eq for Decoded {}

/// Decode an instruction from its first parcel and, when available, the
/// parcel that follows it.
///
/// Every first parcel decodes: fields the manual says are ignored are
/// ignored.  If `parcels` is 2 and `parcel1` was `None`, `m` is taken
/// as zero; use `length` first when the second parcel has to be fetched.
pub fn decode(parcel0: u16, parcel1: Option<u16>) -> Decoded {
    let form = decode_form(parcel0);
    let m = if form.parcels == 2 {
        parcel1.unwrap_or(0)
    } else {
        0
    };
    let mut d = Decoded {
        form,
        op: form.op,
        parcels: form.parcels,
        parcel0,
        g: (parcel0 >> 12) as u8,
        h: (parcel0 >> 9) as u8 & 7,
        i: (parcel0 >> 6) as u8 & 7,
        j: (parcel0 >> 3) as u8 & 7,
        k: parcel0 as u8 & 7,
        m,
        exp: None,
    };
    d.exp = d.fields().exp(form.exp);
    d
}

impl Decoded {
    /// Length in parcels.
    pub fn len(&self) -> usize {
        self.parcels as usize
    }
    pub fn is_empty(&self) -> bool {
        false
    }
    /// The 7-bit opcode gh.
    pub fn gh(&self) -> u8 {
        (self.parcel0 >> 9) as u8
    }
    /// The h, i, j, k and m fields as found in the parcels.
    pub fn fields(&self) -> Fields {
        Fields {
            h: self.h,
            i: self.i,
            j: self.j,
            k: self.k,
            m: self.m,
        }
    }
    /// The 6-bit jk field.
    pub fn jk(&self) -> u8 {
        self.fields().jk()
    }
    /// The 22-bit jkm field.
    pub fn jkm(&self) -> u32 {
        self.fields().jkm()
    }
    /// The 25-bit ijkm field.
    pub fn ijkm(&self) -> u32 {
        self.fields().ijkm()
    }
    /// The canonical encoding: ignored fields zero.
    pub fn canonical(&self) -> Encoding {
        encode(self.form, self.fields())
    }
    /// True if the parcels equal the canonical encoding.
    pub fn is_canonical(&self) -> bool {
        let c = self.canonical();
        c.parcel0 == self.parcel0 && c.parcel1.unwrap_or(0) == self.m
    }
    fn resolve(&self, list: &[RegRef]) -> Vec<Reg> {
        let mut out = Vec::new();
        for r in list {
            let reg = match *r {
                RegRef::Ai => Some(Reg::A(self.i)),
                RegRef::Aj if self.j != 0 => Some(Reg::A(self.j)),
                RegRef::Ak if self.k != 0 => Some(Reg::A(self.k)),
                RegRef::Ah if self.h != 0 => Some(Reg::A(self.h)),
                RegRef::AhDest => Some(Reg::A(self.h)),
                RegRef::A0 => Some(Reg::A(0)),
                RegRef::Si => Some(Reg::S(self.i)),
                RegRef::Sj if self.j != 0 => Some(Reg::S(self.j)),
                RegRef::Sk if self.k != 0 => Some(Reg::S(self.k)),
                RegRef::S0 => Some(Reg::S(0)),
                RegRef::Vi => Some(Reg::V(self.i)),
                RegRef::Vj => Some(Reg::V(self.j)),
                RegRef::Vk => Some(Reg::V(self.k)),
                RegRef::Bjk => Some(Reg::B(self.jk())),
                RegRef::Tjk => Some(Reg::T(self.jk())),
                RegRef::B00 => Some(Reg::B(0)),
                RegRef::BBlock => Some(Reg::BBlock(self.jk())),
                RegRef::TBlock => Some(Reg::TBlock(self.jk())),
                RegRef::Vl => Some(Reg::Vl),
                RegRef::Vm => Some(Reg::Vm),
                RegRef::Rtc => Some(Reg::Rtc),
                RegRef::Xa => Some(Reg::Xa),
                RegRef::Aj | RegRef::Ak | RegRef::Ah | RegRef::Sj | RegRef::Sk => None,
            };
            if let Some(reg) = reg {
                if !out.contains(&reg) {
                    out.push(reg);
                }
            }
        }
        out
    }
    /// The registers this instruction reads.  A register 0 named by the h, j
    /// or k field is left out because the machine substitutes a constant for
    /// it: (Ah) = 0, (Aj) = 0, (Ak) = 1, (Sj) = 0, (Sk) = 2**63.
    pub fn reads(&self) -> Vec<Reg> {
        self.resolve(self.form.reads)
    }
    /// The registers this instruction writes.
    pub fn writes(&self) -> Vec<Reg> {
        self.resolve(self.form.writes)
    }
}
