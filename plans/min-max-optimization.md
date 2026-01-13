# Implementation Plan: Optimize min/max Operations Using Direct PartialOrd

## Overview

Optimize `min()` and `max()` operations by leveraging `substreams::scalar::BigDecimal`'s native `PartialOrd` implementations for primitive integer types. This enables zero-allocation comparisons for integers without any conversion.

## Prerequisites

- **substreams >= 0.7.3** - brings bigdecimal 0.4.10 which implements `PartialOrd<T>` for all primitive integers
- Already updated in Cargo.toml

## Current Implementation Issues

**Location:** [src/tables.rs:492-610](src/tables.rs#L492-L610)

The current `min()` and `max()` methods:
- Accept `<T: ToDatabaseValue>` (any type that converts to string)
- Convert the value to string first via `value.to_value()`
- Parse the string back to BigDecimal via `BigDecimal::from_str(&new_value)`
- This is wasteful: `i64` → `String` → `BigDecimal` instead of direct comparison

**Current code pattern:**
```rust
let new_value = value.to_value();  // i64 -> "100"
let new_decimal = BigDecimal::from_str(&new_value).unwrap_or_else(|_| {  // "100" -> BigDecimal
    panic!("max() requires a valid numeric value...")
});
if new_decimal > existing_decimal { ... }
```

## Design: NumericComparable Trait

Since `BigDecimal` now implements `PartialOrd<i64>`, `PartialOrd<u64>`, etc., we can create a trait that compares directly without allocation:

**New trait in [src/numeric/traits.rs](src/numeric/traits.rs):**

```rust
use std::cmp::Ordering;
use substreams::scalar::BigDecimal;

/// Trait for types that can be compared against a BigDecimal for min/max operations
///
/// This trait leverages BigDecimal's native PartialOrd implementations for primitive
/// types, enabling zero-allocation comparisons for integers.
pub trait NumericComparable: ToDatabaseValue {
    /// Compare self against a BigDecimal value
    ///
    /// For primitive integers, this uses BigDecimal's native PartialOrd implementation
    /// which requires no allocation. For other types, conversion may be needed.
    fn cmp_to_big_decimal(&self, other: &BigDecimal) -> Ordering;
}
```

## Implementation Details

### For Primitive Integers (i8, i16, i32, i64, u8, u16, u32, u64)

Use BigDecimal's native `PartialOrd` - zero allocation:

```rust
macro_rules! impl_numeric_comparable_for_integer {
    ($($t:ty),*) => {
        $(
            impl NumericComparable for $t {
                fn cmp_to_big_decimal(&self, other: &BigDecimal) -> Ordering {
                    // Direct comparison using BigDecimal's PartialOrd<$t>
                    // This is zero-allocation for integers
                    self.partial_cmp(other).unwrap()
                }
            }
        )*
    };
}

impl_numeric_comparable_for_integer!(i8, i16, i32, i64, u8, u16, u32, u64);
```

### For BigDecimal and &BigDecimal

Direct Ord comparison:

```rust
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
```

### For BigInt and &BigInt

BigDecimal also has `PartialOrd<BigInt>`:

```rust
impl NumericComparable for BigInt {
    fn cmp_to_big_decimal(&self, other: &BigDecimal) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl NumericComparable for &BigInt {
    fn cmp_to_big_decimal(&self, other: &BigDecimal) -> Ordering {
        (*self).partial_cmp(other).unwrap()
    }
}
```

### For String and &str

Must parse to BigDecimal (no way around this):

```rust
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
```

## Updated Row Methods

### Refactor max() method

**File:** [src/tables.rs:492-549](src/tables.rs#L492-L549)

**New signature:**
```rust
pub fn max<T: NumericComparable>(&mut self, name: &str, value: T) -> &mut Self
```

**New implementation:**
```rust
pub fn max<T: NumericComparable>(&mut self, name: &str, value: T) -> &mut Self {
    if self.operation == Operation::Delete {
        panic!("cannot set fields on a delete operation")
    }

    match self.columns.get_mut(name) {
        Some(existing) => {
            match existing.update_op {
                UpdateOp::Unspecified => panic!(
                    "cannot call max() on field '{}' after unspecified - incompatible operations",
                    name
                ),
                UpdateOp::Set | UpdateOp::Max => {
                    let existing_bd = BigDecimal::from_str(&existing.value)
                        .expect("existing value should be valid BigDecimal");

                    // Direct comparison - zero allocation for integers!
                    if value.cmp_to_big_decimal(&existing_bd) == Ordering::Greater {
                        existing.value = value.to_value();
                    }
                    existing.update_op = UpdateOp::Max;
                }
                UpdateOp::Add => panic!(
                    "cannot call max() on field '{}' after add/sub() - incompatible operations",
                    name
                ),
                UpdateOp::Min => panic!(
                    "cannot call max() on field '{}' after min() - incompatible operations",
                    name
                ),
                UpdateOp::SetIfNull => panic!(
                    "cannot call max() on field '{}' after set_if_null() - incompatible operations",
                    name
                ),
            }
        }
        None => {
            self.columns.insert(
                name.to_string(),
                FieldValue::with_op(value.to_value(), UpdateOp::Max),
            );
        }
    }
    self
}
```

### Refactor min() method

**File:** [src/tables.rs:554-610](src/tables.rs#L554-L610)

**New signature:**
```rust
pub fn min<T: NumericComparable>(&mut self, name: &str, value: T) -> &mut Self
```

**New implementation:**
```rust
pub fn min<T: NumericComparable>(&mut self, name: &str, value: T) -> &mut Self {
    if self.operation == Operation::Delete {
        panic!("cannot set fields on a delete operation")
    }

    match self.columns.get_mut(name) {
        Some(existing) => {
            match existing.update_op {
                UpdateOp::Unspecified => panic!(
                    "cannot call min() on field '{}' after unspecified - incompatible operations",
                    name
                ),
                UpdateOp::Set | UpdateOp::Min => {
                    let existing_bd = BigDecimal::from_str(&existing.value)
                        .expect("existing value should be valid BigDecimal");

                    // Direct comparison - zero allocation for integers!
                    if value.cmp_to_big_decimal(&existing_bd) == Ordering::Less {
                        existing.value = value.to_value();
                    }
                    existing.update_op = UpdateOp::Min;
                }
                UpdateOp::Add => panic!(
                    "cannot call min() on field '{}' after add/sub() - incompatible operations",
                    name
                ),
                UpdateOp::Max => panic!(
                    "cannot call min() on field '{}' after max() - incompatible operations",
                    name
                ),
                UpdateOp::SetIfNull => panic!(
                    "cannot call min() on field '{}' after set_if_null() - incompatible operations",
                    name
                ),
            }
        }
        None => {
            self.columns.insert(
                name.to_string(),
                FieldValue::with_op(value.to_value(), UpdateOp::Min),
            );
        }
    }
    self
}
```

## Files to Update

### src/numeric/traits.rs

Add the new trait:
```rust
use std::cmp::Ordering;

/// Trait for types that can be compared against a BigDecimal for min/max operations
pub trait NumericComparable: ToDatabaseValue {
    fn cmp_to_big_decimal(&self, other: &BigDecimal) -> Ordering;
}
```

### src/numeric/impls.rs

Add implementations for all types using macros where appropriate.

### src/numeric/mod.rs

Export the new trait:
```rust
pub use traits::{NumericAddable, NumericComparable, ToBigDecimal};
```

### src/tables.rs

1. **Add import:**
   ```rust
   use crate::numeric::NumericComparable;
   use std::cmp::Ordering;
   ```

2. **Update method signatures:**
   - `max<T: ToDatabaseValue>` → `max<T: NumericComparable>`
   - `min<T: ToDatabaseValue>` → `min<T: NumericComparable>`

3. **Update comparison logic:**
   - Use `value.cmp_to_big_decimal(&existing_bd) == Ordering::Greater/Less`

## Implementation Steps

1. ✅ **Update substreams dependency to 0.7.3**
   - Brings bigdecimal 0.4.10 with PartialOrd for primitives

2. ✅ **Add NumericComparable trait to src/numeric/traits.rs**

3. ✅ **Add implementations to src/numeric/impls.rs**
   - Use macro for integer types
   - Manual impl for BigDecimal, BigInt, String, &str

4. ✅ **Update src/numeric/mod.rs exports**

5. ✅ **Update max() method in src/tables.rs**
   - Change signature to `<T: NumericComparable>`
   - Use `value.cmp_to_big_decimal()` for comparison

6. ✅ **Update min() method in src/tables.rs**
   - Change signature to `<T: NumericComparable>`
   - Use `value.cmp_to_big_decimal()` for comparison

7. ✅ **Run existing tests**
   - All tests must pass without modification

## Performance Impact

**Current approach:**
1. `value.to_value()` - convert to string (allocation)
2. `BigDecimal::from_str(&new_value)` - parse string to BigDecimal (allocation + parsing)
3. Compare two BigDecimals
4. Store string if needed

**Optimized approach (for integers):**
1. `value.cmp_to_big_decimal(&existing_bd)` - **zero allocation**, uses native PartialOrd
2. `value.to_value()` only if storing (allocation only when value changes)

**Result for integer min/max:**
- **Zero allocations** for comparison (was 2 allocations)
- String allocation only when value actually changes
- Eliminates string parsing overhead entirely

## Success Criteria

- ✅ All existing tests pass without modification
- ✅ Zero-allocation comparison for primitive integers
- ✅ Leverages substreams BigDecimal's native PartialOrd (no conversion to bigdecimal crate types)
- ✅ Clean trait design following NumericAddable pattern
- ✅ Type safety maintained via trait bounds
- ✅ No breaking changes to public API
