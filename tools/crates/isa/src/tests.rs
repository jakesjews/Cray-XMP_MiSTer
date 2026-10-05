use crate::*;

fn oct(parcels: &[u16]) -> String {
    parcels
        .iter()
        .map(|p| format!("{:06o}", p))
        .collect::<Vec<_>>()
        .join(" ")
}

fn asm(result: &str, operand: &str) -> Vec<u16> {
    match assemble_numeric(result, operand) {
        Ok(a) => a.encoding.parcels(),
        Err(e) => panic!("`{} {}`: {}", result, operand, e),
    }
}

fn asm_err(result: &str, operand: &str) -> String {
    match assemble_numeric(result, operand) {
        Ok(a) => panic!(
            "`{} {}` assembled to {}",
            result,
            operand,
            oct(&a.encoding.parcels())
        ),
        Err(e) => e,
    }
}

/// Which fields the templates and expression kind of a row give a value to.
fn bound_fields(f: &Form) -> (bool, bool, bool, bool, bool) {
    let text = format!("{} {}", f.result, f.operand);
    let has = |s: &str| text.contains(s);
    let mut h = has("Ah");
    let mut i = has("Ai") || has("Si") || has("Vi");
    let mut j = has("Aj") || has("Sj") || has("Vj") || has("Bjk") || has("Tjk");
    let mut k = has("Ak") || has("Sk") || has("Vk") || has("Bjk") || has("Tjk");
    let mut m = false;
    match f.exp {
        ExpKind::None => {}
        ExpKind::Ijk => (i, j, k) = (true, true, true),
        ExpKind::Jk | ExpKind::JkRev => (j, k) = (true, true),
        ExpKind::Jkm | ExpKind::JkmNot | ExpKind::JkmSigned => (j, k, m) = (true, true, true),
        ExpKind::Ijkm => (i, j, k, m) = (true, true, true, true),
    }
    h |= false;
    (h, i, j, k, m)
}

#[test]
fn table_is_consistent() {
    let mut bases = vec![0; OP_COUNT];
    for f in FORMS {
        if f.kind == Kind::Base {
            bases[f.op as usize] += 1;
        }
        assert_eq!(f.parcels as usize, f.pattern.len() - 5, "{}", f.pattern);
        assert_eq!(f.mask & f.dont_care, 0, "{}", f.pattern);
        assert_eq!(f.bits & !f.mask, 0, "{}", f.pattern);
        assert_eq!(
            f.exp != ExpKind::None,
            format!("{} {}", f.result, f.operand).contains("exp"),
            "{} {} {}",
            f.pattern,
            f.result,
            f.operand
        );
        if f.has_syntax() {
            let bound = bound_fields(f);
            let var = (f.uses_h(), f.uses_i(), f.uses_j(), f.uses_k(), f.uses_m());
            assert_eq!(
                bound, var,
                "{}: `{} {}` must give a value to exactly the fields the pattern names",
                f.pattern, f.result, f.operand
            );
        } else {
            assert!(f.operand.is_empty());
        }
        // a special or alternate row is a restriction or respelling of its base row
        let base = f.base();
        assert_eq!(base.kind, Kind::Base);
        assert_eq!(base.op, f.op);
        assert_eq!(base.parcels, f.parcels, "{}", f.pattern);
        assert_eq!(
            f.bits & base.mask,
            base.bits,
            "{} against {}",
            f.pattern,
            base.pattern
        );
        if f.kind != Kind::Base {
            assert!(f.has_syntax());
        }
    }
    for (n, count) in bases.iter().enumerate() {
        assert_eq!(*count, 1, "op number {} must have exactly one base row", n);
    }
}

