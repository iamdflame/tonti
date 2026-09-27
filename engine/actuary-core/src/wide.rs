//! 256-bit intermediate arithmetic for 128-bit fixed point, without external crates.

// Stylus runs wasm32, where the widest native multiply is 64 × 64 → 64 bits and a u128 multiply
// or divide is a library call through memory. So the wide arithmetic works in 32-bit limbs: every
// partial product and every trial quotient is one native 64-bit instruction. Measured with
// `ink-meter`, this is what keeps a many-path quote and a settlement page inside a block.

const M32: u64 = u32::MAX as u64;

const fn limbs(x: u128) -> [u64; 4] {
    [x as u64 & M32, (x >> 32) as u64 & M32, (x >> 64) as u64 & M32, (x >> 96) as u64]
}

const fn join(l0: u64, l1: u64, l2: u64, l3: u64) -> u128 {
    l0 as u128 | (l1 as u128) << 32 | (l2 as u128) << 64 | (l3 as u128) << 96
}

/// Full 128×128 → 256-bit product, as `(hi, lo)`. Schoolbook over 32-bit limbs, unrolled into
/// locals and branch-free: Stylus charges 2,450 ink for every basic block entered, more than the
/// sixteen multiplies, so skipping zero limbs costs more than it saves. Each step
/// `a·b + r + carry` is at most (2³²−1)² + 2(2³²−1) = 2⁶⁴ − 1, so it never overflows.
#[inline(always)]
pub const fn mul_wide(a: u128, b: u128) -> (u128, u128) {
    let (a0, a1, a2, a3) = (a as u64 & M32, (a >> 32) as u64 & M32, (a >> 64) as u64 & M32, (a >> 96) as u64);
    let (b0, b1, b2, b3) = (b as u64 & M32, (b >> 32) as u64 & M32, (b >> 64) as u64 & M32, (b >> 96) as u64);
    let (mut r0, mut r1, mut r2, mut r3, mut r4, mut r5, mut r6, mut r7) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    // Row i adds a_i·b into r[i..i+4] and sets r[i+4] (which no earlier row has written).
    macro_rules! row {
        ($ai:expr, $x0:ident, $x1:ident, $x2:ident, $x3:ident, $x4:ident) => {
            let t = $ai * b0 + $x0;
            $x0 = t & M32;
            let t = $ai * b1 + $x1 + (t >> 32);
            $x1 = t & M32;
            let t = $ai * b2 + $x2 + (t >> 32);
            $x2 = t & M32;
            let t = $ai * b3 + $x3 + (t >> 32);
            $x3 = t & M32;
            $x4 = t >> 32;
        };
    }
    row!(a0, r0, r1, r2, r3, r4);
    row!(a1, r1, r2, r3, r4, r5);
    row!(a2, r2, r3, r4, r5, r6);
    row!(a3, r3, r4, r5, r6, r7);
    (join(r4, r5, r6, r7), join(r0, r1, r2, r3))
}

/// `floor(x / d)` for a divisor below 2³²: four native 64-bit divisions.
pub const fn div_small(x: u128, d: u64) -> u128 {
    let (l0, l1, l2, l3) = (x as u64 & M32, (x >> 32) as u64 & M32, (x >> 64) as u64 & M32, (x >> 96) as u64);
    let q3 = l3 / d;
    let t = ((l3 - q3 * d) << 32) | l2;
    let q2 = t / d;
    let t = ((t - q2 * d) << 32) | l1;
    let q1 = t / d;
    let t = ((t - q1 * d) << 32) | l0;
    join(t / d, q1, q2, q3)
}

