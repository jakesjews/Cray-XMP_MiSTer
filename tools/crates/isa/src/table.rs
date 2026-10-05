//! The instruction table: one row per line of Appendix D of the CRAY-1
//! Hardware Reference Manual (2240004 rev C), the rows of the two options of
//! the 1982 machine (HR-0004 rev F: programmable clock, vector population
//! instructions), plus the few rows needed to give every 16-bit parcel a
//! meaning.  Everything else in this crate is driven from `FORMS`.

/// The machine whose instruction set is meant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Cpu {
    /// The CRAY-1 of 1982 with its two instruction set options.
    #[default]
    Cray1,
    /// The CRAY-1 plus the instructions of a one-processor CRAY X-MP that the
    /// operating system COS needs: the rows with `flag::XMP`.
    Xmp,
}

impl Cpu {
    /// `CRAY1` or `XMP`, as the assembler's `MACHINE` directive spells it.
    pub fn name(self) -> &'static str {
        match self {
            Cpu::Cray1 => "CRAY1",
            Cpu::Xmp => "XMP",
        }
    }
    pub fn from_name(name: &str) -> Option<Cpu> {
        match name.to_ascii_uppercase().replace(['-', '_'], "").as_str() {
            "CRAY1" => Some(Cpu::Cray1),
            "XMP" | "XMP1" => Some(Cpu::Xmp),
            _ => None,
        }
    }
}

/// A base instruction of the Cray-1.  Every first parcel decodes to exactly
/// one `Op`; special syntax forms and alternate spellings share the `Op` of
/// the instruction they assemble to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Op {
    /// 000xxx error exit
    Err,
    /// 0010jk set channel (Aj) current address to (Ak), activate channel
    SetCa,
    /// 0011jk set channel (Aj) limit address to (Ak)
    SetCl,
    /// 0012jx clear channel (Aj) interrupt and error flags
    ClearCi,
    /// 0013jx XA <- (Aj)
    SetXa,
    /// 0014j0 RTC <- (Sj)
    SetRt,
    /// 0014j4 programmable clock: interrupt interval and countdown <- (Sj)
    SetPci,
    /// 0014x5 clear the programmable clock interrupt request
    Cci,
    /// 0014x6 enable the programmable clock interrupt request
    Eci,
    /// 0014x7 disable the programmable clock interrupt request
    Dci,
    /// 0014xk with k = 1, 2 or 3: not defined with the programmable clock; a pass
    ClockPass,
    /// 0015xx to 0017xx: pass
    MonitorPass,
    /// X-MP 0012j1: clear channel (Aj) flags and set its device master clear
    ChanMc,
    /// X-MP 0014j3: cluster number <- j
    SetCln,
    /// X-MP 0023xx: set the operand range interrupt mode flag
    Eri,
    /// X-MP 0024xx: clear the operand range interrupt mode flag
    Dri,
    /// X-MP 0025xx: clear the bidirectional memory mode flag
    Dbm,
    /// X-MP 0026xx: set the bidirectional memory mode flag
    Ebm,
    /// X-MP 0027xx: wait for memory references to complete
    Cmr,
    /// X-MP 0034jk: test and set semaphore jk
    SemTestSet,
    /// X-MP 0036jk: clear semaphore jk
    SemClear,
    /// X-MP 0037jk: set semaphore jk
    SemSet,
    /// X-MP 026ij7: Ai <- (SBj)
    AFromSb,
    /// X-MP 027ij7: SBj <- (Ai)
    SbFromA,
    /// X-MP 072i02: Si <- the semaphores
    SFromSm,
    /// X-MP 072ij3: Si <- (STj)
    SFromSt,
    /// X-MP 073i01: Si <- the status register
    SFromSr,
    /// X-MP 073i02: the semaphores <- (Si)
    SmFromS,
    /// X-MP 073ij3: STj <- (Si)
    StFromS,
    /// 0020xk VL <- (Ak)
    SetVl,
    /// 0021xx set the floating point interrupt mode flag
    Efi,
    /// 0022xx clear the floating point interrupt mode flag
    Dfi,
    /// 0023xx to 0027xx: not defined by the Cray-1 manual
    Undefined,
    /// 003xjx VM <- (Sj)
    SetVm,
    /// 004xxx normal exit
    Ex,
    /// 005xjk P <- (Bjk)
    JumpB,
    /// 006ijkm P <- ijkm
    Jump,
    /// 007ijkm B00 <- P + 2, P <- ijkm
    ReturnJump,
    /// 010ijkm branch if (A0) = 0
    Jaz,
    /// 011ijkm branch if (A0) != 0
    Jan,
    /// 012ijkm branch if (A0) positive (zero counts as positive)
    Jap,
    /// 013ijkm branch if (A0) negative
    Jam,
    /// 014ijkm branch if (S0) = 0
    Jsz,
    /// 015ijkm branch if (S0) != 0
    Jsn,
    /// 016ijkm branch if (S0) positive (zero counts as positive)
    Jsp,
    /// 017ijkm branch if (S0) negative
    Jsm,
    /// 020ijkm Ai <- jkm
    ImmA,
    /// 021ijkm Ai <- complement of jkm
    ImmANot,
    /// 022ijk Ai <- jk
    ImmAShort,
    /// 023ijx Ai <- (Sj)
    AFromS,
    /// 024ijk Ai <- (Bjk)
    AFromB,
    /// 025ijk Bjk <- (Ai)
    BFromA,
    /// 026ijx Ai <- population count of (Sj)
    PopCount,
    /// 026ij1 Ai <- population count parity of (Sj)
    PopParity,
    /// 027ijx Ai <- leading zero count of (Sj)
    LeadingZeros,
    /// 030ijk Ai <- (Aj) + (Ak)
    AddA,
    /// 031ijk Ai <- (Aj) - (Ak)
    SubA,
    /// 032ijk Ai <- (Aj) * (Ak)
    MulA,
    /// 033i0x Ai <- channel number of highest priority interrupt
    ChanInt,
    /// 033ijk, j != 0, k even: Ai <- current address of channel (Aj)
    ChanAddr,
    /// 033ijk, j != 0, k odd: Ai <- error flag of channel (Aj)
    ChanErr,
    /// 034ijk B registers from jk <- (Ai) words of memory from (A0)
    BLoad,
    /// 035ijk memory from (A0) <- (Ai) B registers from jk
    BStore,
    /// 036ijk T registers from jk <- (Ai) words of memory from (A0)
    TLoad,
    /// 037ijk memory from (A0) <- (Ai) T registers from jk
    TStore,
    /// 040ijkm Si <- jkm
    ImmS,
    /// 041ijkm Si <- complement of jkm
    ImmSNot,
    /// 042ijk Si <- mask of 64-jk ones from the right
    MaskRight,
    /// 043ijk Si <- mask of jk ones from the left
    MaskLeft,
    /// 044ijk Si <- (Sj) and (Sk)
    AndS,
    /// 045ijk Si <- (Sj) and not (Sk)
    AndNotS,
    /// 046ijk Si <- (Sj) xor (Sk)
    XorS,
    /// 047ijk Si <- not ((Sj) xor (Sk))
    EqvS,
    /// 050ijk Si <- ((Sj) and (Sk)) or ((Si) and not (Sk))
    MergeS,
    /// 051ijk Si <- (Sj) or (Sk)
    OrS,
    /// 052ijk S0 <- (Si) shifted left jk
    ShlS0,
    /// 053ijk S0 <- (Si) shifted right 64-jk
    ShrS0,
    /// 054ijk Si <- (Si) shifted left jk
    ShlS,
    /// 055ijk Si <- (Si) shifted right 64-jk
    ShrS,
    /// 056ijk Si <- high word of ((Si),(Sj)) shifted left (Ak)
    ShlDS,
    /// 057ijk Si <- low word of ((Sj),(Si)) shifted right (Ak)
    ShrDS,
    /// 060ijk Si <- (Sj) + (Sk)
    AddS,
    /// 061ijk Si <- (Sj) - (Sk)
    SubS,
    /// 062ijk Si <- (Sj) +F (Sk)
    FAddS,
    /// 063ijk Si <- (Sj) -F (Sk)
    FSubS,
    /// 064ijk Si <- (Sj) *F (Sk)
    FMulS,
    /// 065ijk Si <- (Sj) *H (Sk), half precision rounded
    HMulS,
    /// 066ijk Si <- (Sj) *R (Sk), rounded
    RMulS,
    /// 067ijk Si <- 2 - (Sj) * (Sk), reciprocal iteration
    IMulS,
    /// 070ijx Si <- reciprocal approximation of (Sj)
    RecipS,
    /// 071i0k Si <- (Ak), no sign extension
    SFromA,
    /// 071i1k Si <- (Ak), sign extended
    SFromASigned,
    /// 071i2k Si <- (Ak) as an unnormalized floating point number
    SFromAFloat,
    /// 071i3x Si <- 0.75 * 2**48 (CAL `0.6`)
    SConstPoint6,
    /// 071i4x Si <- 0.5 (CAL `0.4`)
    SConstPoint4,
    /// 071i5x Si <- 1.0
    SConst1,
    /// 071i6x Si <- 2.0
    SConst2,
    /// 071i7x Si <- 4.0
    SConst4,
    /// 072ixx Si <- (RTC)
    SFromRt,
    /// 073ixx Si <- (VM)
    SFromVm,
    /// 074ijk Si <- (Tjk)
    SFromT,
    /// 075ijk Tjk <- (Si)
    TFromS,
    /// 076ijk Si <- (Vj element (Ak))
    SFromV,
    /// 077ijk Vi element (Ak) <- (Sj)
    VFromS,
    /// 10hijkm Ai <- memory at (Ah) + jkm
    LoadA,
    /// 11hijkm memory at (Ah) + jkm <- (Ai)
    StoreA,
    /// 12hijkm Si <- memory at (Ah) + jkm
    LoadS,
    /// 13hijkm memory at (Ah) + jkm <- (Si)
    StoreS,
    /// 140ijk Vi <- (Sj) and (Vk)
    AndSV,
    /// 141ijk Vi <- (Vj) and (Vk)
    AndVV,
    /// 142ijk Vi <- (Sj) or (Vk)
    OrSV,
    /// 143ijk Vi <- (Vj) or (Vk)
    OrVV,
    /// 144ijk Vi <- (Sj) xor (Vk)
    XorSV,
    /// 145ijk Vi <- (Vj) xor (Vk)
    XorVV,
    /// 146ijk Vi <- (Sj) where VM bit is 1, (Vk) where it is 0
    MergeSV,
    /// 147ijk Vi <- (Vj) where VM bit is 1, (Vk) where it is 0
    MergeVV,
    /// 150ijk Vi <- (Vj) shifted left (Ak)
    ShlV,
    /// 151ijk Vi <- (Vj) shifted right (Ak)
    ShrV,
    /// 152ijk Vi <- double shift of (Vj) left (Ak)
    ShlDV,
    /// 153ijk Vi <- double shift of (Vj) right (Ak)
    ShrDV,
    /// 154ijk Vi <- (Sj) + (Vk)
    AddSV,
    /// 155ijk Vi <- (Vj) + (Vk)
    AddVV,
    /// 156ijk Vi <- (Sj) - (Vk)
    SubSV,
    /// 157ijk Vi <- (Vj) - (Vk)
    SubVV,
    /// 160ijk Vi <- (Sj) *F (Vk)
    FMulSV,
    /// 161ijk Vi <- (Vj) *F (Vk)
    FMulVV,
    /// 162ijk Vi <- (Sj) *H (Vk)
    HMulSV,
    /// 163ijk Vi <- (Vj) *H (Vk)
    HMulVV,
    /// 164ijk Vi <- (Sj) *R (Vk)
    RMulSV,
    /// 165ijk Vi <- (Vj) *R (Vk)
    RMulVV,
    /// 166ijk Vi <- 2 - (Sj) * (Vk)
    IMulSV,
    /// 167ijk Vi <- 2 - (Vj) * (Vk)
    IMulVV,
    /// 170ijk Vi <- (Sj) +F (Vk)
    FAddSV,
    /// 171ijk Vi <- (Vj) +F (Vk)
    FAddVV,
    /// 172ijk Vi <- (Sj) -F (Vk)
    FSubSV,
    /// 173ijk Vi <- (Vj) -F (Vk)
    FSubVV,
    /// 174ijx Vi <- reciprocal approximation of (Vj)
    RecipV,
    /// 174ij1 Vi <- population counts of (Vj)
    PopV,
    /// 174ij2 Vi <- population count parities of (Vj)
    ParityV,
    /// 175xjk, k low bits 0: VM bit set where (Vj) = 0
    VmZero,
    /// 175xjk, k low bits 1: VM bit set where (Vj) != 0
    VmNonzero,
    /// 175xjk, k low bits 2: VM bit set where (Vj) positive
    VmPositive,
    /// 175xjk, k low bits 3: VM bit set where (Vj) negative
    VmNegative,
    /// 176ixk Vi <- (VL) words of memory from (A0) stepping by (Ak)
    VLoad,
    /// 177xjk memory from (A0) stepping by (Ak) <- (VL) words of Vj
    VStore,
}