#[test]
fn every_first_parcel_decodes_to_one_base_form() {
    let mut seen = [false; OP_COUNT];
    for p0 in 0..=0xffffu16 {
        let d = decode(p0, Some(0o123456));
        seen[d.op as usize] = true;
        assert_eq!(d.form.kind, Kind::Base);
        assert_eq!(d.len(), length(p0));
        assert_eq!(
            (d.g, d.h, d.i, d.j, d.k),
            (
                (p0 >> 12) as u8,
                (p0 >> 9) as u8 & 7,
                (p0 >> 6) as u8 & 7,
                (p0 >> 3) as u8 & 7,
                p0 as u8 & 7
            )
        );
        // the canonical encoding is the same instruction with the same operands
        let c = d.canonical();
        let again = decode(c.parcel0, c.parcel1);
        assert_eq!(again.op, d.op, "{:06o}", p0);
        assert!(again.is_canonical(), "{:06o}", p0);
        assert_eq!(again.exp, d.exp, "{:06o}", p0);
        assert_eq!(c.parcel0 & !d.form.dont_care, c.parcel0);
        assert_eq!(c.parcel0, p0 & !d.form.dont_care, "{:06o}", p0);
        assert_eq!(again.reads(), d.reads(), "{:06o}", p0);
        assert_eq!(again.writes(), d.writes(), "{:06o}", p0);
    }
    assert!(seen.iter().all(|s| *s));
}

#[test]
fn every_form_round_trips() {
    let mut spelled = 0;
    for f in FORMS {
        let fields = f.example_fields();
        let want = encode(f, fields);
        let d = decode(want.parcel0, want.parcel1);
        assert_eq!(d.op, f.op, "{}", f.pattern);
        assert!(f.matches(want.parcel0, want.parcel1), "{}", f.pattern);
        // only `ERR exp` and `EX exp` can set bits the machine ignores
        assert_eq!(
            d.is_canonical(),
            !matches!(f.pattern, "000ijk" | "004ijk"),
            "{}",
            f.pattern
        );
        let Some((result, operand)) = f.example() else {
            assert_eq!(
                disassemble(&d),
                format!("VWD       D'16/O'{:06o}", want.parcel0)
            );
            continue;
        };
        spelled += 1;
        // encode -> text of this row -> assemble
        let a = assemble_numeric(&result, &operand)
            .unwrap_or_else(|e| panic!("{} `{} {}`: {}", f.pattern, result, operand, e));
        assert_eq!(a.encoding, want, "{} `{} {}`", f.pattern, result, operand);
        assert_eq!(a.form.op, f.op);
        assert_eq!(a.form.kind, Kind::Base);
        // decode -> disassemble -> assemble
        let (r2, o2) = disassemble_fields(&d);
        let b = assemble_numeric(&r2, &o2)
            .unwrap_or_else(|e| panic!("{} `{} {}`: {}", f.pattern, r2, o2, e));
        assert_eq!(
            b.encoding, want,
            "{} disassembled as `{} {}`",
            f.pattern, r2, o2
        );
        // a base row's own example is what the disassembler prints for it,
        // unless a special form covers the example fields
        if f.kind == Kind::Special {
            assert_eq!((r2, o2), (result, operand), "{}", f.pattern);
        }
    }
    assert_eq!(spelled, FORMS.len() - 2);
}

#[test]
fn every_encoding_disassembles_to_text_that_assembles_back() {
    let mut raw = 0;
    for p0 in 0..=0xffffu16 {
        for m in [0u16, 1, 0o77, 0o100, 0o123456, 0o177777] {
            let d = decode(p0, Some(m));
            let want = d.canonical();
            let (result, operand) = disassemble_fields(&d);
            if result == "VWD" {
                raw += 1;
                let no_spelling = matches!(d.op, Op::MonitorPass | Op::Undefined);
                let alias = matches!(d.op, Op::ShlS | Op::ShrS) && d.i == 0;
                assert!(no_spelling || alias, "{:06o} {:06o} has no spelling", p0, m);
                let mut text = format!("D'16/O'{:06o}", want.parcel0);
                if let Some(m) = want.parcel1 {
                    text.push_str(&format!(",D'16/O'{:06o}", m));
                }
                assert_eq!(operand, text);
            } else {
                let a = assemble_numeric(&result, &operand).unwrap_or_else(|e| {
                    panic!("{:06o} {:06o} `{} {}`: {}", p0, m, result, operand, e)
                });
                // exact when the parcels are canonical or CAL can spell the ignored bits
                assert!(
                    a.encoding == want || a.encoding.parcels() == [p0, m][..d.len()],
                    "{:06o} {:06o} `{} {}` gave {}",
                    p0,
                    m,
                    result,
                    operand,
                    oct(&a.encoding.parcels())
                );
                assert_eq!(a.form.op, d.op);
            }
            if d.len() == 1 {
                break;
            }
        }
    }
    // 0015xx-0017xx, 0023xx-0027xx, 0540jk, 0550jk
    assert_eq!(raw, 3 * 64 + 5 * 64 + 64 + 64);
}

