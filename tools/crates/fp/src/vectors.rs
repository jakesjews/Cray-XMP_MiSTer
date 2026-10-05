//! Deterministic test vectors and the text vector format that drives the test benches.
//!
//! # File format
//!
//! One case per line, five whitespace-separated fields:
//!
//! ```text
//! OP A B RESULT FLAGS
//! ```
//!
//! * `OP` is one of `add sub mul mulh mulr mul2m recip` (062, 063, 064, 065, 066, 067, 070).
//! * `A`, `B` and `RESULT` are 64-bit words as 16 hexadecimal digits. `B` is 0 for `recip`.
//! * `FLAGS` is one hexadecimal digit. Bit 0 is the floating-point range error. No other
//!   bit is used: the manuals define no other flag (underflow is silent).
//!
//! Blank lines are ignored and `#` starts a comment that runs to the end of the line.

use std::fmt;
use std::fs::File;
use std::io::{self, BufRead, BufWriter, Write};
use std::path::Path;

use crate::{
    fadd, fmul, frecip, fsub, pack, FpResult, MulKind, Profile, COEF_MASK, NORM_BIT, SIGN_BIT,
};

/// Operation named in a vector line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    /// 062 floating sum.
    Add,
    /// 063 floating difference.
    Sub,
    /// 064 floating product.
    Mul,
    /// 065 half-precision rounded floating product.
    MulH,
    /// 066 rounded floating product.
    MulR,
    /// 067 reciprocal iteration, `2 - A*B`.
    Mul2M,
    /// 070 reciprocal approximation of `A`.
    Recip,
}

impl Op {
    /// Every operation, in file-format order.
    pub const ALL: [Op; 7] = [
        Op::Add,
        Op::Sub,
        Op::Mul,
        Op::MulH,
        Op::MulR,
        Op::Mul2M,
        Op::Recip,
    ];

    /// The name used in vector files.
    pub fn name(self) -> &'static str {
        match self {
            Op::Add => "add",
            Op::Sub => "sub",
            Op::Mul => "mul",
            Op::MulH => "mulh",
            Op::MulR => "mulr",
            Op::Mul2M => "mul2m",
            Op::Recip => "recip",
        }
    }

    /// Inverse of [`Op::name`].
    pub fn from_name(name: &str) -> Option<Op> {
        Op::ALL.into_iter().find(|op| op.name() == name)
    }

    /// The multiply kind, for the four multiply operations.
    pub fn mul_kind(self) -> Option<MulKind> {
        match self {
            Op::Mul => Some(MulKind::Full),
            Op::MulH => Some(MulKind::HalfRounded),
            Op::MulR => Some(MulKind::Rounded),
            Op::Mul2M => Some(MulKind::TwoMinus),
            _ => None,
        }
    }

    /// Run the operation. `b` is ignored by `Recip`.
    pub fn eval(self, a: u64, b: u64, profile: Profile) -> FpResult {
        match self {
            Op::Add => fadd(a, b),
            Op::Sub => fsub(a, b),
            Op::Recip => frecip(a, profile),
            Op::Mul | Op::MulH | Op::MulR | Op::Mul2M => {
                fmul(a, b, self.mul_kind().unwrap(), profile)
            }
        }
    }
}

/// `FLAGS` bit 0: floating-point range error.
pub const FLAG_RANGE_ERROR: u8 = 1;

/// One line of a vector file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Vector {
    pub op: Op,
    pub a: u64,
    pub b: u64,
    pub result: u64,
    pub flags: u8,
}

impl Vector {
    /// Evaluate `op` on the operands with this crate and record the outcome.
    pub fn compute(op: Op, a: u64, b: u64, profile: Profile) -> Vector {
        let b = if op == Op::Recip { 0 } else { b };
        let r = op.eval(a, b, profile);
        Vector {
            op,
            a,
            b,
            result: r.value,
            flags: if r.range_error { FLAG_RANGE_ERROR } else { 0 },
        }
    }