/// Number of `Op` variants.
pub const OP_COUNT: usize = Op::VStore as usize + 1;

/// What a table row is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The general form of an instruction.  `decode` always returns a base row
    /// and each `Op` has exactly one.
    Base,
    /// A special syntax form of Appendix D (marked with a dagger there): the
    /// same instruction with some fields fixed.  The disassembler prefers it.
    Special,
    /// Another spelling the assembler accepts.  The disassembler uses it only
    /// when no other spelling reproduces the encoding.
    Alt,
}

/// The functional unit an instruction occupies (the UNIT column of Appendix D).
/// Vector floating point instructions use the same floating point units as
/// the scalar ones; `Form::is_vector` tells them apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Unit {
    None,
    AddrAdd,
    AddrMul,
    ScalarAdd,
    ScalarLogical,
    ScalarShift,
    PopLz,
    FpAdd,
    FpMul,
    FpRecip,
    VecLogical,
    VecShift,
    VecAdd,
    VecPop,
    Memory,
}

impl Unit {
    /// The unit name as printed in Appendix D ("-" for none).
    pub fn name(self) -> &'static str {
        match self {
            Unit::None => "-",
            Unit::AddrAdd => "A Int Add",
            Unit::AddrMul => "A Int Mult",
            Unit::ScalarAdd => "S Int Add",
            Unit::ScalarLogical => "S Logical",
            Unit::ScalarShift => "S Shift",
            Unit::PopLz => "Pop/LZ",
            Unit::FpAdd => "F.P. Add",
            Unit::FpMul => "F.P. Mult",
            Unit::FpRecip => "F.P. Rcpl",
            Unit::VecLogical => "V Logical",
            Unit::VecShift => "V Shift",
            Unit::VecAdd => "V Int Add",
            Unit::VecPop => "V Pop",
            Unit::Memory => "Memory",
        }
    }
}

/// How the `exp` of a CAL template maps onto instruction fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpKind {
    /// The form has no expression operand.
    None,
    /// exp = ijk, 0 to 777 octal (`ERR exp`, `EX exp`).
    Ijk,
    /// exp = j, 0 to 7 (cluster number).
    J,
    /// exp = jk, 0 to 63 (short constant, left shift count, left mask length).
    Jk,
    /// exp = 64 - jk, 1 to 64 (right shift count, right mask length).
    JkRev,
    /// exp = jkm, 0 to 2**22 - 1.
    Jkm,
    /// exp = one's complement of jkm, -2**22 to -1.
    JkmNot,
    /// exp = jkm as a 22-bit two's complement displacement or address;
    /// -2**21 to 2**22 - 1 is accepted, decode gives -2**21 to 2**21 - 1.
    JkmSigned,
    /// exp = low 24 bits of ijkm, a parcel address.  P is 22 bits wide: a
    /// value of 2**22 or more (either low bit of i set) is a program range
    /// error on the machine.  The top bit of i is ignored.
    Ijkm,
}