#[test]
fn ignored_fields_follow_the_cray_1_manual() {
    let op = |p0: u16| decode(p0, None).op;
    // 003xjx: VM <- (Sj) whatever i and k are (0034, 0036, 0037 are X-MP)
    for i in 0..8 {
        for k in 0..8 {
            let d = decode(0o003020 | i << 6 | k, None);
            assert_eq!((d.op, d.j), (Op::SetVm, 2));
            assert_eq!(disassemble(&d), "VM        S2");
            assert_eq!(d.canonical().parcel0, 0o003020);
        }
    }
    assert_eq!(disassemble(&decode(0o003407, None)), "VM        0");
    // 0014jx: RTC <- (Sj), k ignored
    for k in 0..8 {
        assert_eq!(op(0o001430 | k), Op::SetRt);
    }
    // 001 with i = 5, 6, 7 is a pass; 0023 to 0027 are not defined
    for i in 5..8 {
        assert_eq!(op(0o001000 | i << 6 | 0o25), Op::MonitorPass);
    }
    assert_eq!(op(0o002000), Op::SetVl);
    assert_eq!(op(0o002070), Op::SetVl);
    assert_eq!(op(0o002177), Op::Efi);
    assert_eq!(op(0o002277), Op::Dfi);
    for i in 3..8 {
        assert_eq!(op(0o002000 | i << 6), Op::Undefined);
    }
    // 026ijx and 027ijx: population and leading zero count whatever k is
    assert_eq!(op(0o026127), Op::PopCount);
    assert_eq!(op(0o027121), Op::LeadingZeros);
    assert_eq!(disassemble(&decode(0o026127, None)), "A1        PS2");
    // 070ijx and 174ijx: reciprocal whatever k is
    assert_eq!(op(0o070125), Op::RecipS);
    for k in 0..8 {
        assert_eq!(op(0o174120 | k), Op::RecipV);
    }
    // 071: j selects, k ignored for the constants
    assert_eq!(op(0o071137), Op::SConstPoint6);
    assert_eq!(op(0o071177), Op::SConst4);
    // 072ixx reads the real-time clock, 073ixx reads VM
    for jk in 0..64 {
        assert_eq!(op(0o072100 | jk), Op::SFromRt);
        assert_eq!(op(0o073100 | jk), Op::SFromVm);
    }
    assert_eq!(disassemble(&decode(0o073321, None)), "S3        VM");
    // 033: j = 0 is the channel number; otherwise k even is CA, k odd is CE
    assert_eq!(op(0o033107), Op::ChanInt);
    assert_eq!(op(0o033120), Op::ChanAddr);
    assert_eq!(op(0o033126), Op::ChanAddr);
    assert_eq!(op(0o033121), Op::ChanErr);
    assert_eq!(op(0o033127), Op::ChanErr);
    // 175xjk: only the low two bits of k select the test
    assert_eq!(op(0o175020), Op::VmZero);
    assert_eq!(op(0o175724), Op::VmZero);
    assert_eq!(op(0o175027), Op::VmNegative);
    assert_eq!(disassemble(&decode(0o175726, None)), "VM        V2,P");
    // 176ixk ignores j, 177xjk ignores i (no gather or scatter on a Cray-1)
    assert_eq!(disassemble(&decode(0o176173, None)), "V1        ,A0,A3");
    assert_eq!(disassemble(&decode(0o177723, None)), ",A0,A3    V2");
    assert_eq!(disassemble(&decode(0o176170, None)), "V1        ,A0,1");
    // branches: the top bit of i is ignored, the low two are address bits
    let d = decode(0o006400, Some(0o100));
    assert_eq!((d.op, d.exp), (Op::Jump, Some(0o100)));
    assert_eq!(d.canonical().parcel0, 0o006000);
    let d = decode(0o006100, Some(0));
    assert_eq!(d.exp, Some(1 << 22));
    assert_eq!(disassemble(&d), "J         20000000");
    // 000xxx and 004xxx ignore ijk, but CAL can put a value there
    assert_eq!(disassemble(&decode(0, None)), "ERR");
    assert_eq!(disassemble(&decode(0o000123, None)), "ERR       123");
    assert_eq!(disassemble(&decode(0o004000, None)), "EX");
    assert_eq!(disassemble(&decode(0o004027, None)), "EX        27");
    assert_eq!(disassemble(&decode(0o001000, None)), "PASS");
}