    /// The result and flag as an [`FpResult`].
    pub fn expected(&self) -> FpResult {
        FpResult {
            value: self.result,
            range_error: self.flags & FLAG_RANGE_ERROR != 0,
        }
    }
}

impl fmt::Display for Vector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {:016X} {:016X} {:016X} {:X}",
            self.op.name(),
            self.a,
            self.b,
            self.result,
            self.flags
        )
    }
}

/// Parse one line. `Ok(None)` for a blank or comment-only line.
pub fn parse_line(line: &str) -> Result<Option<Vector>, String> {
    let text = line.split('#').next().unwrap_or("").trim();
    if text.is_empty() {
        return Ok(None);
    }
    let fields: Vec<&str> = text.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(format!("expected 5 fields, found {}", fields.len()));
    }
    let op =
        Op::from_name(fields[0]).ok_or_else(|| format!("unknown operation `{}`", fields[0]))?;
    let word = |s: &str| u64::from_str_radix(s, 16).map_err(|e| format!("bad hex word `{s}`: {e}"));
    let flags =
        u8::from_str_radix(fields[4], 16).map_err(|e| format!("bad flags `{}`: {e}", fields[4]))?;
    Ok(Some(Vector {
        op,
        a: word(fields[1])?,
        b: word(fields[2])?,
        result: word(fields[3])?,
        flags,
    }))
}

/// Read every vector from a reader. Errors carry the 1-based line number.
pub fn read_vectors<R: BufRead>(reader: R) -> io::Result<Vec<Vector>> {
    let mut out = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line?;
        match parse_line(&line) {
            Ok(Some(v)) => out.push(v),
            Ok(None) => {}
            Err(msg) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("line {}: {msg}", index + 1),
                ));
            }
        }
    }
    Ok(out)
}

/// Read every vector from a file.
pub fn read_vector_file<P: AsRef<Path>>(path: P) -> io::Result<Vec<Vector>> {
    read_vectors(io::BufReader::new(File::open(path)?))
}

/// Write `n` vectors for `op` (see [`vectors_for`]) as text lines.
pub fn write_vectors<W: Write>(
    out: &mut W,
    op: Op,
    profile: Profile,
    n: usize,
    seed: u64,
) -> io::Result<()> {
    for v in vectors_for(op, profile, n, seed) {
        writeln!(out, "{v}")?;
    }
    Ok(())
}

/// Write a vector file holding `n` vectors for each operation in `ops`, preceded by a
/// two-line `#` header. Returns the number of vectors written.
pub fn write_vector_file<P: AsRef<Path>>(
    path: P,
    ops: &[Op],
    profile: Profile,
    n: usize,
    seed: u64,
) -> io::Result<usize> {
    let mut out = BufWriter::new(File::create(path)?);
    writeln!(
        out,
        "# cray1-fp vectors: profile {profile:?}, {n} per operation, seed {seed:#x}"
    )?;
    writeln!(
        out,
        "# OP A B RESULT FLAGS (hex; FLAGS bit 0 = range error)"
    )?;
    for &op in ops {
        write_vectors(&mut out, op, profile, n, seed)?;
    }
    out.flush()?;
    Ok(ops.len() * n)
}

