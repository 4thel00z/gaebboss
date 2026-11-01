//! Fixed-point decimals for quantities and money: an i64 mantissa with a
//! scale, parsed and printed without floating point, rounded the way the
//! Fachdokumentation prescribes (kaufmännisch, half away from zero).

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};

/// Most digits an i64 mantissa holds without overflow.
const MAX_DIGITS: usize = 18;

#[derive(Clone, Copy, Debug)]
pub struct Decimal {
    mantissa: i64,
    scale: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DecimalError {
    #[error("empty number")]
    Empty,
    #[error("invalid character at offset {0}")]
    InvalidChar(usize),
    #[error("decimal point without fraction digits")]
    TrailingPoint,
    #[error("more than 18 digits")]
    Overflow,
}

impl Decimal {
    pub const ZERO: Decimal = Decimal {
        mantissa: 0,
        scale: 0,
    };

    pub fn new(mantissa: i64, scale: u8) -> Decimal {
        Decimal { mantissa, scale }
    }

    /// Parses `-12.5`, `7`, `0.010`, `+3`: digits, one point, no comma, no
    /// exponent, no grouping.
    pub fn parse(text: &str) -> Result<Decimal, DecimalError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(DecimalError::Empty);
        }
        let (negative, digits) = match bytes[0] {
            b'-' => (true, &bytes[1..]),
            b'+' => (false, &bytes[1..]),
            _ => (false, bytes),
        };
        if digits.is_empty() {
            return Err(DecimalError::InvalidChar(0));
        }
        let mut mantissa: i64 = 0;
        let mut scale: u8 = 0;
        let mut seen_point = false;
        let mut count = 0usize;
        for (i, b) in digits.iter().enumerate() {
            match b {
                b'0'..=b'9' => {
                    count += 1;
                    if count > MAX_DIGITS {
                        return Err(DecimalError::Overflow);
                    }
                    mantissa = mantissa * 10 + i64::from(b - b'0');
                    if seen_point {
                        scale += 1;
                    }
                }
                b'.' if !seen_point && i > 0 => seen_point = true,
                _ => return Err(DecimalError::InvalidChar(text.len() - digits.len() + i)),
            }
        }
        if seen_point && digits.ends_with(b".") {
            return Err(DecimalError::TrailingPoint);
        }
        if negative {
            mantissa = -mantissa;
        }
        Ok(Decimal { mantissa, scale })
    }

    pub fn mantissa(self) -> i64 {
        self.mantissa
    }

    pub fn scale(self) -> u8 {
        self.scale
    }

    pub fn is_zero(self) -> bool {
        self.mantissa == 0
    }

    /// The same number at another scale; dropping digits rounds half away
    /// from zero (kaufmännische Rundung).
    pub fn rescale(self, scale: u8) -> Decimal {
        if scale == self.scale {
            return self;
        }
        if scale > self.scale {
            let factor = pow10(u32::from(scale - self.scale));
            return Decimal {
                mantissa: self.mantissa.saturating_mul(factor),
                scale,
            };
        }
        let factor = pow10(u32::from(self.scale - scale));
        let quotient = self.mantissa / factor;
        let remainder = (self.mantissa % factor).abs();
        let round_up = remainder * 2 >= factor;
        let adjust = match (round_up, self.mantissa < 0) {
            (false, _) => 0,
            (true, false) => 1,
            (true, true) => -1,
        };
        Decimal {
            mantissa: quotient + adjust,
            scale,
        }
    }

    fn aligned(self, other: Decimal) -> (i128, i128, u8) {
        let scale = self.scale.max(other.scale);
        let a = i128::from(self.mantissa) * i128::from(pow10(u32::from(scale - self.scale)));
        let b = i128::from(other.mantissa) * i128::from(pow10(u32::from(scale - other.scale)));
        (a, b, scale)
    }

    pub fn checked_add(self, other: Decimal) -> Option<Decimal> {
        let (a, b, scale) = self.aligned(other);
        Some(Decimal {
            mantissa: i64::try_from(a + b).ok()?,
            scale,
        })
    }

    pub fn checked_sub(self, other: Decimal) -> Option<Decimal> {
        let (a, b, scale) = self.aligned(other);
        Some(Decimal {
            mantissa: i64::try_from(a - b).ok()?,
            scale,
        })
    }

    /// The exact product; `None` when it does not fit 18 digits.
    pub fn checked_mul(self, other: Decimal) -> Option<Decimal> {
        let scale = self.scale.checked_add(other.scale)?;
        if usize::from(scale) > MAX_DIGITS {
            return None;
        }
        let product = i128::from(self.mantissa) * i128::from(other.mantissa);
        let mantissa = i64::try_from(product).ok()?;
        if mantissa.unsigned_abs() >= 10u64.pow(MAX_DIGITS as u32) {
            return None;
        }
        Some(Decimal { mantissa, scale })
    }

    /// `self * pct / 100`, exact.
    pub fn percent_of(self, pct: Decimal) -> Option<Decimal> {
        let product = self.checked_mul(pct)?;
        let scale = product.scale.checked_add(2)?;
        if usize::from(scale) > MAX_DIGITS {
            return None;
        }
        Some(Decimal {
            mantissa: product.mantissa,
            scale,
        })
    }

    /// Whole cents when the number has no finer digits.
    pub fn to_cents(self) -> Option<i64> {
        self.exact_at(2)
    }

    /// Whole thousandths when the number has no finer digits.
    pub fn to_thousandths(self) -> Option<i64> {
        self.exact_at(3)
    }

    fn exact_at(self, scale: u8) -> Option<i64> {
        if self.scale <= scale {
            return self
                .mantissa
                .checked_mul(pow10(u32::from(scale - self.scale)));
        }
        let factor = pow10(u32::from(self.scale - scale));
        if self.mantissa % factor != 0 {
            return None;
        }
        Some(self.mantissa / factor)
    }

    /// Mantissa and scale with trailing fraction zeros removed.
    fn normalized(self) -> (i64, u8) {
        let mut mantissa = self.mantissa;
        let mut scale = self.scale;
        while scale > 0 && mantissa % 10 == 0 {
            mantissa /= 10;
            scale -= 1;
        }
        (mantissa, scale)
    }
}

