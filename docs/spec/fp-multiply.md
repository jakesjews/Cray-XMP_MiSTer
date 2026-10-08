# Floating-point multiply and reciprocal: CRAY-1 versus CRAY X-MP

> A working specification, written while the core was built and kept as it was. Manuals are
> named by their file names on [Bitsavers](http://www.bitsavers.org/pdf/cray/). `cray-sim/` is the
> [cray-sim](https://github.com/andrastantos/cray-sim) project, `system/` its ready-to-run
> COS 1.17; `cos-cray1/` and directories of experiments are working material that is not in
> this repository.
>
> The floating-point library then had two profiles and a module `cray1_pyramid`, an
> experimental reconstruction of the first CRAY-1's multiply pyramid, with tests in
> `tests/pyramid.rs`. They were removed with the CPU's CRAY-1 setting: `fmul` is what this text
> calls `Profile::Xmp`, which `Profile::Cray1` equalled.

Written 2026-10-05. Question: is the CRAY-1's floating-point multiply bit-for-bit the
same as the CRAY X-MP's, and is our reciprocal approximation Cray's? Paths name the
documents as the collection the work was done with has them: `manuals/` are Cray manuals from
Bitsavers and cray-history.net, and `notes/fp-multiply-experiments/` is working material that
is not in this repository. "PDF n" is the page number of the PDF file, not the printed page.

## Conclusions

1. **There were two CRAY-1 multiply units.** Machines from serial 3 up to about 1980 had a
   non-commutative "staircase" pyramid. In May 1980 change packet E-01 to the CRAY-1 manual
   "documents changes to the multiply functional unit that supports symmetrical multiply".
   From then on the CRAY-1 manual, the CRAY-1 S manual and the X-MP manuals describe one and
   the same pyramid, with the same drawing (art number A-0103A in both CRAY-1 manuals).
2. **The later CRAY-1 (the 1980 change, and the CRAY-1 S) is documented as identical to the
   X-MP.** That is what `Profile::Cray1` returns today, so no pyramid change is needed for it.
3. **The original CRAY-1 differs in a defined way.** Revision E of the manual (May 1979)
   corrects the revision C description and is precise enough to implement for 064. The rule
   is in section 3. It reproduces the two error figures the manual itself works out. No
   machine result exists to check it, and its 067 is not documented.
4. **Our 067 (reciprocal iteration) is not the X-MP's.** It follows a guess in cray-sim.
   Cray's own diagnostic simulator (source listing found on the web, section 4) gives a
   different rule, and Kahan's 1990 measurements on real CRAYs agree with Cray's rule and
   not with ours. The fix is one line in the crate and one constant in the RTL (section 7).
5. **064, 065 and 066 in the crate are bit-identical to Cray's diagnostic simulator**, value
   and flag, on 2,000,000 operand pairs each.
6. **The reciprocal approximation is Cray's algorithm.** `frecip` is bit-identical to Cray's
   diagnostic simulator on 6,000,000 operands, the lookup table matches three Cray printings
   of it, and the simulator is organised by the CRAY-1's own module names.
7. The 79 reference vectors in `tests/fp/xmp_ref.vec` are not cray-sim inventions: the 72
   table entries are the "canned answers" of Cray's floating-point diagnostic.

## 1. What each document says about the multiply

### CRAY-1 preliminary reference manual, 1975
`manuals/cray-1/HR-0004-CRAY_1_Hardware_Reference_Manual-PRELIMINARY-1975.OCR.pdf`, PDF 45-46.
Instruction descriptions only, no pyramid. 065: "The low order 24 bits of the result are
cleared." The divide example is 070, 064, 067, 064.

### CRAY-1 hardware reference manual 2240004 revision C, November 1977
`Cray-1-Reference-Manual.pdf` (another scan is `manuals/cray-1/2240004C_...`), PDF 66-70,
printed 3-24 to 3-28. `articles/ed-thelen/CRAY-1-HRM.md` is this text retyped.

- PDF 66: "The pyramid truncates part of the lower bits of the 96-bit product. To adjust for
  this truncation, a constant is unconditionally added above the truncation." Maximum
  truncated amount "approximately 2^-59 x 317 + 2^-96"; product variation "1.16 x 10^-16 to
  6.66 x 10^-16"; "the possibility of rounding up 2^-48".
- PDF 66: "In a full precision rounded multiply, a round bit is entered into the pyramid at
  2^-49 and allowed to propagate up the pyramid. However, in this case, the product bit 2^-49
  is forced to zero."
- PDF 66, table: missing logical products 3, 11, 16, 21, 26 in columns 55 to 59 and every
  product from column 60 on; an unexplained row "5 3" under columns 58 and 59; constant "k k"
  at 2^-51 and 2^-52 (= 3 x 2^-52); round bit R at 2^-49.
- PDF 67, figure 3-5: the pyramid drawn sideways, multiplier rows numbered 1 to 48, a
  staircase line with the legend "Logical products are formed to the left of this line. No
  computation is performed to the right of this line." The crate's measurement of that line
  is `cray1_pyramid::ROW_CUT_FIGURE`. Figure and table disagree from column 60 on.
- PDF 68: "A x B is not necessarily the same as B x A." Half precision: round bits at 2^-32
  and 2^-31, "a 31-bit result (2^-1 to 2^-30) is transmitted back. If the result requires a
  left shift, the bottom bit (2^-30) will be zero."
- PDF 68-69: three 6-bit worked examples. PDF 70: divide sequence, reciprocal "correct to 30
  bits", iteration "47 bits", "18 low-order bits of the half-precision results are returned
  as zeros".

### Revision E, 15 May 1979
`manuals/cray-1/2240004E_Hardware_Reference_197905.pdf`.

- PDF 1, change notice: "Revision E contains rewrites of ... the multiply algorithm ... This
  printing applies to CRAY-1 Computer systems starting with Serial 3. Revision A of this
  publication remains relevant for Serial 1."
- PDF 58 (3-25): "The logical products indicated in figure 3-5 all contribute to the final
  48-bit result. However, the sum bits below 2^-57 are dropped from the accumulation as shown
  in figure 3-5." Worked case, both coefficients all ones, block B: six rows, three reaching
  2^-60 and three reaching 2^-59. "The carry of 4x2^-57 is added to the 57-bit result, but the
  sum of 7x2^-60 is dropped from the accumulation." Part of the pyramid below 2^-60:
  "1/2^96 + 2/2^95 + ... + 36/2^61 = 1/2^96 + 35/2^60".
- PDF 59 (3-26), figure 3-5, redrawn upright. Top edge is "j (multiplicand)" with ticks at
  2^-1, 2^-11, ... 2^-51; the left staircase is "k (multiplier)". Marks: "hh" at 2^-31 and
  2^-32 (note 1, "h = 1 for half precision round"), "f" at 2^-49 (note 2, "f = 1 for full
  precision round"), "11" at 2^-51 and 2^-52 (the constant). The right edge is a staircase
  with three shaded blocks A, B, C and arrows; note 4: "Arrows indicate sum bits that are not
  accumulated into the 57-bit answer." A dashed outline labelled 35 is note 3, "Missing
  carries from the portion of the pyramid below 2^-60." Measured on the 300 dpi scan
  (`notes/fp-multiply-experiments/measure_revE2.py`), the last column formed in each
  multiplier row is:

  | rows | 1-8 | 9-12 | 13-15 | 16-17 | 18 | 19-24 (A) | 25-30 | 31-33 (B) | 34-36 (B) | 37-42 | 43-45 (C) | 46-48 (C) |
  |---|---|---|---|---|---|---|---|---|---|---|---|---|
  | last column | row+48 | 56 | 54 | 55 | 56 | 58 | 55 | 60 | 59 | 57 | 60 | 59 |

  The shaded part of each block is columns 58 and up. This differs from the revision C
  figure in rows 31 to 36 and 43 to 48.
- PDF 60 (3-27): missing logical products "3/2^55 + 11/2^56 + 16/2^57 + 21/2^58 + 26/2^59 +
  31/2^60" (the table above gives exactly these counts); dropped sum bits "0/2^58" for block
  A and "1/2^58 + 1/2^59 + 1/2^60" for each of B and C; "The total error is 154/2^58 +
  1/2^96"; bound for all operands "155/2^58 + 1/2^96"; "(The different case where the first
  operand coefficient is 7 777 777 777 737 777 and the second coefficient is 7 777 777 777
  777 777 gives an error of 154/2^58 + 1/2^82 + 1/2^96.)"; "The value of the constant that is
  unconditionally added into the pyramid is 3/2^52. The possible errors are in the range:
  37/2^58 - 1/2^96 to 192/2^58"; "A x B is not necessarily the same as B x A."
- PDF 61 (3-28): round-bit sentence unchanged from C. Half precision now "a 30-bit result
  (2^-1 to 2^-30) is transmitted back. If the result requires a left shift, the bottom bit
  (2^-30) will be zero." PDF 61-62: the same 6-bit examples. PDF 124 (4-42): 065 "The low
  order 18 bits of the result are cleared."

### CRAY-1 block diagrams, December 1979
`manuals/cray-1/224-0982-PR1-CRAY_1_Computer_Block_Diagrams-December_1979.pdf`, PDF 12-13,
"FLOATING MULTIPLY #1" and "#2", dated 11-28-79. Module level, no gates. It is the original
unit: one "ROUND" line into the second-level module for sum bits 2^47 to 2^53 (2^-49), two
"STRONG ROUND" lines into the module holding 2^-31 and 2^-32, "FORCE 1" into the module for
2^41 to 2^46 (the constant at 2^-51, 2^-52). First-level modules take 12 multiplier bits each
(rows 1-12, 13-24, 25-36, 37-48) and their lowest multiplicand bits agree with the revision E
staircase (bit 2^8, plus an end module from 2^7, for rows 13-24; 2^18 for rows 25-36; 2^28
for rows 37-48). Second level includes sum bit 2^39 (2^-57); third level and later cover 2^40
to 2^95. "-1 ENTRY" lines into levels 2 to 4 and "RECIPROCAL CONTROL", "STRONG ROUND",
"WEAK ROUND", "INTEGER MULT" into the last level: 067 was done by entering ones and
complementing at the output, as later.

### Change packet E-01, May 1980, as printed in revision F (HR-0004 F, May 1982)
`manuals/cray-1/HR-0004F-...May_1982.OCR.pdf` (page numbers are one higher in the other copy).

- PDF 3-4, record of revision: "E May 15, 1979 - ... This printing corrects the description
  of the multiply algorithm". "E-01 May, 1980 - This change packet documents changes to the
  multiply functional unit that supports symmetrical multiply". "F May, 1982 - ...
  incorporates revision E with E-01 ... No other changes have been made."
- PDF 63 (3-25, marked E-01): "This averages to nine carries which are injected at the 2^-56
  position. The errors due to this truncation and rounding are in the range: -0.23 x 2^-48 to
  +0.57 x 2^-48".
- PDF 65 (page "3-26 through 3-29", E-01): new figure, art number A-0103A. Straight right
  edge at 2^-56; "1001" at 2^-53 to 2^-56 (note 3, "Truncation constant"); "ff" at 2^-50 and
  2^-51; "hh" at 2^-31 and 2^-32. "The multiplication is commutative, that is, A times B
  equals B times A. In a full-precision rounded multiply, 2 round bits are entered into the
  pyramid at bit position 2^-50 and 2^-51". Half precision: "a 30-bit result (2^-1 to 2^-30)
  is transmitted back." The 6-bit examples are gone.
- Pages 3-30 and 4-42 (PDF 66, 128) are still revision E pages: "18 low-order bits".

### CRAY-1 S series manual HR-0808, November 1981
`manuals/cray-1/HR-0808_CRAY-1S_HWref_Nov81.pdf`, PDF 104-106 (Part 2, 4-23 and 4-24); text
in `manual-ocr/cray-1/HR-0808_CRAY-1S_HWref_Nov81.md` lines 1773-1793. Same sentences and the
same drawing A-0103A as E-01. Differences: half precision "a 29-bit result (2^-1 to 2^-29)",
065 "The low-order 19 bits of the result are cleared", "Where 29 bits of accuracy is
sufficient". Range rule: "The out-of-range conditions are tested before normalizing."

### X-MP manuals
`cray-1x/docs/hw/HR-0097B_...Aug86.pdf` PDF 100-102 (4-28 to 4-30), figure 4-10 on PDF 101;
`HR-0032-...July_1984.OCR.pdf` PDF 107-108; `CSM-0111000-B` PDF 101; HR-0032 of November
1982 in `manual-ocr/x-mp/`. All the same text.

- "The average value of this truncation is 9.25 x 2^-56 ... Nine carries are injected at the
  2^-56 position ... results range from one too large to one too small in the 2^-48 bit
  position with approximately 99 percent of the values having zero deviation ... The
  multiplication is commutative".
- "The rounding method used adds a constant so that it is 50 percent high (0.25 x 2^-48;
  high) 38 percent of the time and 25 percent low (0.125 x 2^-48; low) 62 percent of the
  time ... 2 round bits are entered into the pyramid at bit position 2^-50 and 2^-51".
- "the 29 most significant bits of the normalized result are transmitted back."
- Same range "-0.23 x 2^-48 to +0.57 x 2^-48" as E-01 and HR-0808.
- Figure 4-10 is A-0103A redrawn with two scales ("If Shift is Needed" ending at 2^-55, "If
  Shift is not Needed" ending at 2^-56), "hh = 11 for half-precision round", "ff = 11 for
  full-precision round", "Truncation compensation constant, 1001 used for all multiplies".
- PDF 96-98 (4-24 to 4-26): exponent matrix (zones 1 to 7), the rules `fmul_model`
  implements.

### X-MP hardware training, HTV-0242, 1988
`manuals/x-mp/HTV-0242-010-CRAY_XMP2_Hardware_Training_Volume_2-1988_MERGE.pdf`. Not looked
at when the crate was written. PDF 177-200 are the multiply unit.

- PDF 183 and 185, "PYRAMID GENERATION" (D-1367C): every logical product as a numbered
  cell, K rows 2^0 to 2^47 against sum bits 2^95 to 2^40. The right edge is straight at sum
  bit 2^40 (2^-56): every product of that weight or more is formed, nothing below.
- PDF 184 and 186, "PYRAMID MODULE LOCATION" (D-1365): "TRUNCATION CONSTANT" marks at result
  positions 2^-5 and 2^-8 (sum bits 2^43, 2^40: 1001); "STRONG ROUND CONSTANT" at result bits
  2^17 and 2^16 (2^-31, 2^-32); "WEAK ROUND CONSTANT" two marks drawn half a column off
  (between 2^-49 and 2^-51); "ITERATION CONSTANT" 47 marks under result bits 2^46 to 2^0.
- PDF 193/195 data flow: "-1 CONSTANT (47)", "STRONG ROUND (2)", "WEAK ROUND (2)", "STRONG
  ROUND TRUNCATION" into the last module.
- PDF 196 (25-16): "R32 = H22 ADD 2 ROUND BITS INTO THE SUMMATION" for 065; "R35 =
  TRUNCATION THE LOWER 19 BITS OF THE RESULTS FOR A 065 INSTRUCTION".
- PDF 198 (25-17): 067 "MAKE GO MINUS ONE FOR 067 INSTRUCTION. 48 1-BITS ARE ADDED IN THE
  SUMMATION." "R34 = COMPLEMENT THE RESULTS ON THE OUTPUT". 066 "ADD 2 ROUND BITS".
- PDF 199-200: integer multiply, range error and underflow terms.
- One drawing (D-1367C) puts the weak round marks at 2^-49 and 2^-50. The manuals, the
  manual's own statistics, and Cray's simulator all say 2^-50 and 2^-51.

`manuals/y-mp/HTV-0834-H-...Volume_2-September_1989a.pdf` (added), PDF 372-381, says the
same for the Y-MP: strong round bits at 2^17 and 2^16, "truncate the lower 19 bits", two
weak round bits, 067 adds a 1 "to each bit position (2^46 - 2^0) ... effective 48 '1' bits"
and "Complement ... toggles its final result".

### Others
- `manuals/papers/an_analysis_of_the_cray1_computer.PDF` (1978) PDF 4: "Multiplication is not
  commutative, because part of the partial sum pyramid is not implemented ... A*B and B*A may
  differ in the last bit".
- Kahan 1990 (section 5) p. 29: "The abbreviation is now done in a commutative way (X*Y =
  Y*X) though long ago this was not so."
- `manuals/ios/HM-1002-...June_1984.OCR.pdf` PDF 173-177: one diagnostic list for "CRAY-1
  Model A, B, C, S and M", containing SFM "Simulate floating-point multiply", SMU "Symmetric
  multiply" and SSFR "Simulate floating-point reciprocal".
- `manuals/papers/DTIC_ADA060638_A_CRAY-1_Simulator_1978.pdf` PDF 17: the Michigan
  simulator used IBM 370 arithmetic. No use here.
- Fenton's Verilog (`cray-1x/Verilog/xmp/fast_float_mult.v`) takes the top of an exact 96-bit
  product; the HASE model does an integer multiply. Neither is bit-accurate.

## 2. How the revisions fit together

| | original CRAY-1 (rev C, E; diagrams 1979) | symmetric (E-01 1980, HR-0808, X-MP) |
|---|---|---|
| products formed | staircase, up to column 60 | all of columns 2 to 56 |
| sums dropped | three blocks lose their sum bits below 2^-57 | none |
| constant | 2^-51 + 2^-52 | 2^-53 + 2^-56 (nine at 2^-56) |
| 066 | one bit at 2^-49; product bit 2^-49 forced to zero | 2^-50 + 2^-51 |
| 065 round bits | 2^-31, 2^-32 | 2^-31, 2^-32 |
| 065 result | 30 bits before the shift | 29 bits after the shift (E-01 still says 30) |
| commutative | no | yes |
| exponent rules | same text | same text, plus "tested before normalizing" |

Revision C and E describe the same hardware; E is the correction. E-01 is a hardware change.
No document found says which serial numbers were built with it or whether older machines
were refitted. The 1984 diagnostic list gives one SFM for all CRAY-1 models, and SFM's
simulator (its change log starts "0980") only knows the symmetric pyramid, which suggests
the field was symmetric by then.

## 3. The original CRAY-1 rule, in the terms of `mul.rs`

Row r (1 to 48) is multiplier bit Sk 2^-r; multiplicand bit Sj 2^-p; the product sits in
column p+r. With `cut(r)` from the table in section 1:

1. For each group of six rows (1-6, 7-12, ... 43-48) sum the logical products with
   p + r <= cut(r), exactly.
2. Drop the bits of each group sum below 2^-57 (only groups 19-24, 31-36 and 43-48 have any).
3. Add the group sums and the constant 2^-51 + 2^-52. The sum has bits 2^-1 to 2^-57.
4. 064: if bit 2^-1 is set take bits 2^-1 to 2^-48, else bits 2^-2 to 2^-49 and exponent - 1.
5. 066: also add 2^-49, then force bit 2^-49 of the sum to zero before step 4 (so a shifted
   result has a zero last bit). This reading matches the wording for 065; the 6-bit example
   "case 1" does not force the bit, so it is not certain.
6. 065: also add 2^-31 + 2^-32, keep bits 2^-1 to 2^-30 of the sum, then step 4.
7. 067: not documented for this unit.

`notes/fp-multiply-experiments/src/bin/reve.rs` implements 1 to 5. Results:

- all ones x all ones: full product minus pyramid = 154/2^58 + 1/2^96, as the manual says;
- Sj = 7777777777737777, Sk all ones: 154/2^58 + 1/2^82 + 1/2^96, as the manual says. With
  the operands swapped it is 152/2^58 + ..., so the "first operand" is Sj, the multiplicand;
- grouping rows by 3 gives 156/2^58 for all ones, and one truncation of the whole sum gives
  152/2^58, so the per-block truncation is what the manual means;
- random normalised operands: the 064 product is the chopped exact product or one above it
  (20.7 percent one above), never below; Sj*Sk differs from Sk*Sj for 1.7 percent of pairs;
  it differs from the X-MP 064 product for 20.9 percent and from the crate's revision C
  reconstruction for 0.25 percent. 066 differs from the X-MP 066 for 36 percent.

## 4. Cray's own simulator: the symmetric rule exactly

Found on the web and added to `manuals/diagnostics/`: assembler listings of the CRAY J90
offline diagnostics, dated 12/06/96, "Cray Research Proprietary". The J90 is the CMOS
machine compatible with the Y-MP; the programs are much older (SFM change log "0980 - ADDED
PREAMBLE", "1082"; FPT history from 1988; SFR "06/28/85").

- `...Listing-jsfm-January_1997.pdf`, "SFM - SIMULATE FLOATING MULTIPLY". PDF 6-7, routine
  FMS: accumulator starts at 11 octal, J coefficient placed with 7 guard bits and shifted
  right one place per multiplier bit with bits falling off the end, "TRUNCATE RESULT" by 7.
  That is the pyramid cut after 2^-56 with nine added. PDF 11, "STRONG/WEAK/2MINUS TEST":
  three fixed answers (the 067 one is no longer in the listing).
- `...Listing-jfpt-January_1997.pdf`, "JFPT - J90 Floating Point Test". PDF 5: "KK = 11
  TRUNCATION CONSTANT", "R = 60000 ROUND CONSTANT", "LL = 30000 ITERATION CONSTANT" (octal).
  PDF 69-71, subroutine SMLT, simulates 064 to 067. PDF 71-75, subroutine SRP, simulates
  070, with the lookup table "A LIST OF CONSTANTS FOR A0,A0**2". PDF 90-92: canned operands
  and expected sums, products and reciprocals.
- `...Listing-jsfr-January_1997.pdf`, "SFR - SIMULATE FLOATING RECIPROCAL".

SMLT, with the product sum P in a register whose bit 63 is 2^-1:

- all four: P = 9 x 2^-56 + sum over multiplier bits of the multiplicand shifted, each row
  cut at 2^-56;
- 065: add 2^-31 + 2^-32; if bit 2^-1 is then set, or both exponents are zero, keep 29 bits
  (2^-1 to 2^-29), else keep 30 bits (2^-1 to 2^-30);
- 066: add 2^-50 + 2^-51 ("R");
- **067: add 47 one-bits at 2^-2 to 2^-48 and the iteration constant 2^-51 + 2^-52 ("LL"),
  then complement bits 2^-2 to 2^-49. Bit 2^-1 is not complemented.**
- exponent: one zero exponent gives zero; both zero is the integer multiply (48 bits from
  2^-1, no shift); E = ej + ek - 040000; E < 020000 gives zero; E, ej or ek >= 060000 gives
  exponent 060000 and the error flag; otherwise bit 2^-1 set keeps E, clear shifts one place
  and gives E - 1.

Equivalent form of 067: the result bits are those of (198 - P) mod 2^56 with P in units of
2^-56. The crate computes (9 - P).

Checks (`notes/fp-multiply-experiments/src/bin/smlt.rs`, a transcription of SMLT):

- 064, 065, 066: crate and SMLT identical, value and flag, on 2,000,000 pairs each (random
  words, in-range, unnormalised, reciprocal pairs).
- 067: identical for 23 percent of that mix. On (frecip(b), b) pairs Cray's result is one
  unit higher in 70.2 percent of cases and two higher in 8.0 percent.
- The three SFM fixed answers: 065 `400100000001FFFF * 4001800000000000 = 4001000000000000`;
  065 `400100000003FFFF * 4001800000000000 = 4001000000080000`; 066
  `4001000000000001 * 4001A00000000000 = 4001000000000002`. Crate: all three. The two 065
  answers together admit only round bits at 2^-31 and 2^-32 and a 29-bit result: bits one
  place higher or a 30-bit result fail the first, cray-sim's bits (2^-32, 2^-33) or 28 bits
  fail the second. The 066 answer fails with round bits one place lower (`swt.rs`).
- The 24 products of `xmp_ref.vec` are JFPT's table ERFM (operands COPA, COPB); the sums are
  ERFA and the reciprocals ERRP.

## 5. Numeric examples from real machines

W. Kahan, "How CRAY's Arithmetic Hurts Scientific Computation", Cray User Group, June 1990
(`manuals/papers/Kahan_..._CUG_Jun90.pdf`, copied from `articles/cray-history/`), PDF 29-31.
`notes/fp-multiply-experiments/src/bin/kahan.rs`.

| Kahan (real CRAYs of 1990) | crate today | with Cray's 067 |
|---|---|---|
| *F: A=4032800000000000, B=4030FFFFFFFFFFFF, C=402FFFFFFFFFFFFE: [B*B]-[A*C] = -2^48 | -2^48 | same |
| *R: A=40308000095892E6, B=4030B504FFFFFFFF, C=4031800008C06C75: [B*B]-[A*C] = -2^48 | -2^48 | same |
| 21.0/3.0 above 7 with two *R, below 7 otherwise | 7+1 unit; 7-1 unit | same |
| (62.0*63.0)/63.0 and (63.0*63.0)/63.0 are not integers | true | same |
| [X/X] < 1 "at half of randomly chosen values X" with *F multiplies | 99.4 percent | 49.7 percent |
| *F then *R: [X/X] < 1 "at about one X in 6", > 1 "at one in 33" | 83.2, 0.0 percent | 16.6, 3.2 percent |
| division errors "almost as big as 2.5 ulps by random testing" (Kiernan, CRI) | up to 3.97 | up to 2.38 |
| *F errs "never by more than 1.23 ulps", *R "by under 0.83 ulp" | consistent | same |

[Y/X] is Y*(R*C) with R = 070 of X and C = 067 of R and X. For 21.0/3.0 the model gives
7 - 1 unit with *F,*F and with *F then *R, 7 + 1 unit with two *R, and exactly 7 with *R
then *F, a combination Kahan may not have meant by "otherwise". His printed digits
(7.00000000000003553) are not a 48-bit number, so only the direction can be compared. The
(62*63)/63 line was run with all *F and with all *R. The 1.23 and 0.83 bounds equal
1 + 30/128 and 0.75 + 9/128, which is what the constants give.
The first *F example gives 0, not -2^48, on the original CRAY-1 pyramid, and the *R example
gives 0 with *F, as Kahan's text implies. Scanning the 067 constant k in "(k - P)" against
the three [X/X] statistics gives k = 197 +/- 2 (`scank2.rs`); Cray's listing gives 198; the
crate has 9. This was found from Kahan's numbers before the listing was read.

Manual figures reproduced by the revision E model are in section 3. No operand and result
pair from an original (pre-1980) CRAY-1 was found anywhere.

Effect on the divide sequence (`divstat.rs`, 2,000,000 pairs, error in units of the last
place of the quotient):

| | crate today | with Cray's 067 |
|---|---|---|
| `fdiv`: (2-rb) *F (a *F r) | -3.74 to +0.19, mean -1.52 | -2.29 to +1.60, mean -0.46 |
| X-MP manual order: ((2-rb) *F r) *R a | -3.87 to +0.92, mean -1.01 | -2.38 to +2.38, mean +0.05 |
| full reciprocal (2-rb) *F r | -2.84 to +0.19 | -1.38 to +1.64, mean +0.01 |
| `fdiv(b, b) != 1.0` | 93.5 percent | 25.3 percent |

The "3.7 units low" in the crate documentation is an artefact of the wrong 067.

## 6. Reciprocal approximation

- **Cray documents the structure for the CRAY-1.** Block diagrams, PDF 14-16 ("RECIPROCAL
  A1" 12-03-79, "RECIPROCAL A1^2" 12-11-79, "RECIPROCAL RESULT" 12-10-79): A0 and A0^2
  looked up from operand bits B0 2^-1 to 2^-7; "FORM A1 MATRIX" with B1 2^0 to 2^-23; A1
  result; A1^2 2^-1 to 2^-36; "A2 MATRIX" on ten RM modules and two RN modules with B2 2^-1
  to 2^-36, "FORCED 0 2^-35"; result bits 2^15 to 2^46. Handwritten on PDF 14: "The input
  operand coefficient is treated as if it were left shifted one bit position ... designated
  to be 2^0 through 2^-36. The lower 11 bits of the operand are not used and discarded. The
  uppermost bit (2^0) is for the most part assumed a one".
- **The exact iteration is Cray's diagnostic simulator.** JFPT subroutine SRP (PDF 71-75) and
  the SFR listing. Comments in SFR (PDF 13) name the hardware it mirrors: "ITERATION FOR
  RM1,RM8 & RN2", "RM2 & RM9", "RM3 & RM10", "RM4,RM5 & RN1", "RM6". Those are the CRAY-1's
  A2 matrix modules in the 1979 diagram; the X-MP's modules are named 3RA to 3RI. SFR's
  history has "06/28/85 Force zero on bits -34 and -35 of A1^2" and "30Sep85 Removes
  references to SSFR", SSFR being the CRAY-1 test name in the 1984 list. cray-sim's routine
  is a translation of this program.
- **The exact table is printed three times by Cray**: JFPT table APRX (PDF 75 on), HTV-0242
  PDF 139-141 "Reciprocal table look-up values" (B, A0, A0^2, -2A0 in octal), and the Y-MP
  overview HTV-0834 in `articles/cray-history/cray-floating-point-numbers_files/`. All 128
  entries of each equal `recip_seed(i) = round(2^15 / (128.5 + i))` and its exact square.
- HTV-0242 PDF 142 (A1 pyramid, C-1314E): all products down to 2^-24; the A0^2 row for 2^-15
  is empty (a square has that bit zero); "2A0 is added into the pyramid ... for rounding
  purposes"; "RETAIN 2^-1 - 2^-18 ... CARRY IS LOST". PDF 148 (A2 pyramid, C-1315D): the
  number of cells in each A1^2 row equals `SECOND_MULTIPLY` in `recip.rs` row for row, with
  "FORCE 0" at 2^-34 and 2^-35. PDF 129: "The coefficient of A2 is 33 bits long of which
  only the upper 30 bits are accurate."
- `srp.rs` (a transcription of SRP): identical to `frecip`, value and flag, on 6,000,000
  operands (random words, every exponent, normalised) and on all 128 table indices; it
  reproduces the 25 reciprocal vectors.
- Manuals: CRAY-1 revisions C to F "correct to 30 bits", range error for exponent <= 020001
  or >= 060000; the X-MP manual counts the lookup as the first of three iterations. No
  manual gives the table.

The same unit, by these documents, in the CRAY-1 of 1979, the X-MP, the Y-MP and the J90.
Nothing indicates the reciprocal unit changed with the 1980 multiply change.

## 7. Recommended changes

1. `mul.rs`, `MulKind::TwoMinus` for `XMP_MANUAL`: replace `comp.wrapping_sub(sum)` by
   Cray's rule. With `u(n) = 1 << (last_column - n)`:
   `(sum + comp + u(51) + u(52) + (((1 << 47) - 1) << (last_column - 48))) ^ (((1 << 48) - 1) << (last_column - 49))`.
   The tail of `fmul_model` is unchanged. `prop067.rs` checks exactly this against SMLT:
   identical on 8,000,000 pairs. Keep cray-sim's rule only inside `MulModel::CRAY_SIM` (a
   model field is needed to select it).
2. `rtl/cray/fp_mul.v`: `K_2M: konst = 56'd11` becomes `56'd200` (the unit then forms
   198 - P; only `s4_out[55:7]` is used). Regenerate the `mul2m` vectors.
3. Documentation in the crate: the reference vectors are Cray's canned answers; 064, 065,
   066, the range rules and the reciprocal are confirmed by Cray's simulator; `fdiv` is
   -2.29 to +1.60 units; `Profile::Cray1` means the CRAY-1 with the 1980 symmetric multiply.
4. New tests: the three SFM answers, Kahan's two examples, 067 vectors, for example
   (r, x, 2 - r*x): `3FFC90395FED8000 4005E333B266F103 40018000000067D9`,
   `3FFFB81CF90C0000 4002B1FA3C80DB06 40018000000007E8`,
   `3FFCB41C73E80000 4005B5EE9E9E2FA4 4001800000005BB3`,
   `3FFF90FE571B0000 4002E1FF0E4AE394 4001800000002DED`.
5. Optional: a `Profile::Cray1Original` from section 3 for 064, 066 and 065. Not
   recommended as the default: unverifiable, and its 067 is unknown.
6. Follow-up outside this question: JFPT also has SFA, "simulate floating add", and the
   add-unit points the crate lists as unvalidated could be settled the same way.

## 8. What remains unverifiable

- Any result of an original (pre-1980) CRAY-1. The section 3 rule rests on revision E alone.
  Its 066 forced bit and its 067 are uncertain.
- Which CRAY-1 serial numbers had the symmetric unit, and whether it was retrofitted.
- Whether the symmetric CRAY-1 of 1980-82 had exactly the X-MP's 065 width and 067
  constants. E-01 kept "30-bit result"; HR-0808 a year later says 29 bits and 19 cleared.
  The 067 rule is attested for the X-MP and Y-MP generation only (J90 listing, Kahan's
  statistics, the 47 "-1 constant" lines of HTV-0242). No CRAY-1 document gives it.
- The listing is the J90 edition of the diagnostics. That the X-MP edition had the same
  constants is inferred from the program histories and from Kahan's measurements.

## 9. Documents added

- `manuals/diagnostics/CRAY_J90_Series_Offline_Diagnostic_Listing-jsfm-January_1997.pdf`
- `manuals/diagnostics/CRAY_J90_Series_Offline_Diagnostic_Listing-jsfr-January_1997.pdf`
- `manuals/diagnostics/CRAY_J90_Series_Offline_Diagnostic_Listing-jfpt-January_1997.pdf`
  (all three from cray.modularcircuits.com/cray_docs/hw/ymp/test_code/)
- `manuals/y-mp/HTV-0834-H-CRAY_YMP_Hardware_Training_Volume_2-September_1989a.pdf`
  (cray.modularcircuits.com/cray_docs/hw/ymp/)
- `manuals/papers/Kahan_How_CRAYs_Arithmetic_Hurts_Scientific_Computation_CUG_Jun90.pdf`
  (a copy of the file already under `articles/cray-history/`)

Experiments: `notes/fp-multiply-experiments/` (a Cargo project that depends on the crate by
path; `cargo run --release --bin smlt`, `srp`, `prop067`, `kahan`, `scank2`, `divstat`,
`reve`). `smlt.rs` and `srp.rs` are transcriptions of Cray's listings, not of cray-sim. The
Python scripts need the page images (`pdfimages -png -f 59 -l 59` of revision E, `pdftoppm
-r 300 -f 186 -l 186` of HTV-0242).