#[test]
fn register_lists_use_the_special_values_of_register_zero() {
    let d = decode(0o030123, None);
    assert_eq!(
        (d.reads(), d.writes()),
        (vec![Reg::A(2), Reg::A(3)], vec![Reg::A(1)])
    );
    assert_eq!(decode(0o030103, None).reads(), vec![Reg::A(3)]);
    assert_eq!(decode(0o030120, None).reads(), vec![Reg::A(2)]);
    assert_eq!(decode(0o030000, None).reads(), vec![]);
    assert_eq!(decode(0o030000, None).writes(), vec![Reg::A(0)]);
    assert_eq!(decode(0o044120, None).reads(), vec![Reg::S(2)]);
    assert_eq!(
        decode(0o050123, None).reads(),
        vec![Reg::S(1), Reg::S(2), Reg::S(3)]
    );
    assert_eq!(decode(0o052305, None).writes(), vec![Reg::S(0)]);
    assert_eq!(decode(0o010000, Some(0)).reads(), vec![Reg::A(0)]);
    assert_eq!(decode(0o017000, Some(0)).reads(), vec![Reg::S(0)]);
    assert_eq!(decode(0o007000, Some(0)).writes(), vec![Reg::B(0)]);
    assert_eq!(decode(0o100100, Some(5)).reads(), vec![]);
    assert_eq!(decode(0o103100, Some(5)).reads(), vec![Reg::A(3)]);
    assert_eq!(
        decode(0o133100, Some(5)).reads(),
        vec![Reg::A(3), Reg::S(1)]
    );
    assert_eq!(decode(0o034123, None).reads(), vec![Reg::A(1), Reg::A(0)]);
    assert_eq!(decode(0o034123, None).writes(), vec![Reg::BBlock(0o23)]);
    assert_eq!(
        decode(0o037123, None).reads(),
        vec![Reg::A(1), Reg::A(0), Reg::TBlock(0o23)]
    );
    assert_eq!(
        decode(0o146123, None).reads(),
        vec![Reg::S(2), Reg::V(3), Reg::Vm, Reg::Vl]
    );
    assert_eq!(
        decode(0o176103, None).reads(),
        vec![Reg::A(0), Reg::A(3), Reg::Vl]
    );
    assert_eq!(decode(0o176100, None).reads(), vec![Reg::A(0), Reg::Vl]);
    assert_eq!(decode(0o175020, None).writes(), vec![Reg::Vm]);
    assert_eq!(decode(0o002003, None).writes(), vec![Reg::Vl]);
}