/// `floor((hi·2^128 + lo) / d)`. Returns `None` if the quotient doesn't fit in 128 bits
/// (`hi >= d`) or `d` is zero. Knuth's algorithm D in base 2³² (Hacker's Delight `divmnu`).
pub const fn div_wide(hi: u128, lo: u128, d: u128) -> Option<u128> {
    if d == 0 || hi >= d {
        return None;
    }
    if hi == 0 && d <= M32 as u128 {
        return Some(div_small(lo, d as u64));
    }
    let (h, l, v) = (limbs(hi), limbs(lo), limbs(d));
    let u = [l[0], l[1], l[2], l[3], h[0], h[1], h[2], h[3]];
    // Significant limbs of the divisor and the dividend.
    let mut n = 4;
    while v[n - 1] == 0 {
        n -= 1;
    }
    let mut m = 8;
    while m > 0 && u[m - 1] == 0 {
        m -= 1;
    }
    if m < n {
        return Some(0);
    }
    let mut q = [0u64; 8];
    if n == 1 {
        // Short division by one limb.
        let mut k = 0u64;
        let mut j = m;
        while j > 0 {
            j -= 1;
            let t = (k << 32) | u[j];
            q[j] = t / v[0];
            k = t - q[j] * v[0];
        }
    } else {
        // Normalize so the divisor's top limb has its high bit set.
        let s = (v[n - 1] as u32).leading_zeros();
        let mut vn = [0u64; 4];
        let mut i = n - 1;
        while i > 0 {
            vn[i] = ((v[i] << s) | if s == 0 { 0 } else { v[i - 1] >> (32 - s) }) & M32;
            i -= 1;
        }
        vn[0] = (v[0] << s) & M32;
        let mut un = [0u64; 9];
        un[m] = if s == 0 { 0 } else { u[m - 1] >> (32 - s) };
        let mut i = m - 1;
        while i > 0 {
            un[i] = ((u[i] << s) | if s == 0 { 0 } else { u[i - 1] >> (32 - s) }) & M32;
            i -= 1;
        }
        un[0] = (u[0] << s) & M32;
        let mut j = m - n + 1;
        while j > 0 {
            j -= 1;
            // Estimate the quotient limb from the top two limbs, then correct it (at most twice).
            let num = (un[j + n] << 32) | un[j + n - 1];
            let mut qhat = num / vn[n - 1];
            let mut rhat = num - qhat * vn[n - 1];
            while qhat > M32 || qhat * vn[n - 2] > ((rhat << 32) | un[j + n - 2]) {
                qhat -= 1;
                rhat += vn[n - 1];
                if rhat > M32 {
                    break;
                }
            }
            // Multiply and subtract.
            let mut borrow: i64 = 0;
            let mut i = 0;
            while i < n {
                let p = qhat * vn[i];
                let t = un[i + j] as i64 - borrow - (p & M32) as i64;
                un[i + j] = t as u64 & M32;
                borrow = (p >> 32) as i64 - (t >> 32);
                i += 1;
            }
            let t = un[j + n] as i64 - borrow;
            un[j + n] = t as u64 & M32;
            q[j] = qhat;
            if t < 0 {
                // Overshot by one: add the divisor back.
                q[j] -= 1;
                let mut carry = 0u64;
                let mut i = 0;
                while i < n {
                    let t = un[i + j] + vn[i] + carry;
                    un[i + j] = t & M32;
                    carry = t >> 32;
                    i += 1;
                }
                un[j + n] = (un[j + n] + carry) & M32;
            }
        }
    }
    // hi < d guarantees the quotient fits in the low four limbs.
    Some(join(q[0], q[1], q[2], q[3]))
}

