//! Numeric literal spelling. The parser keeps the original text; the value
//! and the suffix type are decided here, once.

use zil_ir::body::{FloatTy, IntTy};

const INT_SUFFIXES: [(&str, IntTy); 10] = [
    ("isize", IntTy::Isize),
    ("usize", IntTy::Usize),
    ("i8", IntTy::I8),
    ("i16", IntTy::I16),
    ("i32", IntTy::I32),
    ("i64", IntTy::I64),
    ("u8", IntTy::U8),
    ("u16", IntTy::U16),
    ("u32", IntTy::U32),
    ("u64", IntTy::U64),
];

/// Magnitude and optional suffix of an integer literal.
pub fn parse_int(text: &str) -> Option<(u128, Option<IntTy>)> {
    let (digits, suffix) = INT_SUFFIXES
        .iter()
        .find_map(|(s, t)| text.strip_suffix(s).map(|d| (d, Some(*t))))
        .unwrap_or((text, None));
    let (radix, body) = if let Some(rest) = digits.strip_prefix("0x") {
        (16, rest)
    } else if let Some(rest) = digits.strip_prefix("0b") {
        (2, rest)
    } else if let Some(rest) = digits.strip_prefix("0o") {
        (8, rest)
    } else {
        (10, digits)
    };
    let cleaned: String = body.chars().filter(|c| *c != '_').collect();
    if cleaned.is_empty() {
        return None;
    }
    u128::from_str_radix(&cleaned, radix)
        .ok()
        .map(|v| (v, suffix))
}

/// Value and optional suffix of a float literal.
pub fn parse_float(text: &str) -> Option<(f64, Option<FloatTy>)> {
    let (digits, suffix) = if let Some(d) = text.strip_suffix("f32") {
        (d, Some(FloatTy::F32))
    } else if let Some(d) = text.strip_suffix("f64") {
        (d, Some(FloatTy::F64))
    } else {
        (text, None)
    };
    let cleaned: String = digits.chars().filter(|c| *c != '_').collect();
    let v: f64 = cleaned.parse().ok()?;
    v.is_finite().then_some((v, suffix))
}

/// Whether the mathematical value `v` is representable in `ty`.
pub fn int_fits(v: i128, ty: IntTy, pointer_bits: u32) -> bool {
    let bits = ty.bits(pointer_bits);
    if ty.signed() {
        let min = -(1i128 << (bits - 1));
        let max = (1i128 << (bits - 1)) - 1;
        (min..=max).contains(&v)
    } else {
        v >= 0 && (v as u128) < (1u128 << bits)
    }
}

/// Two's complement bits of `v` truncated to the width of `ty`.
pub fn int_bits(v: i128, ty: IntTy, pointer_bits: u32) -> u128 {
    let bits = ty.bits(pointer_bits);
    let mask = if bits == 128 {
        u128::MAX
    } else {
        (1u128 << bits) - 1
    };
    (v as u128) & mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_spellings() {
        assert_eq!(parse_int("42"), Some((42, None)));
        assert_eq!(parse_int("127i8"), Some((127, Some(IntTy::I8))));
        assert_eq!(
            parse_int("0xaaaaaaaau32"),
            Some((0xaaaa_aaaa, Some(IntTy::U32)))
        );
        assert_eq!(parse_int("1_000usize"), Some((1000, Some(IntTy::Usize))));
        assert_eq!(parse_int("0x"), None);
    }

    #[test]
    fn ranges() {
        assert!(int_fits(-128, IntTy::I8, 64));
        assert!(!int_fits(128, IntTy::I8, 64));
        assert!(int_fits(4_294_967_295, IntTy::U32, 64));
        assert!(!int_fits(-1, IntTy::U32, 64));
        assert!(int_fits(-9_223_372_036_854_775_808, IntTy::Isize, 64));
        assert_eq!(int_bits(-1, IntTy::I8, 64), 0xff);
    }

    #[test]
    fn floats() {
        assert_eq!(parse_float("4294967295.0"), Some((4_294_967_295.0, None)));
        assert_eq!(parse_float("1.5f32"), Some((1.5, Some(FloatTy::F32))));
    }
}