#[test]
fn flags_and_units() {
    let f = |p0: u16| decode(p0, None).form;
    assert!(f(0).is_exit() && f(0o004000).is_exit() && !f(0o005000).is_exit());
    for gh in 0o005..=0o017u16 {
        assert!(f(gh << 9).is_branch());
        assert_eq!(f(gh << 9).is_conditional_branch(), gh >= 0o010);
    }
    assert!(!f(0o020000).is_branch());
    for i in 0..5 {
        assert!(f(0o001000 | i << 6).is_monitor_only());
    }
    assert!(!f(0o001500).is_monitor_only());
    assert!(!f(0o033000).is_monitor_only());
    for gh in [0o034u16, 0o036, 0o100, 0o107, 0o120, 0o127, 0o176] {
        assert!(
            f(gh << 9).reads_memory() && !f(gh << 9).writes_memory(),
            "{:o}",
            gh
        );
        assert_eq!(f(gh << 9).unit(), Unit::Memory);
    }
    for gh in [0o035u16, 0o037, 0o110, 0o117, 0o130, 0o137, 0o177] {
        assert!(
            f(gh << 9).writes_memory() && !f(gh << 9).reads_memory(),
            "{:o}",
            gh
        );
    }
    for gh in 0..0o200u16 {
        assert_eq!(f(gh << 9).is_vector(), gh >= 0o140, "{:o}", gh);
    }
    let unit = |gh: u16| f(gh << 9).unit();
    assert_eq!(unit(0o026), Unit::PopLz);
    assert_eq!(unit(0o030), Unit::AddrAdd);
    assert_eq!(unit(0o032), Unit::AddrMul);
    assert_eq!(unit(0o042), Unit::ScalarLogical);
    assert_eq!(unit(0o051), Unit::ScalarLogical);
    assert_eq!(unit(0o052), Unit::ScalarShift);
    assert_eq!(unit(0o057), Unit::ScalarShift);
    assert_eq!(unit(0o061), Unit::ScalarAdd);
    assert_eq!(unit(0o063), Unit::FpAdd);
    assert_eq!(unit(0o067), Unit::FpMul);
    assert_eq!(unit(0o070), Unit::FpRecip);
    assert_eq!(unit(0o071), Unit::None);
    assert_eq!(unit(0o147), Unit::VecLogical);
    assert_eq!(unit(0o153), Unit::VecShift);
    assert_eq!(unit(0o157), Unit::VecAdd);
    assert_eq!(unit(0o167), Unit::FpMul);
    assert_eq!(unit(0o173), Unit::FpAdd);
    assert_eq!(unit(0o174), Unit::FpRecip);
    assert_eq!(unit(0o175), Unit::VecLogical);
    // special rows answer for their instruction
    let special = FORMS.iter().find(|f| f.pattern == "030i0k").unwrap();
    assert_eq!(special.unit(), Unit::AddrAdd);
    assert_eq!(special.reads(), base_form(Op::AddA).reads());
}

#[test]
fn a_register_constants_pick_020_021_022() {
    assert_eq!(asm("A1", "0"), [0o022100]);
    assert_eq!(asm("A3", "10"), [0o022310]);
    assert_eq!(asm("A1", "77"), [0o022177]);
    assert_eq!(asm("A2", "100"), [0o020200, 0o000100]);
    assert_eq!(asm("A4", "1777777"), [0o020407, 0o177777]);
    assert_eq!(asm("A4", "17777777"), [0o020477, 0o177777]);
    assert_eq!(asm("A2", "-10"), [0o021200, 0o000007]);
    assert_eq!(asm("A2", "-20000000"), [0o021277, 0o177777]);
    // -1 is the special form of 031
    assert_eq!(asm("A2", "-1"), [0o031200]);
    assert_eq!(asm("A2", "-2"), [0o021200, 0o000001]);
    // # complements: positive gives 021, negative gives 020
    assert_eq!(asm("A2", "#10"), [0o021200, 0o000010]);
    assert_eq!(asm("A6", "#-1"), [0o020600, 0o000000]);
    assert_eq!(asm("A6", "#0"), [0o021600, 0o000000]);
    // a 24-bit pattern with the top bits set is a negative A value
    assert_eq!(asm("A1", "77777777"), [0o021100, 0o000000]);
    assert_eq!(asm("A1", "77777770"), [0o021100, 0o000007]);
    assert!(asm_err("A1", "20000000").contains("does not fit"));
    assert!(asm_err("A1", "-20000001").contains("does not fit"));
    // a forward reference always takes two parcels
    let mut forward = |_: &str, _| {
        Ok(ExpValue {
            value: 5,
            forward: true,
        })
    };
    let a = assemble("A1", "LATER", &mut forward).unwrap();
    assert_eq!(a.encoding.parcels(), [0o020100, 0o000005]);
    // what the disassembler prints for encodings CAL would shorten
    assert_eq!(disassemble(&decode(0o020100, Some(5))), "A1        #-6");
    assert_eq!(disassemble(&decode(0o020100, Some(0o100))), "A1        100");
    assert_eq!(disassemble(&decode(0o021100, Some(0))), "A1        #0");
    assert_eq!(disassemble(&decode(0o021100, Some(7))), "A1        -10");
    assert_eq!(disassemble(&decode(0o022105, None)), "A1        5");
}