fn pow10(exp: u32) -> i64 {
    10i64.checked_pow(exp).unwrap_or(i64::MAX)
}

impl PartialEq for Decimal {
    fn eq(&self, other: &Decimal) -> bool {
        self.normalized() == other.normalized()
    }
}

impl Eq for Decimal {}

impl Hash for Decimal {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.normalized().hash(state);
    }
}

impl PartialOrd for Decimal {
    fn partial_cmp(&self, other: &Decimal) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Decimal {
    fn cmp(&self, other: &Decimal) -> Ordering {
        let (a, b, _) = self.aligned(*other);
        a.cmp(&b)
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let digits = self.mantissa.unsigned_abs().to_string();
        if self.mantissa < 0 {
            f.write_str("-")?;
        }
        let scale = usize::from(self.scale);
        if scale == 0 {
            return f.write_str(&digits);
        }
        if digits.len() <= scale {
            f.write_str("0.")?;
            for _ in digits.len()..scale {
                f.write_str("0")?;
            }
            return f.write_str(&digits);
        }
        let (int, frac) = digits.split_at(digits.len() - scale);
        write!(f, "{int}.{frac}")
    }
}

/// The item total: quantity times unit price, rounded to cents
/// (Fachdokumentation 4.6.17 rule 2).
pub fn item_total(qty: Decimal, up: Decimal) -> Option<Decimal> {
    Some(qty.checked_mul(up)?.rescale(2))
}

/// A total after a percentage discount, rounded to cents
/// (Fachdokumentation 4.6.17 rule 2).
pub fn total_after_discount(total: Decimal, pct: Decimal) -> Option<Decimal> {
    Some(total.checked_sub(total.percent_of(pct)?)?.rescale(2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(text: &str) -> Decimal {
        Decimal::parse(text).unwrap()
    }

    #[test]
    fn item_total_example() {
        assert_eq!(
            item_total(d("73.789"), d("9.77")).unwrap().to_string(),
            "720.92"
        );
    }

    #[test]
    fn discount_example() {
        let total = d("720.92");
        let after = total_after_discount(total, d("1.5")).unwrap();
        assert_eq!(after.to_string(), "710.11");
        assert_eq!(total.checked_sub(after).unwrap().to_string(), "10.81");
    }

    #[test]
    fn parse_keeps_scale() {
        assert_eq!(d("0.010").to_string(), "0.010");
        assert_eq!(d("5000.000").to_string(), "5000.000");
        assert_eq!(d("7").to_string(), "7");
        assert_eq!(d("-12.5").to_string(), "-12.5");
        assert_eq!(d("+3.25").to_string(), "3.25");
        assert_eq!(d("0.010").scale(), 3);
        assert_eq!(d("0.010").mantissa(), 10);
    }

    #[test]
    fn rescale_half_away_from_zero() {
        assert_eq!(d("2.345").rescale(2).to_string(), "2.35");
        assert_eq!(d("-2.345").rescale(2).to_string(), "-2.35");
        assert_eq!(d("2.344").rescale(2).to_string(), "2.34");
        assert_eq!(d("2.5").rescale(0).to_string(), "3");
        assert_eq!(d("1.5").rescale(3).to_string(), "1.500");
    }

    #[test]
    fn parse_rejects_comma_exponent_and_garbage() {
        assert!(Decimal::parse("1,5").is_err());
        assert!(Decimal::parse("1e3").is_err());
        assert!(Decimal::parse("").is_err());
        assert!(Decimal::parse("abc").is_err());
        assert!(Decimal::parse("1.").is_err());
        assert!(Decimal::parse("99999999999999999999").is_err());
    }

    #[test]
    fn display_negative_fraction_below_one() {
        assert_eq!(d("-0.5").to_string(), "-0.5");
        assert_eq!(Decimal::new(-5, 2).to_string(), "-0.05");
    }

    #[test]
    fn ordering_and_equality_across_scales() {
        assert_eq!(d("1.50"), d("1.5"));
        assert!(d("1.50") < d("1.51"));
        assert!(d("-1") < d("0.001"));
        let mut set = std::collections::HashSet::new();
        set.insert(d("1.50"));
        assert!(set.contains(&d("1.5")));
    }

    #[test]
    fn arithmetic_aligns_scales() {
        assert_eq!(d("1.5").checked_add(d("0.25")).unwrap().to_string(), "1.75");
        assert_eq!(d("2").checked_mul(d("0.333")).unwrap().to_string(), "0.666");
        assert_eq!(
            d("100").percent_of(d("19")).unwrap().rescale(2).to_string(),
            "19.00"
        );
        assert!(d("9999999999999").checked_mul(d("9999999999999")).is_none());
    }

    #[test]
    fn exact_conversions_to_cents_and_thousandths() {
        assert_eq!(d("54.20").to_cents(), Some(5420));
        assert_eq!(d("54.2").to_cents(), Some(5420));
        assert_eq!(d("54.201").to_cents(), None);
        assert_eq!(d("1240").to_thousandths(), Some(1_240_000));
        assert_eq!(d("0.5005").to_thousandths(), None);
        assert!(d("0").is_zero());
        assert!(d("0.000").is_zero());
    }

    #[test]
    fn sum_of_rounded_totals() {
        let totals = [d("720.92"), d("1000.00"), d("0.01")];
        let sum = totals
            .iter()
            .try_fold(Decimal::ZERO, |acc, t| acc.checked_add(*t))
            .unwrap();
        assert_eq!(sum.rescale(2).to_string(), "1720.93");
    }
}