/// Value dependent choice of encoding made by `assemble` (CAL rules).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sel {
    None,
    /// `Ai exp`: 022 if 0 to 63 and not a forward reference, else 020 if
    /// positive, else 021 with the complement.
    ImmA,
    /// `Ai #exp`: 021 with jkm = exp if exp is positive, else 020 with the
    /// complement of exp.
    ImmANot,
    /// `Si exp`: 040 if positive, else 041 with the complement.
    ImmS,
    /// `Si #exp`: 041 with jkm = exp if exp is positive, else 040 with the
    /// complement of exp.
    ImmSNot,
    /// A count the form cannot express (64 for `ExpKind::Jk`, 0 for
    /// `ExpKind::JkRev`) assembles as the given instruction with jk = 0.
    Edge(Op),
}

/// A register named by a form, before the instruction fields are known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegRef {
    Ai,
    Aj,
    Ak,
    Ah,
    A0,
    Si,
    Sj,
    Sk,
    S0,
    Vi,
    Vj,
    Vk,
    Bjk,
    Tjk,
    B00,
    /// (Ai) B registers starting at Bjk, wrapping after B77.
    BBlock,
    /// (Ai) T registers starting at Tjk, wrapping after T77.
    TBlock,
    Vl,
    Vm,
    Rtc,
    Xa,
}

/// Flag bits of `Form::flags`.
pub mod flag {
    /// Changes P: 005 to 017.
    pub const BRANCH: u16 = 1 << 0;
    /// Conditional branch: 010 to 017.
    pub const COND: u16 = 1 << 1;
    /// Reads memory: 034, 036, 10h, 12h, 176.
    pub const MEM_READ: u16 = 1 << 2;
    /// Writes memory: 035, 037, 11h, 13h, 177.
    pub const MEM_WRITE: u16 = 1 << 3;
    /// Acts only in monitor mode, a no-op otherwise: 0010 to 0014.
    pub const MONITOR: u16 = 1 << 4;
    /// Causes an exchange sequence: 000, 004.
    pub const EXIT: u16 = 1 << 5;
    /// Operates on (VL) elements of vector registers: 140 to 177.
    pub const VECTOR: u16 = 1 << 6;
    /// The spelling requires j != 0 (Appendix D notes "j != 0").
    pub const J_NONZERO: u16 = 1 << 7;
    /// The spelling is not in Appendix D (it comes from the CAL manual
    /// examples in cal_dv.txt or from later CAL).
    pub const NOT_IN_APPENDIX_D: u16 = 1 << 8;
    /// The instruction belongs to an option of the 1982 machine (HR-0004
    /// rev F): the programmable clock or the vector population instructions.
    /// Its page reference is to rev F.
    pub const OPTION: u16 = 1 << 9;
    /// The row exists only on the X-MP (`Cpu::Xmp`); its page reference is to
    /// the X-MP mainframe reference manual CSM-0111000.
    pub const XMP: u16 = 1 << 10;
}

pub(crate) const VAR_H: u8 = 1;
pub(crate) const VAR_I: u8 = 2;
pub(crate) const VAR_J: u8 = 4;
pub(crate) const VAR_K: u8 = 8;
pub(crate) const VAR_M: u8 = 16;
pub(crate) const M_ZERO: u8 = 32;

/// One row of the instruction table.
#[derive(Clone, Copy, Debug)]
pub struct Form {
    /// The instruction this row assembles to.
    pub op: Op,
    pub kind: Kind,
    /// Opcode pattern in Appendix D notation: six octal digits of the first
    /// parcel with `h`, `i`, `j`, `k` for operand fields and `x` for ignored
    /// fields, followed by `m` for a second parcel (or `0` for a second
    /// parcel that must be zero).
    pub pattern: &'static str,
    /// Length in parcels, 1 or 2.
    pub parcels: u8,
    /// CAL result field template (Appendix D notation).  Empty when CAL has
    /// no spelling for the row.
    pub result: &'static str,
    /// CAL operand field template.
    pub operand: &'static str,
    /// Meaning of `exp` in the templates.
    pub exp: ExpKind,
    /// Value dependent encoding choice made when assembling this spelling.
    pub sel: Sel,
    /// Page of the manual describing the instruction.
    pub page: &'static str,
    /// Short description.
    pub desc: &'static str,
    /// Bits of the first parcel that identify the row.
    pub mask: u16,
    /// Value of the masked bits.
    pub bits: u16,
    /// Bits of the first parcel the machine ignores; zero when encoded.
    pub dont_care: u16,
    pub(crate) var: u8,
    pub(crate) unit: Unit,
    pub(crate) flags: u16,
    pub(crate) reads: &'static [RegRef],
    pub(crate) writes: &'static [RegRef],
}

const fn octal(c: u8) -> u16 {
    if c < b'0' || c > b'7' {
        panic!("bad octal digit in opcode pattern");
    }
    (c - b'0') as u16
}

const fn new(
    kind: Kind,
    op: Op,
    pattern: &'static str,
    result: &'static str,
    operand: &'static str,
    page: &'static str,
    desc: &'static str,
) -> Form {
    let b = pattern.as_bytes();
    if b.len() != 6 && b.len() != 7 {
        panic!("opcode pattern must have 6 or 7 characters");
    }
    let mut mask: u16 = 0;
    let mut bits: u16 = 0;
    let mut dont_care: u16 = 0;
    let mut var: u8 = 0;
    // g: one bit and one octal digit
    if octal(b[0]) > 1 {
        panic!("first digit of an opcode pattern is 0 or 1");
    }
    mask |= 0x8000 | (7 << 12);
    bits |= (octal(b[0]) << 15) | (octal(b[1]) << 12);
    // h, i, j, k
    let mut n = 2;
    while n < 6 {
        let shift = (5 - n) * 3;
        let c = b[n];
        let letter = b"hijk"[n - 2];
        if c == letter {
            var |= 1u8 << (n - 2);
        } else if c == b'x' && n > 2 {
            dont_care |= 7 << shift;
        } else {
            mask |= 7 << shift;
            bits |= octal(c) << shift;
        }
        n += 1;
    }
    let mut parcels = 1;
    if b.len() == 7 {
        parcels = 2;
        if b[6] == b'm' {
            var |= VAR_M;
        } else if b[6] == b'0' {
            var |= M_ZERO;
        } else {
            panic!("seventh character of an opcode pattern is m or 0");
        }
    }
    Form {
        op,
        kind,
        pattern,
        parcels,
        result,
        operand,
        exp: ExpKind::None,
        sel: Sel::None,
        page,
        desc,
        mask,
        bits,
        dont_care,
        var,
        unit: Unit::None,
        flags: 0,
        reads: &[],
        writes: &[],
    }
}

const fn base(
    op: Op,
    pattern: &'static str,
    result: &'static str,
    operand: &'static str,
    page: &'static str,
    desc: &'static str,
) -> Form {
    new(Kind::Base, op, pattern, result, operand, page, desc)
}

const fn spec(
    op: Op,
    pattern: &'static str,
    result: &'static str,
    operand: &'static str,
    page: &'static str,
    desc: &'static str,
) -> Form {
    new(Kind::Special, op, pattern, result, operand, page, desc)
}

const fn alt(
    op: Op,
    pattern: &'static str,
    result: &'static str,
    operand: &'static str,
    page: &'static str,
    desc: &'static str,
) -> Form {
    new(Kind::Alt, op, pattern, result, operand, page, desc)
}

impl Form {
    const fn e(mut self, exp: ExpKind) -> Form {
        self.exp = exp;
        self
    }
    const fn s(mut self, sel: Sel) -> Form {
        self.sel = sel;
        self
    }
    const fn u(mut self, unit: Unit) -> Form {
        self.unit = unit;
        self
    }
    const fn f(mut self, flags: u16) -> Form {
        self.flags |= flags;
        self
    }
    const fn r(mut self, reads: &'static [RegRef]) -> Form {
        self.reads = reads;
        self
    }
    const fn w(mut self, writes: &'static [RegRef]) -> Form {
        self.writes = writes;
        self
    }
    /// Mark more bits of the first parcel as ignored.
    const fn dc(mut self, bits: u16) -> Form {
        self.mask &= !bits;
        self.bits &= !bits;
        self.dont_care |= bits;
        self
    }
}

use flag::*;
use ExpKind as E;
use Op::*;
use RegRef::*;
use Unit as U;