#[test]
fn s_register_constants_masks_and_shifts() {
    assert_eq!(asm("S2", "130"), [0o040200, 0o000130]);
    assert_eq!(asm("S4", "-1777777"), [0o041407, 0o177776]);
    assert_eq!(asm("S3", "#2"), [0o041300, 0o000002]);
    assert_eq!(asm("S6", "#-1"), [0o040600, 0o000000]);
    // Appendix D special forms
    assert_eq!(asm("S6", "0"), [0o043600]);
    assert_eq!(asm("S6", "1"), [0o042677]);
    assert_eq!(asm("S6", "-1"), [0o042600]);
    assert_eq!(asm("S6", "2"), [0o040600, 0o000002]);
    assert!(asm_err("S1", "20000000").contains("does not fit"));
    // masks
    assert_eq!(asm("S2", "<5"), [0o042273]);
    assert_eq!(asm("S2", "#>73"), [0o042273]);
    assert_eq!(asm("S4", "<100"), [0o042400]);
    assert_eq!(asm("S5", "<0"), [0o043500]);
    assert_eq!(asm("S2", ">5"), [0o043205]);
    assert_eq!(asm("S2", "#<73"), [0o043205]);
    assert_eq!(asm("S4", ">100"), [0o042400]);
    assert_eq!(asm("S5", ">0"), [0o043500]);
    assert_eq!(asm("S5", "#>100"), [0o043500]);
    assert_eq!(asm("S5", "#<0"), [0o042500]);
    assert!(asm_err("S5", "<101").contains("out of range"));
    // shifts
    assert_eq!(asm("S0", "S3<5"), [0o052305]);
    assert_eq!(asm("S0", "S2<100"), [0o053200]);
    assert_eq!(asm("S0", "S3>5"), [0o053373]);
    assert_eq!(asm("S0", "S1>0"), [0o052100]);
    assert_eq!(asm("S0", "S0<12"), [0o052012]);
    assert_eq!(asm("S7", "S7<3"), [0o054703]);
    assert_eq!(asm("S3", "S3<100"), [0o055300]);
    assert_eq!(asm("S7", "S7>3"), [0o055775]);
    assert_eq!(asm("S3", "S3>0"), [0o054300]);
    assert!(asm_err("S3", "S4<3").contains("no instruction form"));
    assert_eq!(disassemble(&decode(0o042273, None)), "S2        <5");
    assert_eq!(disassemble(&decode(0o042200, None)), "S2        -1");
    assert_eq!(disassemble(&decode(0o043200, None)), "S2        0");
    assert_eq!(disassemble(&decode(0o053200, None)), "S0        S2>100");
    assert_eq!(disassemble(&decode(0o040100, Some(1))), "S1        #-2");
    assert_eq!(disassemble(&decode(0o041100, Some(0))), "S1        #0");
    assert_eq!(
        disassemble(&decode(0o054012, None)),
        "VWD       D'16/O'054012"
    );
}