/// SplitMix64, the only random source used by the generator.
#[derive(Clone, Debug)]
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> SplitMix64 {
        SplitMix64(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform value in `0..bound` (`bound > 0`); the tiny modulo bias is irrelevant here.
    pub fn below(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }

    fn flag(&mut self) -> bool {
        self.next_u64() & 1 != 0
    }

    /// Random coefficient with bit 47 set.
    pub fn norm_coef(&mut self) -> u64 {
        (self.next_u64() & COEF_MASK) | NORM_BIT
    }
}

const BIAS: u16 = 0o40000;
const ONES: u64 = COEF_MASK;
const HALF: u64 = NORM_BIT;
/// Coefficient just below 1/sqrt(2): its square is just below one half.
const ROOT_HALF: u64 = 0xB504_F333_F9DE;

fn both(v: &mut Vec<(u64, u64)>, a: u64, b: u64) {
    v.push((a, b));
    v.push((b, a));
}

fn add_corners() -> Vec<(u64, u64)> {
    let mut v = Vec::new();
    let signs = [(false, false), (false, true), (true, false), (true, true)];

    // Exponent differences 0, 1, 47, 48, 49, 63, 64 and above.
    let coefs = [
        (ONES, ONES),
        (HALF, ONES),
        (HALF | 1, HALF | 1),
        (0xAAAA_AAAA_AAAA, 0xD555_5555_5555),
    ];
    for d in [0u16, 1, 2, 47, 48, 49, 63, 64, 65, 100, 0o17777] {
        for (ca, cb) in coefs {
            for (sa, sb) in signs {
                both(&mut v, pack(sa, BIAS + d, ca), pack(sb, BIAS, cb));
            }
        }
    }

    // Cancellation leaving k = 1..=48 leading zeros, equal exponents (48 = exact zero).
    for k in 1..=48u32 {
        let c = 0xC000_0012_3456u64;
        let c2 = if k == 48 { c } else { c - (1 << (47 - k)) };
        both(&mut v, pack(false, BIAS + 5, c), pack(true, BIAS + 5, c2));
    }
    // Cancellation leaving k = 1..=47 leading zeros after a one-place alignment that
    // drops a set bit.
    for k in 1..=47u32 {
        let c2 = (1u64 << 48) - (1u64 << (48 - k)) + 1;
        both(
            &mut v,
            pack(false, BIAS + 5, HALF),
            pack(true, BIAS + 4, c2),
        );
    }

    // Carry out of the 48-bit sum.
    for negative in [false, true] {
        let e = BIAS + 7;
        for (ca, eb, cb) in [
            (ONES, e, ONES),
            (HALF, e, HALF),
            (ONES, e, 1),
            (ONES, e - 1, ONES),
            (ONES, e - 47, ONES),
            (ONES, e - 48, ONES),
            (ONES - 1, e - 1, 3),
        ] {
            both(&mut v, pack(negative, e, ca), pack(negative, eb, cb));
        }
    }

    // Plus and minus zero operands.
    let zeros = [0u64, SIGN_BIT];
    let others = [
        pack(false, BIAS + 1, HALF),
        pack(true, BIAS + 1, ONES),
        pack(false, 0o40060, 0x2A),
        pack(true, 0o40060, 0x00FF_FFFF),
        pack(true, BIAS, 0x0000_FFFF_0000),
    ];
    for z in zeros {
        for z2 in zeros {
            v.push((z, z2));
        }
        for x in others {
            both(&mut v, z, x);
        }
    }

    // Unnormalised operands, including an unnormalised operand with the larger exponent
    // and zero coefficients under a non-zero exponent.
    for lz in [1u32, 2, 23, 24, 46, 47] {
        let c = ONES >> lz;
        let e = BIAS + 2;
        both(&mut v, pack(false, e, c), 0);
        both(&mut v, pack(true, e, c), pack(true, e, c));
        both(&mut v, pack(false, e, c), pack(true, e, HALF));
        both(&mut v, pack(false, e + 3, c), pack(false, e, ONES));
        both(&mut v, pack(false, e + 3, c), pack(true, e, ONES));
        both(
            &mut v,
            pack(true, e + 3, 1 << (47 - lz)),
            pack(false, e, ONES),
        );
    }
    both(&mut v, pack(false, BIAS + 10, 0), pack(false, BIAS, ONES));
    both(&mut v, pack(true, BIAS, 0), pack(false, BIAS, ONES));
    both(&mut v, pack(true, BIAS + 60, 0), pack(true, BIAS + 60, 0));

    // Exponents at the edges of the range.
    for e in [
        0o1u16, 0o17776, 0o17777, 0o20000, 0o20001, 0o57776, 0o57777, 0o60000, 0o60001, 0o77777,
    ] {
        for negative in [false, true] {
            both(&mut v, pack(negative, e, HALF), 0);
            both(&mut v, pack(negative, e, ONES), 0);
            both(&mut v, pack(negative, e, ONES), pack(negative, e, ONES));
            both(
                &mut v,
                pack(negative, e, HALF),
                pack(!negative, e, HALF | 1),
            );
            both(&mut v, pack(negative, e, HALF), pack(!negative, e, HALF));
            both(
                &mut v,
                pack(negative, e, 0xC000_0000_0000),
                pack(!negative, e, HALF),
            );
            both(&mut v, pack(negative, e, ONES), pack(negative, e - 1, ONES));
            both(
                &mut v,
                pack(negative, e, HALF),
                pack(!negative, e - 1, ONES),
            );
            if e < 0o77777 {
                both(&mut v, pack(negative, e, ONES), pack(negative, e + 1, HALF));
            }
        }
    }

    // Results on either side of the underflow boundary: exponent 020000 + k with
    // cancellation leaving k - 1, k and k + 1 leading zeros.
    for k in [1u32, 2, 10, 46] {
        for lz in [k - 1, k, k + 1] {
            let c = 0xFFFF_FFFF_FFFFu64;
            let c2 = if lz == 0 {
                0x7FFF_FFFF_FFFF
            } else {
                c - (1 << (47 - lz))
            };
            both(
                &mut v,
                pack(false, 0o20000 + k as u16, c),
                pack(true, 0o20000 + k as u16, c2),
            );
        }
    }
    v
}

fn mul_corners(kind: MulKind, profile: Profile) -> Vec<(u64, u64)> {
    let mut v = Vec::new();

    // Coefficients near all ones, near one half, near 1/sqrt(2), and unnormalised patterns.
    let pats = [
        ONES,
        ONES - 1,
        ONES ^ 0xFFFF,
        0xFFFF_FFFF_8000,
        0xFFFF_FFFE_0000,
        0xFFFF_FF00_0000,
        HALF,
        HALF | 1,
        HALF | 0x00FF_FFFF,
        0xAAAA_AAAA_AAAA,
        ROOT_HALF,
        ROOT_HALF + 1,
        0x5555_5555_5555,
        0x0000_00FF_FFFF,
    ];
    for a in pats {
        for b in pats {
            v.push((pack(false, BIAS + 1, a), pack(false, BIAS + 2, b)));
        }
    }
    // A single coefficient bit against several patterns, in both orders.
    for bit in 0..48u32 {
        let s = 1u64 << bit;
        for other in [ONES, HALF, s, 0xAAAA_AAAA_AAAA] {
            both(&mut v, pack(false, BIAS, s), pack(true, BIAS + 3, other));
        }
    }

    // Both exponents zero: integer multiply, no normalisation.
    both(&mut v, 6, 0x5400_0000_0000);
    both(&mut v, 4 << 24, 6 << 24);
    both(&mut v, 0x00FF_FFFF << 24, 0x00FF_FFFF << 24);
    for a in pats {
        both(&mut v, a, ONES);
        both(&mut v, SIGN_BIT | a, HALF);
        both(&mut v, SIGN_BIT | a, SIGN_BIT | a);
    }
    for (a, b) in [
        (0, 0),
        (SIGN_BIT, 0),
        (0, SIGN_BIT),
        (SIGN_BIT, SIGN_BIT),
        (SIGN_BIT, 5),
        (0, ONES),
    ] {
        v.push((a, b));
    }

    // Exactly one exponent zero.
    for e in [0o1u16, 0o17777, 0o20000, BIAS, 0o57777, 0o60000, 0o77777] {
        both(&mut v, pack(false, 0, ONES), pack(false, e, ONES));
        both(&mut v, 0, pack(true, e, HALF));
        both(&mut v, SIGN_BIT, pack(false, e, HALF));
    }

    // Exponent sums at the underflow and overflow edges, for products that do and do not
    // need the normalising shift, with operand exponents from every zone of the matrix.
    let shapes = [
        (HALF, HALF),
        (ONES, ONES),
        (ROOT_HALF, ROOT_HALF),
        (ROOT_HALF + 1, ROOT_HALF + 1),
        (0xC000_0000_0000, 0xC000_0000_0000),
    ];
    for target in [
        0o17776i32, 0o17777, 0o20000, 0o20001, 0o57776, 0o57777, 0o60000, 0o60001,
    ] {
        for ea in [
            0o1i32, 0o10000, 0o17777, 0o20000, 0o30000, 0o40000, 0o50000, 0o57777, 0o60000, 0o77777,
        ] {
            let eb = target + 0o40000 - ea;
            if !(1..=0o77777).contains(&eb) {
                continue;
            }
            for (ca, cb) in shapes {
                both(
                    &mut v,
                    pack(false, ea as u16, ca),
                    pack(true, eb as u16, cb),
                );
            }
        }
    }
    for (ea, eb) in [
        (0o60000u16, 0o1u16),
        (0o60000, 0o20000),
        (0o60000, 0o40000),
        (0o77777, 0o77777),
        (0o57777, 0o57777),
        (0o1, 0o1),
        (0o20000, 0o20000),
    ] {
        both(&mut v, pack(false, ea, ONES), pack(false, eb, HALF));
    }

    // Operands as the divide sequence presents them: a value and its reciprocal
    // approximation, and exact reciprocal pairs.
    if kind == MulKind::TwoMinus {
        for coef in [
            HALF,
            HALF | 1,
            0xC000_0000_0000,
            ONES,
            ROOT_HALF,
            0xAAAA_AAAA_AAAA,
            0x8000_0100_0000,
            0xFFFF_FFFF_0000,
        ] {
            for e in [BIAS - 3, BIAS, BIAS + 1, BIAS + 40] {
                let b = pack(false, e, coef);
                let r = frecip(b, profile).value;
                both(&mut v, r, b);
                both(&mut v, r ^ SIGN_BIT, b);
            }
        }
        let one = pack(false, BIAS + 1, HALF);
        v.push((one, one));
        both(&mut v, pack(false, BIAS, HALF), pack(false, BIAS + 2, HALF));
    }
    v
}

fn recip_corners() -> Vec<(u64, u64)> {
    let mut v = Vec::new();
    let mut rng = SplitMix64::new(0x7265_6369_705F_636F);

    // Every table index with minimum, maximum and random low bits.
    for index in 0..128u64 {
        let top = HALF | (index << 40);
        let low_mask = (1u64 << 40) - 1;
        for low in [0, low_mask, rng.next_u64() & low_mask] {
            v.push((pack(false, BIAS + 1, top | low), 0));
        }
    }
    // Coefficient exactly one half, across exponents and signs.
    for e in [0o20002u16, BIAS - 1, BIAS, BIAS + 1, BIAS + 2, 0o57777] {
        v.push((pack(false, e, HALF), 0));
        v.push((pack(true, e, HALF), 0));
    }
    // Unnormalised operands (bit 47 clear), which the unit does not detect.
    for coef in [
        0x4000_0000_0000u64,
        0x7FFF_FFFF_FFFF,
        0x0000_0000_0001,
        0x0080_0000_0000,
        0x7F00_0000_0000,
        0,
    ] {
        v.push((pack(false, BIAS + 1, coef), 0));
        v.push((pack(true, BIAS - 5, coef), 0));
    }
    // Zero, and the exponents on either side of the range limits.
    v.push((0, 0));
    v.push((SIGN_BIT, 0));
    for e in [
        0u16, 0o1, 0o17777, 0o20000, 0o20001, 0o20002, 0o20003, 0o57776, 0o57777, 0o60000, 0o60001,
        0o77777,
    ] {
        for coef in [HALF, 0xC000_0000_0000, ONES] {
            v.push((pack(false, e, coef), 0));
            v.push((pack(true, e, coef), 0));
        }
    }
    v
}

/// The fixed corner cases for `op`, as operand pairs (`b` is 0 for `Recip`).
///
/// * add, sub: exponent differences 0, 1, 2, 47, 48, 49, 63, 64, 65, 100 and 017777 in all
///   sign combinations; cancellation leaving 1 to 48 leading zeros; carry out; plus and
///   minus zero; unnormalised operands; exponents 1, 017776, 017777, 020000, 020001, 057776,
///   057777, 060000, 060001, 077777; results on both sides of the underflow boundary.
/// * mul, mulh, mulr, mul2m: coefficients near all ones, near one half and near 1/sqrt(2);
///   every single-bit coefficient; both exponents zero; exactly one exponent zero; exponent
///   sums 017776 to 020001 and 057776 to 060001 reached from every zone of the exponent
///   matrix; each asymmetric pair in both orders. `mul2m` adds value/reciprocal pairs.
/// * recip: all 128 table indices with minimum, maximum and random low bits; coefficient
///   exactly one half; unnormalised operands; zero; exponents 0, 1, 017777, 020000, 020001,
///   020002, 020003, 057776, 057777, 060000, 060001, 077777.
pub fn corner_operands(op: Op, profile: Profile) -> Vec<(u64, u64)> {
    match op {
        Op::Add | Op::Sub => add_corners(),
        Op::Recip => recip_corners(),
        Op::Mul | Op::MulH | Op::MulR | Op::Mul2M => mul_corners(op.mul_kind().unwrap(), profile),
    }
}

fn random_word(rng: &mut SplitMix64, exp: u16, coef: u64) -> u64 {
    pack(rng.flag(), exp, coef)
}

fn random_operands(op: Op, profile: Profile, rng: &mut SplitMix64, index: usize) -> (u64, u64) {
    let class = index % 4;
    match op {
        Op::Recip => {
            let a = match class {
                0 => rng.next_u64(),
                1 => {
                    let e = 0o20002 + rng.below(0o37776) as u16;
                    let coef = rng.norm_coef();
                    random_word(rng, e, coef)
                }
                2 => {
                    let low_mask = (1u64 << 40) - 1;
                    let low = match rng.below(4) {
                        0 => 0,
                        1 => low_mask,
                        2 => 1u64 << rng.below(40),
                        _ => low_mask ^ (1u64 << rng.below(40)),
                    };
                    let e = BIAS - 64 + rng.below(128) as u16;
                    let index = rng.below(128);
                    random_word(rng, e, HALF | (index << 40) | low)
                }
                _ => {
                    let e = [0o20001u16, 0o20002, 0o57777, 0o60000][rng.below(4) as usize];
                    let coef = (rng.next_u64() & COEF_MASK) >> rng.below(3);
                    random_word(rng, e, coef)
                }
            };
            (a, 0)
        }
        Op::Add | Op::Sub => match class {
            0 => (rng.next_u64(), rng.next_u64()),
            1 => {
                // Normalised operands, exponent difference 0..=50, either order.
                let d = rng.below(51) as u16;
                let e = 0o20100 + rng.below(0o37500) as u16;
                let ca = rng.norm_coef();
                let cb = rng.norm_coef();
                let a = random_word(rng, e + d, ca);
                let b = random_word(rng, e, cb);
                if rng.flag() {
                    (a, b)
                } else {
                    (b, a)
                }
            }
            2 => {
                // Opposite signs, exponents equal or one apart, coefficients sharing a
                // random number of leading bits: cancellation of every depth.
                let ca = rng.norm_coef();
                let keep = rng.below(48) as u32;
                let noise = (rng.next_u64() & COEF_MASK) >> keep;
                let cb = (ca ^ noise) | NORM_BIT;
                let e = 0o20000 + rng.below(0o40000) as u16;
                let d = rng.below(2) as u16;
                let negative = rng.flag();
                let a = pack(negative, e, ca);
                let b = pack(!negative, e.saturating_sub(d).max(1), cb);
                if rng.flag() {
                    (a, b)
                } else {
                    (b, a)
                }
            }
            _ => {
                // Unnormalised coefficients, nearby exponents.
                let ca = (rng.next_u64() & COEF_MASK) >> rng.below(48);
                let cb = (rng.next_u64() & COEF_MASK) >> rng.below(48);
                let e = 0o20000 + rng.below(0o40000) as u16;
                let d = rng.below(60) as u16;
                (
                    random_word(rng, e, ca),
                    random_word(rng, e.saturating_sub(d).max(1), cb),
                )
            }
        },
        Op::Mul | Op::MulH | Op::MulR | Op::Mul2M => match class {
            0 => (rng.next_u64(), rng.next_u64()),
            1 if op == Op::Mul2M => {
                // The operands 067 is meant for: a reciprocal approximation and its operand.
                let e = BIAS - 0o4000 + rng.below(0o10000) as u16;
                let coef = rng.norm_coef();
                let b = random_word(rng, e, coef);
                let r = frecip(b, profile).value;
                if rng.flag() {
                    (r, b)
                } else {
                    (b, r)
                }
            }
            1 => {
                // Normalised operands with an in-range product.
                let ea = BIAS - 0o4000 + rng.below(0o10000) as u16;
                let eb = BIAS - 0o4000 + rng.below(0o10000) as u16;
                let ca = rng.norm_coef();
                let cb = rng.norm_coef();
                (random_word(rng, ea, ca), random_word(rng, eb, cb))
            }
            2 => {
                // Exponent sum within three of a range edge, or a zero exponent.
                let ca = rng.norm_coef();
                let cb = rng.norm_coef();
                let edge = if rng.flag() { 0o20000i32 } else { 0o60000 };
                let target = edge - 3 + rng.below(7) as i32;
                let ea = 0o20000 + rng.below(0o40000) as i32;
                let eb = (target + 0o40000 - ea).clamp(0, 0o77777);
                let (ea, eb) = match rng.below(16) {
                    0 => (0, eb),
                    1 => (0, 0),
                    _ => (ea, eb),
                };
                (
                    random_word(rng, ea as u16, ca),
                    random_word(rng, eb as u16, cb),
                )
            }
            _ => {
                // Unnormalised coefficients.
                let ca = (rng.next_u64() & COEF_MASK) >> rng.below(24);
                let cb = (rng.next_u64() & COEF_MASK) >> rng.below(24);
                let ea = BIAS - 100 + rng.below(200) as u16;
                let eb = BIAS - 100 + rng.below(200) as u16;
                (random_word(rng, ea, ca), random_word(rng, eb, cb))
            }
        },
    }
}

/// `n` vectors for `op`, computed with [`Profile::Xmp`]. See [`vectors_for`].
pub fn vectors(op: Op, n: usize, seed: u64) -> impl Iterator<Item = Vector> {
    vectors_for(op, Profile::Xmp, n, seed)
}

/// `n` vectors for `op`: first the corner cases of [`corner_operands`] (all of them if `n`
/// is large enough), then pseudo-random cases that cycle through four classes: uniform
/// random 64-bit patterns; normalised operands (for add and sub with exponent differences
/// 0 to 50, for mul2m a value and its reciprocal approximation); range-edge and
/// cancellation cases; unnormalised operands.
///
/// The sequence depends only on `op`, `profile`, `n` and `seed`.
pub fn vectors_for(op: Op, profile: Profile, n: usize, seed: u64) -> impl Iterator<Item = Vector> {
    let corners = corner_operands(op, profile);
    let salt = Op::ALL.iter().position(|&o| o == op).unwrap() as u64;
    let mut rng = SplitMix64::new(seed ^ salt.wrapping_mul(0xA076_1D64_78BD_642F));
    (0..n).map(move |i| {
        let (a, b) = match corners.get(i) {
            Some(&pair) => pair,
            None => random_operands(op, profile, &mut rng, i - corners.len()),
        };
        Vector::compute(op, a, b, profile)
    })
}
