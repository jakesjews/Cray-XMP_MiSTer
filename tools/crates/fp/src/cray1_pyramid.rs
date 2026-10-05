//! Attempted reconstruction of the CRAY-1 multiply pyramid (HRM 2240004 rev C, pages 3-24
//! to 3-27 and figure 3-5). **Experimental: nothing here is used by [`crate::fmul`].**
//!
//! # What the manual determines
//!
//! Figure 3-5 draws the 48 multiplier rows (row 1 is the most significant multiplier bit)
//! against the product columns and a staircase line: "Logical products are formed to the
//! left of this line. No computation is performed to the right of this line." Measured on
//! the 300 dpi scan, the line cuts each row after the column listed in [`ROW_CUT_FIGURE`].
//! With that reading the number of missing logical products in columns 55 to 59 is 3, 11,
//! 16, 21 and 26, exactly the "Max. missing log. prod." row of the table on page 3-24.
//!
//! The same table places the unconditional truncation constant at `2^-51` and `2^-52`
//! (`= 2^-52 x 3 ~ 6.66e-16`) and the round bit of the rounded multiply at `2^-49`.
//!
//! The 6-bit worked examples on pages 3-26 and 3-27 use a staircase of their own and a round
//! bit at `2^-7`. [`toy6`] reproduces all three cases, in both operand orders.
//!
//! # What it leaves open
//!
//! 1. **Columns 60 to 62.** In the figure, rows 31 to 36 reach column 60 and rows 43 to 48
//!    reach column 62. The table says every logical product in columns 60 and beyond is
//!    missing (37, 36, 35, 34), and the manual's own arithmetic for the maximum truncation
//!    (`... + 2^-96`) only works if they are. [`Staircase::Figure`] and [`Staircase::Table`]
//!    are the two readings.
//! 2. **Intermediate truncation.** The table adds an unexplained row "5 3" under columns 58
//!    and 59 to reach its total of `317 x 2^-59`. The missing logical products of the table
//!    account for `304 x 2^-59` only ([`max_truncation`]). The other `13 x 2^-59` are
//!    presumably sum bits dropped inside the adder tree, whose wiring the manual does not
//!    give. A model that sums the logical products exactly, as this one does, would then be
//!    too accurate by up to `13 x 2^-59` (about 0.006 of the last coefficient bit) and would
//!    differ from the machine in the last bit on a fraction of a percent of products.
//! 3. **Rounded multiply.** "A round bit is entered into the pyramid at `2^-49` ... However,
//!    in this case, the product bit `2^-49` is forced to zero." Case 1 of the worked examples
//!    keeps the analogous bit. Which bit is forced, and when, is not determinable.
//! 4. **Half-precision multiply and 067.** The text gives round bits at `2^-31` and `2^-32`
//!    and a 30-bit result whose last bit is zero after a shift, but not where the low 18 bits
//!    are cut relative to the pyramid, and nothing about the complement step of 067.
//! 5. **Operand roles.** The figure does not say whether Sj or Sk is the multiplier. The
//!    X-MP figure labels Sk the multiplier; that is assumed here.
//! 6. **The 6-bit examples** show no truncation constant and their staircase is not the
//!    48-bit one scaled down, so they validate the idea, not the 48-bit wiring.
//!
//! Because of points 1 and 2 the reconstruction is not unique, and `Profile::Cray1` multiply
//! returns the `Profile::Xmp` result.
//!
//! # What the tests show (`tests/pyramid.rs`)
//!
//! * Either reading gives an unrounded product that is exact or one unit high (about 21
//!   percent of random products are one high), as page 3-24 says, and `A x B` differs from
//!   `B x A` for about 1.7 percent of random operand pairs.
//! * It differs from the X-MP product in the last bit for about 21 percent of random
//!   operands, so the fallback to X-MP arithmetic is not a close approximation.
//! * No reference vector validates it: the 24 table products of cray-sim's `fp_test.cpp` are
//!   X-MP results (the staircase is one high on four of them). The one product in that
//!   file's `main()` that the X-MP multiply does not reproduce is reproduced by the
//!   staircase, which is suggestive and no more.

use crate::COEF_MASK;