#[test]
fn special_forms_and_register_designators() {
    assert_eq!(asm("A1", "A2"), [0o030102]);
    assert_eq!(asm("A2", "A3+1"), [0o030230]);
    assert_eq!(asm("A1", "-A2"), [0o031102]);
    assert_eq!(asm("A4", "A5-1"), [0o031450]);
    assert_eq!(asm("S7", "S1"), [0o051701]);
    assert_eq!(asm("S1", "SB"), [0o051100]);
    assert_eq!(asm("S1", "#SB"), [0o047100]);
    assert_eq!(asm("S1", "S2&SB"), [0o044120]);
    assert_eq!(asm("S1", "SB&S2"), [0o044120]);
    assert!(asm_err("S1", "SB&S0").contains("register 0"));
    assert_eq!(asm("S1", "#SB&S2"), [0o045120]);
    assert_eq!(asm("S1", "S2!S1&SB"), [0o050120]);
    assert_eq!(asm("S6", "#S7"), [0o047607]);
    assert_eq!(asm("S6", "S6<A4"), [0o056604]);
    assert_eq!(asm("S6", "S6,S2<1"), [0o056620]);
    assert_eq!(asm("S6", "S6>A4"), [0o057604]);
    assert_eq!(asm("S5", "-S6"), [0o061506]);
    assert_eq!(asm("S5", "+FS6"), [0o062506]);
    assert_eq!(asm("S5", "-FS6"), [0o063506]);
    assert_eq!(asm("S1", "0.6"), [0o071130]);
    assert_eq!(asm("S5", "4."), [0o071570]);
    assert_eq!(asm("VL", "1"), [0o002000]);
    assert_eq!(asm("VM", "0"), [0o003000]);
    assert_eq!(asm("V1", "0"), [0o140100]);
    assert_eq!(asm("V1", "V3"), [0o142103]);
    assert_eq!(asm("V1", "#VM&V3"), [0o146103]);
    assert_eq!(asm("V1", "-V3"), [0o156103]);
    assert_eq!(asm("V1", "+FV3"), [0o170103]);
    assert_eq!(asm("V1", "V2,V2>1"), [0o153120]);
    assert_eq!(asm("V1,A3", "0"), [0o077103]);
    assert_eq!(asm("A1", ",A2"), [0o102100, 0]);
    assert_eq!(asm("A1", "5,"), [0o100100, 5]);
    assert_eq!(asm("A1", "5,0"), [0o100100, 5]);
    assert_eq!(asm("A1", "5,A0"), [0o100100, 5]);
    assert_eq!(asm("-1,A2", "S4"), [0o132477, 0o177777]);
    assert_eq!(asm("17777777,A2", "S4"), [0o132477, 0o177777]);
    assert_eq!(asm("A1", "CI"), [0o033100]);
    assert_eq!(asm("A1", "CA,A2"), [0o033120]);
    assert_eq!(asm("A1", "CE,A2"), [0o033121]);
    assert!(asm_err("A1", "CA,A0").contains("register 0"));
    assert_eq!(asm("CA,A2", "A3"), [0o001023]);
    assert_eq!(asm("CL,A2", "A3"), [0o001123]);
    assert_eq!(asm("CI,A2", ""), [0o001220]);
    assert_eq!(asm("XA", "A2"), [0o001320]);
    assert_eq!(asm("RT", "S2"), [0o001420]);
    assert_eq!(asm("EFI", ""), [0o002100]);
    assert_eq!(asm("DFI", ""), [0o002200]);
    assert_eq!(asm("PASS", ""), [0o001000]);
    assert_eq!(asm("B7,A4", ","), [0o034407]);
    assert_eq!(asm("0,A0", "T22,A5"), [0o037522]);
    assert_eq!(asm("V5", ",,1"), [0o176500]);
    // register designators by symbol
    let mut eval = |text: &str, usage| {
        assert_eq!(usage, ExpUse::Value);
        Ok(ExpValue {
            value: if text == "RTNADDR" { 0o17 } else { 3 },
            forward: false,
        })
    };
    assert_eq!(
        assemble("J", "B.RTNADDR", &mut eval)
            .unwrap()
            .encoding
            .parcels(),
        [0o005017]
    );
    assert_eq!(
        assemble("T.RTNADDR", "S.X", &mut eval)
            .unwrap()
            .encoding
            .parcels(),
        [0o075317]
    );
    assert_eq!(
        assemble("A.X", "A.X+A.X", &mut eval)
            .unwrap()
            .encoding
            .parcels(),
        [0o030333]
    );
    let mut big = |_: &str, _| {
        Ok(ExpValue {
            value: 0o100,
            forward: false,
        })
    };
    assert!(assemble("J", "B.BIG", &mut big)
        .unwrap_err()
        .contains("out of range"));
    // an expression cannot name a register
    assert!(asm_err("A1", "A2+A9").contains("no instruction form"));
    assert!(asm_err("J", "S1").contains("no instruction form"));
}