/// `floor(a·b / d)` with a 256-bit intermediate.
pub const fn mul_div(a: u128, b: u128, d: u128) -> Option<u128> {
    let (hi, lo) = mul_wide(a, b);
    div_wide(hi, lo, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_wide_matches_known_products() {
        assert_eq!(mul_wide(u128::MAX, u128::MAX), (u128::MAX - 1, 1));
        assert_eq!(mul_wide(1 << 64, 1 << 64), (1, 0));
        assert_eq!(mul_wide(3, 5), (0, 15));
    }

    /// The original u128 implementations, kept as the reference the limb versions must equal.
    mod reference {
        const M64: u128 = u64::MAX as u128;
        pub fn mul_wide(a: u128, b: u128) -> (u128, u128) {
            let (a1, a0) = (a >> 64, a & M64);
            let (b1, b0) = (b >> 64, b & M64);
            let (p00, p01, p10, p11) = (a0 * b0, a0 * b1, a1 * b0, a1 * b1);
            let mid = (p00 >> 64) + (p01 & M64) + (p10 & M64);
            ((p11 + (p01 >> 64) + (p10 >> 64) + (mid >> 64)), (p00 & M64) | (mid << 64))
        }
        pub fn div_wide(hi: u128, lo: u128, d: u128) -> Option<u128> {
            if d == 0 || hi >= d {
                return None;
            }
            if hi == 0 {
                return Some(lo / d);
            }
            let (mut rem, mut q) = (hi, 0u128);
            for i in (0..128).rev() {
                let carry = rem >> 127;
                rem = (rem << 1) | ((lo >> i) & 1);
                q <<= 1;
                if carry == 1 || rem >= d {
                    rem = rem.wrapping_sub(d);
                    q |= 1;
                }
            }
            Some(q)
        }
    }

    /// SplitMix64, and operands with random bit lengths (small, 64-ish, full width), because the
    /// limb algorithms have separate paths for every number of significant limbs.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }
        fn wide(&mut self) -> u128 {
            let x = ((self.next() as u128) << 64) | self.next() as u128;
            let bits = self.next() % 129;
            let x = if bits == 128 { x } else { x & ((1u128 << bits) - 1) };
            // Sometimes a run of ones or zeros, which exercises normalisation and the add-back.
            match self.next() % 8 {
                0 => x | (u128::MAX >> (self.next() % 128)),
                1 => x & !(u128::MAX >> (self.next() % 128)),
                _ => x,
            }
        }
    }

    #[test]
    fn limb_arithmetic_equals_the_u128_reference_on_two_million_inputs() {
        let mut rng = Rng(0x7071_4663);
        for _ in 0..2_000_000 {
            let (a, b) = (rng.wide(), rng.wide());
            assert_eq!(mul_wide(a, b), reference::mul_wide(a, b), "mul_wide({a}, {b})");
            let (hi, lo, d) = (rng.wide(), rng.wide(), rng.wide());
            assert_eq!(div_wide(hi, lo, d), reference::div_wide(hi, lo, d), "div_wide({hi}, {lo}, {d})");
            // And the quotients that exist: hi < d.
            let hi = if d > 0 { hi % d } else { 0 };
            assert_eq!(div_wide(hi, lo, d), reference::div_wide(hi, lo, d), "div_wide({hi}, {lo}, {d})");
        }
        for _ in 0..1_000_000 {
            let x = rng.wide();
            let d = (rng.next() & M32).max(1);
            assert_eq!(div_small(x, d), x / d as u128, "div_small({x}, {d})");
        }
        // Edges: every limb-count boundary of the divisor, the largest quotients, add-back cases.
        for d in [1u128, 2, u32::MAX as u128, 1 << 32, u64::MAX as u128, 1 << 64, (1 << 96) - 1, 1 << 96, u128::MAX, u128::MAX - 1, 0x8000_0000_0000_0000_0000_0000_0000_0001] {
            for (hi, lo) in [(0, 0), (0, u128::MAX), (d - 1, u128::MAX), (d - 1, 0), (d / 2, 12345)] {
                assert_eq!(div_wide(hi, lo, d), reference::div_wide(hi, lo, d), "edge div_wide({hi}, {lo}, {d})");
            }
        }
    }

    #[test]
    fn div_wide_inverts_mul_wide() {
        let cases = [(u128::MAX, 7u128), (1u128 << 100, (1u128 << 90) + 12345), (987654321, 123456789)];
        for (a, b) in cases {
            let (hi, lo) = mul_wide(a, b);
            assert_eq!(div_wide(hi, lo, b), Some(a));
        }
        assert_eq!(div_wide(5, 0, 5), None);
        assert_eq!(mul_div(10, 10, 3), Some(33));
    }
}