/// Last product column (weight `2^-column`) whose logical product is formed in each
/// multiplier row, as drawn in figure 3-5. Index 0 is row 1 (multiplier bit of weight
/// `2^-1`, register bit 47). Row `r` holds columns `r+1 ..= r+48` in a full pyramid.
pub const ROW_CUT_FIGURE: [u8; 48] = [
    49, 50, 51, 52, 53, 54, 55, 56, // rows 1-8 are complete
    56, 56, 56, 56, // rows 9-12
    54, 54, 54, // rows 13-15
    55, 55, // rows 16-17
    56, // row 18
    58, 58, 58, 58, 58, 58, // rows 19-24
    55, 55, 55, 55, 55, 55, // rows 25-30
    60, 60, 60, 60, 60, 60, // rows 31-36
    57, 57, 57, 57, 57, 57, // rows 37-42
    62, 62, 62, 62, 62, 62, // rows 43-48
];

/// The two readings of the staircase that the manual supports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Staircase {
    /// Exactly as drawn in figure 3-5.
    Figure,
    /// As the table on page 3-24 counts it: the figure, but with nothing formed to the right
    /// of column 59.
    Table,
}

impl Staircase {
    /// Last column formed in multiplier row `row` (1..=48).
    pub fn row_cut(self, row: u32) -> u32 {
        let cut = u32::from(ROW_CUT_FIGURE[(row - 1) as usize]);
        match self {
            Staircase::Figure => cut,
            Staircase::Table => cut.min(59),
        }
    }

    /// Number of logical products of a full pyramid that are not formed in `column`.
    pub fn missing_in_column(self, column: u32) -> u32 {
        (1..=48u32)
            .filter(|&row| column > row && column <= row + 48 && column > self.row_cut(row))
            .count() as u32
    }

    /// Sum of the logical products that are formed, in units of `2^-62`.
    /// `multiplier` selects the rows; `multiplicand` is shifted along them.
    pub fn partial_product_sum(self, multiplicand: u64, multiplier: u64) -> u64 {
        let wide = (multiplicand & COEF_MASK) << 14; // a_p at bit 62-p
        let mut sum = 0u64;
        for row in 1..=48u32 {
            if (multiplier >> (48 - row)) & 1 != 0 {
                let shifted = wide >> row; // a_p at bit 62-(p+row)
                let drop = 62 - self.row_cut(row);
                sum += (shifted >> drop) << drop;
            }
        }
        sum
    }

    /// Unrounded product coefficient under this reading: the formed logical products plus
    /// the constant at `2^-51` and `2^-52`, summed exactly. Returns the 48-bit coefficient
    /// and whether the one-place normalising left shift was applied.
    pub fn product(self, multiplicand: u64, multiplier: u64) -> (u64, bool) {
        let out = self.partial_product_sum(multiplicand, multiplier) + (3 << 10);
        if (out >> 61) & 1 != 0 {
            ((out >> 14) & COEF_MASK, false)
        } else {
            ((out >> 13) & COEF_MASK, true)
        }
    }
}

/// Largest possible sum of the logical products that are not formed, as an exact fraction
/// `numerator / 2^96`.
pub fn max_truncation(staircase: Staircase) -> u128 {
    (2..=96u32)
        .map(|column| u128::from(staircase.missing_in_column(column)) << (96 - column))
        .sum()
}

/// The manual's stated maximum truncation, `2^-59 x 317 + 2^-96`, as a numerator over `2^96`.
pub const MANUAL_MAX_TRUNCATION: u128 = (317 << 37) + 1;

/// The 6-bit multiplier of the worked examples (pages 3-26 and 3-27).
///
/// `multiplicand` and `multiplier` are 6-bit fractions (bit 5 has weight `2^-1`). Rows for
/// multiplier bits `2^-1`, `2^-2` and `2^-3` are complete; the rows for `2^-4` and `2^-5`
/// stop after column 7 and the row for `2^-6` after column 9. A round bit is added at
/// `2^-7`. Returns the 9-bit pyramid sum before the round bit (as printed in the manual),
/// the 6-bit result coefficient and whether it was shifted left one place.
pub fn toy6(multiplicand: u8, multiplier: u8) -> (u16, u8, bool) {
    const LAST_COLUMN: [u32; 6] = [7, 8, 9, 7, 7, 9];
    let m = u32::from(multiplicand & 0x3F);
    let mut sum = 0u32; // units of 2^-9
    for row in 1..=6u32 {
        if (multiplier >> (6 - row)) & 1 != 0 {
            let shifted = (m << 3) >> row; // a_p at bit 9-(p+row)
            let drop = 9 - LAST_COLUMN[(row - 1) as usize];
            sum += (shifted >> drop) << drop;
        }
    }
    let rounded = sum + (1 << 2); // round bit at 2^-7
    if (rounded >> 8) & 1 != 0 {
        (sum as u16, ((rounded >> 3) & 0x3F) as u8, false)
    } else {
        (sum as u16, ((rounded >> 2) & 0x3F) as u8, true)
    }
}
