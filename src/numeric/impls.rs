use crate::numeric::{NumericAddable, ToBigDecimal};
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
