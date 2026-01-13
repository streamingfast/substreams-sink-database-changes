use crate::numeric::{NumericAddable, NumericComparable, ToBigDecimal};
use std::cmp::Ordering;
use std::str::FromStr;
use substreams::scalar::{BigDecimal, BigInt};

// Macro for implementing ToBigDecimal and NumericAddable on integer types that have From<T> for BigDecimal
macro_rules! impl_numeric_for_integer {
    ($($t:ty),*) => {
        $(
            impl ToBigDecimal for $t {
                fn to_big_decimal(&self) -> BigDecimal {
                    BigDecimal::from(*self)
                }
            }

            impl NumericAddable for $t {
                fn add_assign_to(&self, target: &mut BigDecimal) {
                    *target += BigDecimal::from(*self);
                }

                fn sub_assign_from(&self, target: &mut BigDecimal) {
                    *target -= BigDecimal::from(*self);
                }
            }
        )*
    };
}

// Macro for implementing ToBigDecimal and NumericAddable on integer types that need casting
macro_rules! impl_numeric_for_integer_via_cast {
    ($($t:ty => $cast:ty),*) => {
        $(
            impl ToBigDecimal for $t {
                fn to_big_decimal(&self) -> BigDecimal {
                    BigDecimal::from(*self as $cast)
                }
            }

            impl NumericAddable for $t {
                fn add_assign_to(&self, target: &mut BigDecimal) {
                    *target += BigDecimal::from(*self as $cast);
                }

                fn sub_assign_from(&self, target: &mut BigDecimal) {
                    *target -= BigDecimal::from(*self as $cast);
                }
            }
        )*
    };
}

// Apply to integer types that have From<T> implementations for BigDecimal
impl_numeric_for_integer!(i32, i64, u32, u64);

// Apply to smaller integer types that need casting via i64/u64
impl_numeric_for_integer_via_cast!(
    i8 => i64,
    i16 => i64,
    u8 => u64,
    u16 => u64
);

impl ToBigDecimal for BigDecimal {
    fn to_big_decimal(&self) -> BigDecimal {
        self.clone()
    }
}

impl NumericAddable for BigDecimal {
    fn add_assign_to(&self, target: &mut BigDecimal) {
        *target += self;
    }

    fn sub_assign_from(&self, target: &mut BigDecimal) {
        *target -= self;
    }
}

impl ToBigDecimal for &BigDecimal {
    fn to_big_decimal(&self) -> BigDecimal {
        (*self).clone()
    }
}

impl NumericAddable for &BigDecimal {
    fn add_assign_to(&self, target: &mut BigDecimal) {
        *target += *self;
    }

    fn sub_assign_from(&self, target: &mut BigDecimal) {
        *target -= *self;
    }
}

impl ToBigDecimal for BigInt {
    fn to_big_decimal(&self) -> BigDecimal {
        BigDecimal::from(self.clone())
    }
}

impl NumericAddable for BigInt {
    fn add_assign_to(&self, target: &mut BigDecimal) {
        *target += BigDecimal::from(self.clone());
    }

    fn sub_assign_from(&self, target: &mut BigDecimal) {
        *target -= BigDecimal::from(self.clone());
    }
}

impl ToBigDecimal for &BigInt {
    fn to_big_decimal(&self) -> BigDecimal {
        BigDecimal::from((*self).clone())
    }
}

impl NumericAddable for &BigInt {
    fn add_assign_to(&self, target: &mut BigDecimal) {
        *target += BigDecimal::from((*self).clone());
    }

    fn sub_assign_from(&self, target: &mut BigDecimal) {
        *target -= BigDecimal::from((*self).clone());
    }
}