#[test]
fn expressions_are_evaluated_in_the_right_address_unit() {
    let mut uses = Vec::new();
    let mut eval = |text: &str, usage| {
        uses.push((text.to_string(), usage));
        Ok(ExpValue {
            value: 0o100,
            forward: false,
        })
    };
    assemble("J", "LOOP+2", &mut eval).unwrap();
    assemble("JSZ", "LOOP", &mut eval).unwrap();
    assemble("A1", "TABLE+1,A2", &mut eval).unwrap();
    assemble("TABLE,", "S1", &mut eval).unwrap();
    assemble("A1", "TABLE", &mut eval).unwrap();
    assemble("S1", "<COUNT", &mut eval).unwrap();
    assemble("S1", "'A'R,A2", &mut eval).unwrap();
    let want = [
        ("LOOP+2", ExpUse::Parcel),
        ("LOOP", ExpUse::Parcel),
        ("TABLE+1", ExpUse::Word),
        ("TABLE", ExpUse::Word),
        ("TABLE", ExpUse::Value),
        ("COUNT", ExpUse::Value),
        ("'A'R", ExpUse::Word),
    ];
    assert_eq!(uses.len(), want.len());
    for ((text, usage), (wt, wu)) in uses.iter().zip(want) {
        assert_eq!((text.as_str(), *usage), (wt, wu));
    }
}

#[test]
fn numbers() {
    assert_eq!(eval_number("17"), Some(0o17));
    assert_eq!(eval_number("-17"), Some(-0o17));
    assert_eq!(eval_number("D'64"), Some(64));
    assert_eq!(eval_number("O'100"), Some(64));
    assert_eq!(eval_number("X'ff"), Some(255));
    assert_eq!(eval_number("8"), None);
    assert_eq!(eval_number("LABEL"), None);
    assert_eq!(signed_octal(-8), "-10");
    assert!(is_reserved_name("A1") && is_reserved_name("B77") && is_reserved_name("VM"));
    assert!(!is_reserved_name("A8") && !is_reserved_name("B100") && !is_reserved_name("CON1"));
    assert_eq!(
        quoted_mask("D'12,'A,''B',X"),
        [
            false, false, false, false, false, true, true, true, true, true, true, true, false,
            false
        ]
    );
}

#[test]
fn garbage_operands_never_panic() {
    let pieces = [
        "A1", "S2", "V3", "B17", "T.X", "A.", "S0", "VM", "SB", "RT", "CI", "CA", "J", "EX", "*",
        "+", "-", "/", "#", "<", ">", "&", "!", "\\", ",", ".", "(", ")", "'", "D'", "X'", "A'",
        "W.", "P.", "0", "1", "8", "100", "0.6", "é",
    ];
    let mut state = 0x1975u64;
    let mut next = |bound: usize| {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 33) as usize) % bound
    };
    for _ in 0..20000 {
        let mut fields = [String::new(), String::new()];
        for f in &mut fields {
            for _ in 0..next(5) {
                f.push_str(pieces[next(pieces.len())]);
            }
        }
        if let Ok(a) = assemble_numeric(&fields[0], &fields[1]) {
            // whatever assembles decodes to the instruction it claims to be
            let d = decode(a.encoding.parcel0, a.encoding.parcel1);
            assert_eq!(d.op, a.form.op, "`{}` `{}`", fields[0], fields[1]);
        }
    }
}
