use std::cmp::Ordering;
use substreams::scalar::BigDecimal;

/// Trait for types that can be converted to BigDecimal
///
/// This trait provides a way to convert numeric types to BigDecimal for use in
/// database operations. It's used as a base trait for numeric operations.
pub trait ToBigDecimal {
    /// Convert self to a BigDecimal
    ///
    /// For numeric types, this should be a lossless conversion.
    /// For string types, this may parse and panic if the string is not a valid number.
    fn to_big_decimal(&self) -> BigDecimal;
}

/// Trait for types that can be added to a BigDecimal in-place
///
/// This trait enables efficient accumulation by mutating a BigDecimal directly
/// instead of creating intermediate allocations. Types implementing this trait
/// can be used with the `add()` and `sub()` methods on database rows.
///
/// This trait extends `ToBigDecimal` to allow conversion to BigDecimal when needed.
pub trait NumericAddable: ToBigDecimal {
    /// Add self to the target BigDecimal, mutating it directly
    ///
    /// This enables efficient accumulation without intermediate allocations.
    /// For example: `100i64.add_assign_to(&mut target)` will add 100 to target.
    fn add_assign_to(&self, target: &mut BigDecimal);

    /// Subtract self from the target BigDecimal, mutating it directly
    ///
    /// Equivalent to: `*target -= self`
    /// For example: `50i64.sub_assign_from(&mut target)` will subtract 50 from target.
    fn sub_assign_from(&self, target: &mut BigDecimal);
}

/// Trait for types that can be compared against a BigDecimal for min/max operations
///
/// This trait leverages BigDecimal's native PartialOrd implementations for primitive
/// types, enabling zero-allocation comparisons for integers.
pub trait NumericComparable: ToBigDecimal {
    /// Compare self against a BigDecimal value
    ///
    /// For primitive integers, this uses BigDecimal's native PartialOrd implementation
    /// which requires no allocation. For other types, conversion may be needed.
    fn cmp_to_big_decimal(&self, other: &BigDecimal) -> Ordering;
}