/// Extra flag for spellings taken from cal_dv.txt or later CAL.
const X: u16 = NOT_IN_APPENDIX_D;
const BR: u16 = BRANCH;
const CBR: u16 = BRANCH | COND;
const VEC: u16 = VECTOR;
const OPT: u16 = OPTION;
const XM: u16 = XMP;

/// The instruction table.
///
/// Order matters in three places:
/// * `decode` returns the first `Kind::Base` row whose pattern matches, so a
///   catch-all row follows the rows it completes (001ixx, 002ixx, 033);
/// * `assemble` tries rows whose templates have no `exp` first, then the
///   rest, each group in table order, and takes the first spelling that
///   matches;
/// * `disassemble` tries `Special` rows, then the `Base` row, then `Alt`
///   rows, each group in table order.
#[rustfmt::skip]
pub static FORMS: &[Form] = &[
    // ---- 000 to 004: exits, monitor functions, VL and VM
    base(Err, "000xxx", "ERR", "", "4-7", "Error exit").f(EXIT),
    alt(Err, "000ijk", "ERR", "exp", "4-7", "Error exit").e(E::Ijk),
    base(SetCa, "0010jk", "CA,Aj", "Ak", "4-8", "Set the channel (Aj) current address to (Ak) and begin the I/O sequence").f(MONITOR).r(&[Aj, Ak]),
    spec(SetCa, "001000", "PASS", "", "4-8", "Pass (0010jk is a no-op when j = 0)").f(X),
    base(SetCl, "0011jk", "CL,Aj", "Ak", "4-8", "Set the channel (Aj) limit address to (Ak)").f(MONITOR).r(&[Aj, Ak]),
    base(ChanMc, "0012j1", "MC,Aj", "", "X 5-9", "Clear channel (Aj) flags; set device master clear (output) or clear a held ready (input)").f(MONITOR | XM).r(&[Aj]),
    base(ClearCi, "0012jx", "CI,Aj", "", "4-8", "Clear channel (Aj) interrupt flag").f(MONITOR).r(&[Aj]),
    base(SetXa, "0013jx", "XA", "Aj", "4-8", "Enter XA register with (Aj)").f(MONITOR).r(&[Aj]).w(&[Xa]),
    base(SetRt, "0014j0", "RT", "Sj", "4-8", "Enter real-time clock register with (Sj)").f(MONITOR).r(&[Sj]).w(&[Rtc]),
    base(SetPci, "0014j4", "PCI", "Sj", "F 4-10", "Enter interrupt interval register with (Sj)").f(MONITOR | OPT).r(&[Sj]),
    base(Cci, "0014x5", "CCI", "", "F 4-10", "Clear the programmable clock interrupt request").f(MONITOR | OPT),
    base(Eci, "0014x6", "ECI", "", "F 4-10", "Enable the programmable clock interrupt request").f(MONITOR | OPT),
    base(Dci, "0014x7", "DCI", "", "F 4-10", "Disable the programmable clock interrupt request").f(MONITOR | OPT),
    base(SetCln, "0014j3", "CLN", "exp", "X 5-11", "Enter cluster number register with j").e(E::J).f(MONITOR | XM),
    base(ClockPass, "0014xk", "", "", "F 4-10", "Pass (0014jk with k = 1, 2 or 3 is not defined)").f(X),
    base(MonitorPass, "001ixx", "", "", "4-8", "Pass (monitor function with i = 5, 6 or 7)").f(X),
    base(SetVl, "0020xk", "VL", "Ak", "4-10", "Transmit (Ak) to VL register").r(&[Ak]).w(&[Vl]),
    spec(SetVl, "0020x0", "VL", "1", "4-10", "Transmit 1 to VL register"),
    base(Efi, "0021xx", "EFI", "", "4-10.1", "Enable interrupt on floating point error"),
    base(Dfi, "0022xx", "DFI", "", "4-10.1", "Disable interrupt on floating point error"),
    base(Eri, "0023xx", "ERI", "", "X 5-15", "Enable interrupt on operand range error").f(XM),
    base(Dri, "0024xx", "DRI", "", "X 5-15", "Disable interrupt on operand range error").f(XM),
    base(Dbm, "0025xx", "DBM", "", "X 5-15", "Disable bidirectional memory transfers").f(XM),
    base(Ebm, "0026xx", "EBM", "", "X 5-15", "Enable bidirectional memory transfers").f(XM),
    base(Cmr, "0027xx", "CMR", "", "X 5-15", "Complete memory references").f(XM),
    base(Undefined, "002ixx", "", "", "", "Not defined by the Cray-1 manual (0023xx to 0027xx)").f(X),
    base(SemTestSet, "0034jk", "SMjk", "1,TS", "X 5-17", "Test and set semaphore jk").f(XM),
    base(SemClear, "0036jk", "SMjk", "0", "X 5-17", "Clear semaphore jk").f(XM),
    base(SemSet, "0037jk", "SMjk", "1", "X 5-17", "Set semaphore jk").f(XM),
    base(SetVm, "003xjx", "VM", "Sj", "4-11", "Transmit (Sj) to VM register").r(&[Sj]).w(&[Vm]),
    spec(SetVm, "003x0x", "VM", "0", "4-11", "Clear VM register"),
    base(Ex, "004xxx", "EX", "", "4-12", "Normal exit").f(EXIT),
    alt(Ex, "004ijk", "EX", "exp", "4-12", "Normal exit").e(E::Ijk),
    // ---- 005 to 017: branches
    base(JumpB, "005xjk", "J", "Bjk", "4-13", "Jump to (Bjk)").f(BR).r(&[Bjk]),
    base(Jump, "006ijkm", "J", "exp", "4-14", "Jump to exp").e(E::Ijkm).dc(0o400).f(BR),
    base(ReturnJump, "007ijkm", "R", "exp", "4-15", "Return jump to exp; set B00 to P").e(E::Ijkm).dc(0o400).f(BR).w(&[B00]),
    base(Jaz, "010ijkm", "JAZ", "exp", "4-16", "Branch to exp if (A0) = 0").e(E::Ijkm).dc(0o400).f(CBR).r(&[A0]),
    base(Jan, "011ijkm", "JAN", "exp", "4-16", "Branch to exp if (A0) not 0").e(E::Ijkm).dc(0o400).f(CBR).r(&[A0]),
    base(Jap, "012ijkm", "JAP", "exp", "4-16", "Branch to exp if (A0) positive").e(E::Ijkm).dc(0o400).f(CBR).r(&[A0]),
    base(Jam, "013ijkm", "JAM", "exp", "4-16", "Branch to exp if (A0) negative").e(E::Ijkm).dc(0o400).f(CBR).r(&[A0]),
    base(Jsz, "014ijkm", "JSZ", "exp", "4-17", "Branch to exp if (S0) = 0").e(E::Ijkm).dc(0o400).f(CBR).r(&[S0]),
    base(Jsn, "015ijkm", "JSN", "exp", "4-17", "Branch to exp if (S0) not 0").e(E::Ijkm).dc(0o400).f(CBR).r(&[S0]),
    base(Jsp, "016ijkm", "JSP", "exp", "4-17", "Branch to exp if (S0) positive").e(E::Ijkm).dc(0o400).f(CBR).r(&[S0]),
    base(Jsm, "017ijkm", "JSM", "exp", "4-17", "Branch to exp if (S0) negative").e(E::Ijkm).dc(0o400).f(CBR).r(&[S0]),
    // ---- 020 to 033: A registers
    base(ImmA, "020ijkm", "Ai", "exp", "4-18", "Transmit exp = jkm to Ai").e(E::Jkm).s(Sel::ImmA).w(&[Ai]),
    alt(ImmA, "020ijkm", "Ai", "#exp", "4-18", "Transmit complement of exp = jkm to Ai (exp negative)").e(E::JkmNot).s(Sel::ImmANot).f(X),
    base(ImmANot, "021ijkm", "Ai", "exp", "4-18", "Transmit exp = 1's complement of jkm to Ai").e(E::JkmNot).s(Sel::ImmA).w(&[Ai]),
    alt(ImmANot, "021ijkm", "Ai", "#exp", "4-18", "Transmit complement of exp = jkm to Ai").e(E::Jkm).s(Sel::ImmANot).f(X),
    base(ImmAShort, "022ijk", "Ai", "exp", "4-19", "Transmit exp = jk to Ai").e(E::Jk).s(Sel::ImmA).w(&[Ai]),
    base(AFromS, "023ijx", "Ai", "Sj", "4-20", "Transmit (Sj) to Ai").r(&[Sj]).w(&[Ai]),
    base(AFromB, "024ijk", "Ai", "Bjk", "4-21", "Transmit (Bjk) to Ai").r(&[Bjk]).w(&[Ai]),
    base(BFromA, "025ijk", "Bjk", "Ai", "4-21", "Transmit (Ai) to Bjk").r(&[Ai]).w(&[Bjk]),
    base(AFromSb, "026ij7", "Ai", "SBj", "X 5-32", "Transmit (SBj) to Ai").f(XM).w(&[Ai]),
    base(PopParity, "026ij1", "Ai", "QSj", "F 4-25", "Population count parity of (Sj) to Ai").u(U::PopLz).f(OPT).r(&[Sj]).w(&[Ai]),
    base(PopCount, "026ijx", "Ai", "PSj", "4-22", "Population count of (Sj) to Ai").u(U::PopLz).r(&[Sj]).w(&[Ai]),
    base(SbFromA, "027ij7", "SBj", "Ai", "X 5-34", "Transmit (Ai) to SBj").f(XM).r(&[Ai]),
    base(LeadingZeros, "027ijx", "Ai", "ZSj", "4-23", "Leading zero count of (Sj) to Ai").u(U::PopLz).r(&[Sj]).w(&[Ai]),
    base(AddA, "030ijk", "Ai", "Aj+Ak", "4-24", "Integer sum of (Aj) and (Ak) to Ai").u(U::AddrAdd).r(&[Aj, Ak]).w(&[Ai]),
    spec(AddA, "030i0k", "Ai", "Ak", "4-24", "Transmit (Ak) to Ai"),
    spec(AddA, "030ij0", "Ai", "Aj+1", "4-24", "Integer sum of (Aj) and 1 to Ai"),
    base(SubA, "031ijk", "Ai", "Aj-Ak", "4-24", "Integer difference of (Aj) less (Ak) to Ai").u(U::AddrAdd).r(&[Aj, Ak]).w(&[Ai]),
    spec(SubA, "031i00", "Ai", "-1", "4-24", "Transmit -1 to Ai"),
    spec(SubA, "031i0k", "Ai", "-Ak", "4-24", "Transmit the negative of (Ak) to Ai"),
    spec(SubA, "031ij0", "Ai", "Aj-1", "4-24", "Integer difference of (Aj) less 1 to Ai"),
    base(MulA, "032ijk", "Ai", "Aj*Ak", "4-25", "Integer product of (Aj) and (Ak) to Ai").u(U::AddrMul).r(&[Aj, Ak]).w(&[Ai]),
    base(ChanInt, "033i0x", "Ai", "CI", "4-26", "Channel number to Ai (j = 0)").w(&[Ai]),
    base(ChanAddr, "033ij0", "Ai", "CA,Aj", "4-26", "Address of channel (Aj) to Ai (j not 0; k even)").dc(6).f(J_NONZERO).r(&[Aj]).w(&[Ai]),
    base(ChanErr, "033ij1", "Ai", "CE,Aj", "4-26", "Error flag of channel (Aj) to Ai (j not 0; k odd)").dc(6).f(J_NONZERO).r(&[Aj]).w(&[Ai]),
    // ---- 034 to 037: B and T block transfers
    base(BLoad, "034ijk", "Bjk,Ai", ",A0", "4-28", "Read (Ai) words to B register jk from (A0)").u(U::Memory).f(MEM_READ).r(&[Ai, A0]).w(&[BBlock]),
    alt(BLoad, "034ijk", "Bjk,Ai", "0,A0", "4-28", "Read (Ai) words to B register jk from (A0)"),
    alt(BLoad, "034ijk", "Bjk,Ai", ",", "4-28", "Read (Ai) words to B register jk from (A0)").f(X),
    base(BStore, "035ijk", ",A0", "Bjk,Ai", "4-28", "Store (Ai) words at B register jk to (A0)").u(U::Memory).f(MEM_WRITE).r(&[Ai, A0, BBlock]),
    alt(BStore, "035ijk", "0,A0", "Bjk,Ai", "4-28", "Store (Ai) words at B register jk to (A0)"),
    alt(BStore, "035ijk", ",", "Bjk,Ai", "4-28", "Store (Ai) words at B register jk to (A0)").f(X),
    base(TLoad, "036ijk", "Tjk,Ai", ",A0", "4-28", "Read (Ai) words to T register jk from (A0)").u(U::Memory).f(MEM_READ).r(&[Ai, A0]).w(&[TBlock]),
    alt(TLoad, "036ijk", "Tjk,Ai", "0,A0", "4-28", "Read (Ai) words to T register jk from (A0)"),
    alt(TLoad, "036ijk", "Tjk,Ai", ",", "4-28", "Read (Ai) words to T register jk from (A0)").f(X),
    base(TStore, "037ijk", ",A0", "Tjk,Ai", "4-28", "Store (Ai) words at T register jk to (A0)").u(U::Memory).f(MEM_WRITE).r(&[Ai, A0, TBlock]),
    alt(TStore, "037ijk", "0,A0", "Tjk,Ai", "4-28", "Store (Ai) words at T register jk to (A0)"),
    alt(TStore, "037ijk", ",", "Tjk,Ai", "4-28", "Store (Ai) words at T register jk to (A0)").f(X),
    // ---- 040 to 043: S immediates and masks
    base(ImmS, "040ijkm", "Si", "exp", "4-30", "Transmit exp = jkm to Si").e(E::Jkm).s(Sel::ImmS).w(&[Si]),
    alt(ImmS, "040ijkm", "Si", "#exp", "4-30", "Transmit complement of exp = jkm to Si (exp negative)").e(E::JkmNot).s(Sel::ImmSNot).f(X),
    base(ImmSNot, "041ijkm", "Si", "exp", "4-30", "Transmit exp = 1's complement of jkm to Si").e(E::JkmNot).s(Sel::ImmS).w(&[Si]),
    alt(ImmSNot, "041ijkm", "Si", "#exp", "4-30", "Transmit complement of exp = jkm to Si").e(E::Jkm).s(Sel::ImmSNot).f(X),
    spec(MaskRight, "042i77", "Si", "1", "4-31", "Enter 1 into Si"),
    spec(MaskRight, "042i00", "Si", "-1", "4-31", "Enter -1 into Si"),
    base(MaskRight, "042ijk", "Si", "<exp", "4-31", "Form 1's mask exp = 64-jk bits in Si from the right").e(E::JkRev).s(Sel::Edge(MaskLeft)).u(U::ScalarLogical).w(&[Si]),
    alt(MaskRight, "042ijk", "Si", "#>exp", "4-31", "Form 1's mask 64-exp = 64-jk bits in Si from the right").e(E::Jk).s(Sel::Edge(MaskLeft)),
    spec(MaskLeft, "043i00", "Si", "0", "4-31", "Clear Si"),
    base(MaskLeft, "043ijk", "Si", ">exp", "4-31", "Form 1's mask exp = jk bits in Si from the left").e(E::Jk).s(Sel::Edge(MaskRight)).u(U::ScalarLogical).w(&[Si]),
    alt(MaskLeft, "043ijk", "Si", "#<exp", "4-31", "Form 1's mask 64-exp = jk bits in Si from the left").e(E::JkRev).s(Sel::Edge(MaskRight)),
    // ---- 044 to 051: S logical
    base(AndS, "044ijk", "Si", "Sj&Sk", "4-33", "Logical product of (Sj) and (Sk) to Si").u(U::ScalarLogical).r(&[Sj, Sk]).w(&[Si]),
    spec(AndS, "044ij0", "Si", "Sj&SB", "4-33", "Sign bit of (Sj) to Si"),
    alt(AndS, "044ij0", "Si", "SB&Sj", "4-33", "Sign bit of (Sj) to Si (j not 0)").f(J_NONZERO),
    base(AndNotS, "045ijk", "Si", "#Sk&Sj", "4-33", "Logical product of (Sj) and 1's complement of (Sk) to Si").u(U::ScalarLogical).r(&[Sj, Sk]).w(&[Si]),
    spec(AndNotS, "045ij0", "Si", "#SB&Sj", "4-33", "(Sj) with sign bit cleared to Si"),
    base(XorS, "046ijk", "Si", "Sj\\Sk", "4-33", "Logical difference of (Sj) and (Sk) to Si").u(U::ScalarLogical).r(&[Sj, Sk]).w(&[Si]),
    spec(XorS, "046ij0", "Si", "Sj\\SB", "4-33", "Toggle sign bit of Sj, then enter into Si"),
    alt(XorS, "046ij0", "Si", "SB\\Sj", "4-33", "Toggle sign bit of Sj, then enter into Si (j not 0)").f(J_NONZERO),
    base(EqvS, "047ijk", "Si", "#Sj\\Sk", "4-33", "Logical equivalence of (Sk) and (Sj) to Si").u(U::ScalarLogical).r(&[Sj, Sk]).w(&[Si]),
    spec(EqvS, "047i00", "Si", "#SB", "4-33", "Enter 1's complement of sign bit into Si"),
    spec(EqvS, "047i0k", "Si", "#Sk", "4-33", "Transmit 1's complement of (Sk) to Si"),
    spec(EqvS, "047ij0", "Si", "#Sj\\SB", "4-33", "Logical equivalence of (Sj) and sign bit to Si"),
    alt(EqvS, "047ij0", "Si", "#SB\\Sj", "4-33", "Logical equivalence of (Sj) and sign bit to Si (j not 0)").f(J_NONZERO),
    base(MergeS, "050ijk", "Si", "Sj!Si&Sk", "4-33", "Logical product of (Si) and (Sk) complement ORed with logical product of (Sj) and (Sk) to Si").u(U::ScalarLogical).r(&[Si, Sj, Sk]).w(&[Si]),
    spec(MergeS, "050ij0", "Si", "Sj!Si&SB", "4-33", "Scalar merge of (Si) and sign bit of (Sj) to Si"),
    base(OrS, "051ijk", "Si", "Sj!Sk", "4-33", "Logical sum of (Sj) and (Sk) to Si").u(U::ScalarLogical).r(&[Sj, Sk]).w(&[Si]),
    spec(OrS, "051i00", "Si", "SB", "4-33", "Enter sign bit into Si"),
    spec(OrS, "051i0k", "Si", "Sk", "4-33", "Transmit (Sk) to Si"),
    spec(OrS, "051ij0", "Si", "Sj!SB", "4-33", "Logical sum of (Sj) and sign bit to Si"),
    alt(OrS, "051ij0", "Si", "SB!Sj", "4-33", "Logical sum of (Sj) and sign bit to Si (j not 0)").f(J_NONZERO),
    // ---- 052 to 057: S shifts
    base(ShlS0, "052ijk", "S0", "Si<exp", "4-35", "Shift (Si) left exp = jk places to S0").e(E::Jk).s(Sel::Edge(ShrS0)).u(U::ScalarShift).r(&[Si]).w(&[S0]),
    base(ShrS0, "053ijk", "S0", "Si>exp", "4-35", "Shift (Si) right exp = 64-jk places to S0").e(E::JkRev).s(Sel::Edge(ShlS0)).u(U::ScalarShift).r(&[Si]).w(&[S0]),
    base(ShlS, "054ijk", "Si", "Si<exp", "4-35", "Shift (Si) left exp = jk places").e(E::Jk).s(Sel::Edge(ShrS)).u(U::ScalarShift).r(&[Si]).w(&[Si]),
    base(ShrS, "055ijk", "Si", "Si>exp", "4-35", "Shift (Si) right exp = 64-jk places").e(E::JkRev).s(Sel::Edge(ShlS)).u(U::ScalarShift).r(&[Si]).w(&[Si]),
    base(ShlDS, "056ijk", "Si", "Si,Sj<Ak", "4-36", "Shift (Si and Sj) left (Ak) places to Si").u(U::ScalarShift).r(&[Si, Sj, Ak]).w(&[Si]),
    spec(ShlDS, "056ij0", "Si", "Si,Sj<1", "4-36", "Shift (Si and Sj) left one place to Si"),
    spec(ShlDS, "056i0k", "Si", "Si<Ak", "4-36", "Shift (Si) left (Ak) places to Si"),
    base(ShrDS, "057ijk", "Si", "Sj,Si>Ak", "4-36", "Shift (Sj and Si) right (Ak) places to Si").u(U::ScalarShift).r(&[Si, Sj, Ak]).w(&[Si]),
    spec(ShrDS, "057ij0", "Si", "Sj,Si>1", "4-36", "Shift (Sj and Si) right one place to Si"),
    spec(ShrDS, "057i0k", "Si", "Si>Ak", "4-36", "Shift (Si) right (Ak) places to Si"),
    // ---- 060 to 070: S arithmetic
    base(AddS, "060ijk", "Si", "Sj+Sk", "4-37", "Integer sum of (Sj) and (Sk) to Si").u(U::ScalarAdd).r(&[Sj, Sk]).w(&[Si]),
    base(SubS, "061ijk", "Si", "Sj-Sk", "4-37", "Integer difference of (Sj) and (Sk) to Si").u(U::ScalarAdd).r(&[Sj, Sk]).w(&[Si]),
    spec(SubS, "061i0k", "Si", "-Sk", "4-37", "Transmit negative of (Sk) to Si"),
    base(FAddS, "062ijk", "Si", "Sj+FSk", "4-38", "Floating sum of (Sj) and (Sk) to Si").u(U::FpAdd).r(&[Sj, Sk]).w(&[Si]),
    spec(FAddS, "062i0k", "Si", "+FSk", "4-38", "Normalize (Sk) to Si"),
    base(FSubS, "063ijk", "Si", "Sj-FSk", "4-38", "Floating difference of (Sj) and (Sk) to Si").u(U::FpAdd).r(&[Sj, Sk]).w(&[Si]),
    spec(FSubS, "063i0k", "Si", "-FSk", "4-38", "Transmit normalized negative of (Sk) to Si"),
    base(FMulS, "064ijk", "Si", "Sj*FSk", "4-38", "Floating product of (Sj) and (Sk) to Si").u(U::FpMul).r(&[Sj, Sk]).w(&[Si]),
    base(HMulS, "065ijk", "Si", "Sj*HSk", "4-39", "Half precision rounded floating product of (Sj) and (Sk) to Si").u(U::FpMul).r(&[Sj, Sk]).w(&[Si]),
    base(RMulS, "066ijk", "Si", "Sj*RSk", "4-39", "Full precision rounded floating product of (Sj) and (Sk) to Si").u(U::FpMul).r(&[Sj, Sk]).w(&[Si]),
    base(IMulS, "067ijk", "Si", "Sj*ISk", "4-39", "2 - floating product of (Sj) and (Sk) to Si").u(U::FpMul).r(&[Sj, Sk]).w(&[Si]),
    base(RecipS, "070ijx", "Si", "/HSj", "4-41", "Floating reciprocal approximation of (Sj) to Si").u(U::FpRecip).r(&[Sj]).w(&[Si]),
    // ---- 071 to 077: S transmits
    base(SFromA, "071i0k", "Si", "Ak", "4-42", "Transmit (Ak) to Si with no sign extension").r(&[Ak]).w(&[Si]),
    base(SFromASigned, "071i1k", "Si", "+Ak", "4-42", "Transmit (Ak) to Si with sign extension").r(&[Ak]).w(&[Si]),
    base(SFromAFloat, "071i2k", "Si", "+FAk", "4-42", "Transmit (Ak) to Si as unnormalized floating point number").r(&[Ak]).w(&[Si]),
    base(SConstPoint6, "071i3x", "Si", "0.6", "4-42", "Transmit constant 0.75*2**48 to Si").w(&[Si]),
    base(SConstPoint4, "071i4x", "Si", "0.4", "4-42", "Transmit constant 0.5 to Si").w(&[Si]),
    base(SConst1, "071i5x", "Si", "1.", "4-42", "Transmit constant 1.0 to Si").w(&[Si]),
    base(SConst2, "071i6x", "Si", "2.", "4-42", "Transmit constant 2.0 to Si").w(&[Si]),
    base(SConst4, "071i7x", "Si", "4.", "4-42", "Transmit constant 4.0 to Si").w(&[Si]),
    base(SFromSm, "072i02", "Si", "SM", "X 5-59", "Transmit the semaphores to Si").f(XM).w(&[Si]),
    base(SFromSt, "072ij3", "Si", "STj", "X 5-59", "Transmit (STj) to Si").f(XM).w(&[Si]),
    base(SFromRt, "072ixx", "Si", "RT", "4-44", "Transmit (RTC) to Si").r(&[Rtc]).w(&[Si]),
    base(SFromSr, "073i01", "Si", "SR0", "X 5-59", "Transmit the status register to Si").f(XM).w(&[Si]),
    base(SmFromS, "073i02", "SM", "Si", "X 5-59", "Transmit (Si) to the semaphores").f(XM).r(&[Si]),
    base(StFromS, "073ij3", "STj", "Si", "X 5-59", "Transmit (Si) to STj").f(XM).r(&[Si]),
    base(SFromVm, "073ixx", "Si", "VM", "4-44", "Transmit (VM) to Si").r(&[Vm]).w(&[Si]),
    base(SFromT, "074ijk", "Si", "Tjk", "4-44", "Transmit (Tjk) to Si").r(&[Tjk]).w(&[Si]),
    base(TFromS, "075ijk", "Tjk", "Si", "4-44", "Transmit (Si) to Tjk").r(&[Si]).w(&[Tjk]),
    base(SFromV, "076ijk", "Si", "Vj,Ak", "4-45", "Transmit (Vj element (Ak)) to Si").r(&[Vj, Ak]).w(&[Si]),
    base(VFromS, "077ijk", "Vi,Ak", "Sj", "4-45", "Transmit (Sj) to Vi element (Ak)").r(&[Sj, Ak]).w(&[Vi]),
    spec(VFromS, "077i0k", "Vi,Ak", "0", "4-45", "Clear Vi element (Ak)"),
    // ---- 10h to 13h: scalar memory references
    base(LoadA, "10hijkm", "Ai", "exp,Ah", "4-46", "Read from ((Ah) + exp) to Ai (A0 = 0)").e(E::JkmSigned).u(U::Memory).f(MEM_READ).r(&[Ah]).w(&[Ai]),
    spec(LoadA, "100ijkm", "Ai", "exp,0", "4-46", "Read from (exp) to Ai").e(E::JkmSigned),
    alt(LoadA, "100ijkm", "Ai", "exp,", "4-46", "Read from (exp) to Ai").e(E::JkmSigned),
    spec(LoadA, "10hi000", "Ai", ",Ah", "4-46", "Read from (Ah) to Ai"),
    base(StoreA, "11hijkm", "exp,Ah", "Ai", "4-46", "Store (Ai) to (Ah) + exp (A0 = 0)").e(E::JkmSigned).u(U::Memory).f(MEM_WRITE).r(&[Ah, Ai]),
    spec(StoreA, "110ijkm", "exp,0", "Ai", "4-46", "Store (Ai) to exp").e(E::JkmSigned),
    alt(StoreA, "110ijkm", "exp,", "Ai", "4-46", "Store (Ai) to exp").e(E::JkmSigned),
    spec(StoreA, "11hi000", ",Ah", "Ai", "4-46", "Store (Ai) to (Ah)"),
    base(LoadS, "12hijkm", "Si", "exp,Ah", "4-46", "Read from ((Ah) + exp) to Si (A0 = 0)").e(E::JkmSigned).u(U::Memory).f(MEM_READ).r(&[Ah]).w(&[Si]),
    spec(LoadS, "120ijkm", "Si", "exp,0", "4-46", "Read from exp to Si").e(E::JkmSigned),
    alt(LoadS, "120ijkm", "Si", "exp,", "4-46", "Read from exp to Si").e(E::JkmSigned),
    spec(LoadS, "12hi000", "Si", ",Ah", "4-46", "Read from (Ah) to Si"),
    base(StoreS, "13hijkm", "exp,Ah", "Si", "4-46", "Store (Si) to (Ah) + exp (A0 = 0)").e(E::JkmSigned).u(U::Memory).f(MEM_WRITE).r(&[Ah, Si]),
    spec(StoreS, "130ijkm", "exp,0", "Si", "4-46", "Store (Si) to exp").e(E::JkmSigned),
    alt(StoreS, "130ijkm", "exp,", "Si", "4-46", "Store (Si) to exp").e(E::JkmSigned),
    spec(StoreS, "13hi000", ",Ah", "Si", "4-46", "Store (Si) to (Ah)"),
    // ---- 140 to 147: V logical
    base(AndSV, "140ijk", "Vi", "Sj&Vk", "4-48", "Logical products of (Sj) and (Vk) to Vi").u(U::VecLogical).f(VEC).r(&[Sj, Vk, Vl]).w(&[Vi]),
    spec(AndSV, "140i00", "Vi", "0", "4-48", "Clear Vi"),
    base(AndVV, "141ijk", "Vi", "Vj&Vk", "4-48", "Logical products of (Vj) and (Vk) to Vi").u(U::VecLogical).f(VEC).r(&[Vj, Vk, Vl]).w(&[Vi]),
    base(OrSV, "142ijk", "Vi", "Sj!Vk", "4-48", "Logical sums of (Sj) and (Vk) to Vi").u(U::VecLogical).f(VEC).r(&[Sj, Vk, Vl]).w(&[Vi]),
    spec(OrSV, "142i0k", "Vi", "Vk", "4-48", "Transmit (Vk) to Vi"),
    base(OrVV, "143ijk", "Vi", "Vj!Vk", "4-48", "Logical sums of (Vj) and (Vk) to Vi").u(U::VecLogical).f(VEC).r(&[Vj, Vk, Vl]).w(&[Vi]),
    base(XorSV, "144ijk", "Vi", "Sj\\Vk", "4-48", "Logical differences of (Sj) and (Vk) to Vi").u(U::VecLogical).f(VEC).r(&[Sj, Vk, Vl]).w(&[Vi]),
    base(XorVV, "145ijk", "Vi", "Vj\\Vk", "4-48", "Logical differences of (Vj) and (Vk) to Vi").u(U::VecLogical).f(VEC).r(&[Vj, Vk, Vl]).w(&[Vi]),
    base(MergeSV, "146ijk", "Vi", "Sj!Vk&VM", "4-48", "Transmit (Sj) if VM bit = 1; (Vk) if VM bit = 0 to Vi").u(U::VecLogical).f(VEC).r(&[Sj, Vk, Vm, Vl]).w(&[Vi]),
    spec(MergeSV, "146i0k", "Vi", "#VM&Vk", "4-48", "Vector merge of (Vk) and 0 to Vi"),
    base(MergeVV, "147ijk", "Vi", "Vj!Vk&VM", "4-48", "Transmit (Vj) if VM bit = 1; (Vk) if VM bit = 0 to Vi").u(U::VecLogical).f(VEC).r(&[Vj, Vk, Vm, Vl]).w(&[Vi]),
    // ---- 150 to 153: V shifts
    base(ShlV, "150ijk", "Vi", "Vj<Ak", "4-52", "Shift (Vj) left (Ak) places to Vi").u(U::VecShift).f(VEC).r(&[Vj, Ak, Vl]).w(&[Vi]),
    spec(ShlV, "150ij0", "Vi", "Vj<1", "4-52", "Shift (Vj) left one place to Vi"),
    base(ShrV, "151ijk", "Vi", "Vj>Ak", "4-52", "Shift (Vj) right (Ak) places to Vi").u(U::VecShift).f(VEC).r(&[Vj, Ak, Vl]).w(&[Vi]),
    spec(ShrV, "151ij0", "Vi", "Vj>1", "4-52", "Shift (Vj) right one place to Vi"),
    base(ShlDV, "152ijk", "Vi", "Vj,Vj<Ak", "4-53", "Double shift (Vj) left (Ak) places to Vi").u(U::VecShift).f(VEC).r(&[Vj, Ak, Vl]).w(&[Vi]),
    spec(ShlDV, "152ij0", "Vi", "Vj,Vj<1", "4-53", "Double shift (Vj) left one place to Vi"),
    base(ShrDV, "153ijk", "Vi", "Vj,Vj>Ak", "4-53", "Double shift (Vj) right (Ak) places to Vi").u(U::VecShift).f(VEC).r(&[Vj, Ak, Vl]).w(&[Vi]),
    spec(ShrDV, "153ij0", "Vi", "Vj,Vj>1", "4-53", "Double shift (Vj) right one place to Vi"),
    // ---- 154 to 157: V integer add
    base(AddSV, "154ijk", "Vi", "Sj+Vk", "4-56", "Integer sums of (Sj) and (Vk) to Vi").u(U::VecAdd).f(VEC).r(&[Sj, Vk, Vl]).w(&[Vi]),
    base(AddVV, "155ijk", "Vi", "Vj+Vk", "4-56", "Integer sums of (Vj) and (Vk) to Vi").u(U::VecAdd).f(VEC).r(&[Vj, Vk, Vl]).w(&[Vi]),
    base(SubSV, "156ijk", "Vi", "Sj-Vk", "4-56", "Integer differences of (Sj) and (Vk) to Vi").u(U::VecAdd).f(VEC).r(&[Sj, Vk, Vl]).w(&[Vi]),
    spec(SubSV, "156i0k", "Vi", "-Vk", "4-56", "Transmit negative of (Vk) to Vi"),
    base(SubVV, "157ijk", "Vi", "Vj-Vk", "4-56", "Integer differences of (Vj) and (Vk) to Vi").u(U::VecAdd).f(VEC).r(&[Vj, Vk, Vl]).w(&[Vi]),
    // ---- 160 to 174: V floating point
    base(FMulSV, "160ijk", "Vi", "Sj*FVk", "4-58", "Floating products of (Sj) and (Vk) to Vi").u(U::FpMul).f(VEC).r(&[Sj, Vk, Vl]).w(&[Vi]),
    base(FMulVV, "161ijk", "Vi", "Vj*FVk", "4-58", "Floating products of (Vj) and (Vk) to Vi").u(U::FpMul).f(VEC).r(&[Vj, Vk, Vl]).w(&[Vi]),
    base(HMulSV, "162ijk", "Vi", "Sj*HVk", "4-58", "Half precision rounded floating products of (Sj) and (Vk) to Vi").u(U::FpMul).f(VEC).r(&[Sj, Vk, Vl]).w(&[Vi]),
    base(HMulVV, "163ijk", "Vi", "Vj*HVk", "4-58", "Half precision rounded floating products of (Vj) and (Vk) to Vi").u(U::FpMul).f(VEC).r(&[Vj, Vk, Vl]).w(&[Vi]),
    base(RMulSV, "164ijk", "Vi", "Sj*RVk", "4-58", "Rounded floating products of (Sj) and (Vk) to Vi").u(U::FpMul).f(VEC).r(&[Sj, Vk, Vl]).w(&[Vi]),
    base(RMulVV, "165ijk", "Vi", "Vj*RVk", "4-58", "Rounded floating products of (Vj) and (Vk) to Vi").u(U::FpMul).f(VEC).r(&[Vj, Vk, Vl]).w(&[Vi]),
    base(IMulSV, "166ijk", "Vi", "Sj*IVk", "4-58", "2 - floating products of (Sj) and (Vk) to Vi").u(U::FpMul).f(VEC).r(&[Sj, Vk, Vl]).w(&[Vi]),
    base(IMulVV, "167ijk", "Vi", "Vj*IVk", "4-58", "2 - floating products of (Vj) and (Vk) to Vi").u(U::FpMul).f(VEC).r(&[Vj, Vk, Vl]).w(&[Vi]),
    base(FAddSV, "170ijk", "Vi", "Sj+FVk", "4-61", "Floating sums of (Sj) and (Vk) to Vi").u(U::FpAdd).f(VEC).r(&[Sj, Vk, Vl]).w(&[Vi]),
    spec(FAddSV, "170i0k", "Vi", "+FVk", "4-61", "Normalize (Vk) to Vi"),
    base(FAddVV, "171ijk", "Vi", "Vj+FVk", "4-61", "Floating sums of (Vj) and (Vk) to Vi").u(U::FpAdd).f(VEC).r(&[Vj, Vk, Vl]).w(&[Vi]),
    base(FSubSV, "172ijk", "Vi", "Sj-FVk", "4-61", "Floating differences of (Sj) and (Vk) to Vi").u(U::FpAdd).f(VEC).r(&[Sj, Vk, Vl]).w(&[Vi]),
    spec(FSubSV, "172i0k", "Vi", "-FVk", "4-61", "Transmit normalized negatives of (Vk) to Vi"),
    base(FSubVV, "173ijk", "Vi", "Vj-FVk", "4-61", "Floating differences of (Vj) and (Vk) to Vi").u(U::FpAdd).f(VEC).r(&[Vj, Vk, Vl]).w(&[Vi]),
    base(PopV, "174ij1", "Vi", "PVj", "F 4-70", "Population counts of (Vj) to Vi").u(U::VecPop).f(VEC | OPT).r(&[Vj, Vl]).w(&[Vi]),
    base(ParityV, "174ij2", "Vi", "QVj", "F 4-70", "Population count parities of (Vj) to Vi").u(U::VecPop).f(VEC | OPT).r(&[Vj, Vl]).w(&[Vi]),
    base(RecipV, "174ijx", "Vi", "/HVj", "4-63", "Floating reciprocal approximations of (Vj) to Vi").u(U::FpRecip).f(VEC).r(&[Vj, Vl]).w(&[Vi]),
    // ---- 175 to 177: vector mask and vector memory references
    base(VmZero, "175xj0", "VM", "Vj,Z", "4-65", "VM = 1 where (Vj) = 0").dc(4).u(U::VecLogical).f(VEC).r(&[Vj, Vl]).w(&[Vm]),
    base(VmNonzero, "175xj1", "VM", "Vj,N", "4-65", "VM = 1 where (Vj) not 0").dc(4).u(U::VecLogical).f(VEC).r(&[Vj, Vl]).w(&[Vm]),
    base(VmPositive, "175xj2", "VM", "Vj,P", "4-65", "VM = 1 where (Vj) positive").dc(4).u(U::VecLogical).f(VEC).r(&[Vj, Vl]).w(&[Vm]),
    base(VmNegative, "175xj3", "VM", "Vj,M", "4-65", "VM = 1 where (Vj) negative").dc(4).u(U::VecLogical).f(VEC).r(&[Vj, Vl]).w(&[Vm]),
    base(VLoad, "176ixk", "Vi", ",A0,Ak", "4-67", "Read (VL) words to Vi from (A0) incremented by (Ak)").u(U::Memory).f(VEC | MEM_READ).r(&[A0, Ak, Vl]).w(&[Vi]),
    spec(VLoad, "176ix0", "Vi", ",A0,1", "4-67", "Read (VL) words to Vi from (A0) incremented by 1"),
    alt(VLoad, "176ixk", "Vi", ",,Ak", "4-67", "Read (VL) words to Vi from (A0) incremented by (Ak)").f(X),
    alt(VLoad, "176ix0", "Vi", ",,1", "4-67", "Read (VL) words to Vi from (A0) incremented by 1").f(X),
    base(VStore, "177xjk", ",A0,Ak", "Vj", "4-67", "Store (VL) words from Vj to (A0) incremented by (Ak)").u(U::Memory).f(VEC | MEM_WRITE).r(&[A0, Ak, Vj, Vl]),
    spec(VStore, "177xj0", ",A0,1", "Vj", "4-67", "Store (VL) words from Vj to (A0) incremented by 1"),
    alt(VStore, "177xjk", ",,Ak", "Vj", "4-67", "Store (VL) words from Vj to (A0) incremented by (Ak)").f(X),
    alt(VStore, "177xj0", ",,1", "Vj", "4-67", "Store (VL) words from Vj to (A0) incremented by 1").f(X),
];