// Macro for implementing ToBigDecimal and NumericAddable for string types
macro_rules! impl_numeric_for_string {
    ($($t:ty),*) => {
        $(
            impl ToBigDecimal for $t {
                fn to_big_decimal(&self) -> BigDecimal {
                    BigDecimal::from_str(self).unwrap_or_else(|_| {
                        panic!(
                            "add/sub() requires a valid numeric value, got: {}",
                            self
                        )
                    })
                }
            }

            impl NumericAddable for $t {
                fn add_assign_to(&self, target: &mut BigDecimal) {
                    let value = self.to_big_decimal();
                    *target += value;
                }

                fn sub_assign_from(&self, target: &mut BigDecimal) {
                    let value = self.to_big_decimal();
                    *target -= value;
                }
            }
        )*
    };
}

// Apply to String and &str types
impl_numeric_for_string!(String, &str);

// ============================================================
// NumericComparable implementations
// ============================================================

// Macro for implementing NumericComparable on integer types using BigDecimal's native PartialOrd
macro_rules! impl_numeric_comparable_for_integer {
    ($($t:ty),*) => {
        $(
            impl NumericComparable for $t {
                fn cmp_to_big_decimal(&self, other: &BigDecimal) -> Ordering {
                    // Direct comparison using BigDecimal's PartialOrd<$t>
                    // This is zero-allocation for integers
                    self.partial_cmp(other)
                        .expect("BigDecimal comparison should always succeed for integers")
                }
            }
        )*
    };
}

// Apply to all primitive integer types
impl_numeric_comparable_for_integer!(i8, i16, i32, i64, u8, u16, u32, u64);

// Specialized implementations for isize/usize (cast to i64/u64)
impl NumericComparable for isize {
    fn cmp_to_big_decimal(&self, other: &BigDecimal) -> Ordering {
        (*self as i64).partial_cmp(other)
            .expect("BigDecimal comparison should always succeed for integers")
    }
}

impl NumericComparable for usize {
    fn cmp_to_big_decimal(&self, other: &BigDecimal) -> Ordering {
        (*self as u64).partial_cmp(other)
            .expect("BigDecimal comparison should always succeed for integers")
    }
}

// i128 and u128 - use BigDecimal's native PartialOrd for these types
impl NumericComparable for i128 {
    fn cmp_to_big_decimal(&self, other: &BigDecimal) -> Ordering {
        self.partial_cmp(other)
            .expect("BigDecimal comparison should always succeed for i128")
    }
}

impl NumericComparable for u128 {
    fn cmp_to_big_decimal(&self, other: &BigDecimal) -> Ordering {
        self.partial_cmp(other)
            .expect("BigDecimal comparison should always succeed for u128")
    }
}

// BigDecimal implementations
impl NumericComparable for BigDecimal {
    fn cmp_to_big_decimal(&self, other: &BigDecimal) -> Ordering {
        self.cmp(other)
    }
}

impl NumericComparable for &BigDecimal {
    fn cmp_to_big_decimal(&self, other: &BigDecimal) -> Ordering {
        (*self).cmp(other)
    }
}

// BigInt implementations - convert to BigDecimal for comparison
impl NumericComparable for BigInt {
    fn cmp_to_big_decimal(&self, other: &BigDecimal) -> Ordering {
        let self_bd = BigDecimal::from(self.clone());
        self_bd.cmp(other)
    }
}

impl NumericComparable for &BigInt {
    fn cmp_to_big_decimal(&self, other: &BigDecimal) -> Ordering {
        let self_bd = BigDecimal::from((*self).clone());
        self_bd.cmp(other)
    }
}

// String implementations - must parse to BigDecimal
impl NumericComparable for String {
    fn cmp_to_big_decimal(&self, other: &BigDecimal) -> Ordering {
        let self_bd = BigDecimal::from_str(self).unwrap_or_else(|_| {
            panic!("min/max() requires a valid numeric value, got: {}", self)
        });
        self_bd.cmp(other)
    }
}

impl NumericComparable for &str {
    fn cmp_to_big_decimal(&self, other: &BigDecimal) -> Ordering {
        let self_bd = BigDecimal::from_str(self).unwrap_or_else(|_| {
            panic!("min/max() requires a valid numeric value, got: {}", self)
        });
        self_bd.cmp(other)
    }
}
