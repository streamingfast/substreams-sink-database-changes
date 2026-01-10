# Add AddAssign/SubAssign Support to substreams BigDecimal

## Overview

Add `AddAssign` and `SubAssign` trait implementations to the `BigDecimal` wrapper type in the substreams-rs crate. This will enable in-place mutation operations which can reduce allocations in downstream libraries like `substreams-sink-database-changes`.

## Current Situation

**File:** `substreams/src/scalar.rs`

The `BigDecimal` wrapper currently implements:
- `Add<T>` (line 274-283)
- `Sub<T>` (line 285-294)
- `Mul<T>` (line 296-305)
- `Div<T>` (line 307-321)

But **does NOT implement**:
- `AddAssign` (no `+=` operator)
- `SubAssign` (no `-=` operator)

This means code like `*target += value` doesn't compile, forcing users to write `*target = target.clone() + value`, which allocates unnecessarily.

## Why This Matters

**Use case in substreams-sink-database-changes:**

When accumulating numeric values (e.g., balance updates in blockchain data), we need to:
1. Parse existing value from string to BigDecimal
2. Add/subtract a new value
3. Convert back to string

**Current code (forced by missing AddAssign):**
```rust
let mut target = BigDecimal::from_str(&existing.value).unwrap();
let value = BigDecimal::from(100i64);
*target = target.clone() + value;  // ❌ Unnecessary clone
```

**Desired code (with AddAssign):**
```rust
let mut target = BigDecimal::from_str(&existing.value).unwrap();
let value = BigDecimal::from(100i64);
target += value;  // ✅ No clone needed
```

## Implementation Plan

### Step 1: Add use statement

**Location:** `substreams/src/scalar.rs`, line 1-4

Update the imports to include `AddAssign` and `SubAssign`:

```rust
use std::ops::{
    Add, AddAssign, BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign,
    Div, Mul, Neg, Rem, Shl, ShlAssign, Shr, ShrAssign, Sub, SubAssign,
};
```

### Step 2: Implement AddAssign for BigDecimal

**Location:** After the `Sub` implementation (after line 294)

Add the following implementations:

```rust
impl<T> AddAssign<T> for BigDecimal
where
    T: Into<BigDecimal>,
{
    fn add_assign(&mut self, other: T) {
        self.0 = self.0.clone() + other.into().0;
    }
}

impl AddAssign<&BigDecimal> for BigDecimal {
    fn add_assign(&mut self, other: &BigDecimal) {
        self.0 = self.0.clone() + other.0.clone();
    }
}
```

**Why two implementations?**
- First one handles owned values and types that convert to BigDecimal
- Second one specifically handles `&BigDecimal` to avoid unnecessary wrapping

### Step 3: Implement SubAssign for BigDecimal

**Location:** After the `AddAssign` implementations

```rust
impl<T> SubAssign<T> for BigDecimal
where
    T: Into<BigDecimal>,
{
    fn sub_assign(&mut self, other: T) {
        self.0 = self.0.clone() - other.into().0;
    }
}

impl SubAssign<&BigDecimal> for BigDecimal {
    fn sub_assign(&mut self, other: &BigDecimal) {
        self.0 = self.0.clone() - other.0.clone();
    }
}
```

## Complete Implementation

Here's the complete code to add to `substreams/src/scalar.rs`:

```rust
// Add after line 294 (after Sub implementation)

impl<T> AddAssign<T> for BigDecimal
where
    T: Into<BigDecimal>,
{
    fn add_assign(&mut self, other: T) {
        self.0 = self.0.clone() + other.into().0;
    }
}

impl AddAssign<&BigDecimal> for BigDecimal {
    fn add_assign(&mut self, other: &BigDecimal) {
        self.0 = self.0.clone() + other.0.clone();
    }
}

impl<T> SubAssign<T> for BigDecimal
where
    T: Into<BigDecimal>,
{
    fn sub_assign(&mut self, other: T) {
        self.0 = self.0.clone() - other.into().0;
    }
}

impl SubAssign<&BigDecimal> for BigDecimal {
    fn sub_assign(&mut self, other: &BigDecimal) {
        self.0 = self.0.clone() - other.0.clone();
    }
}
```

## Note on Performance

The implementation still requires cloning the underlying `bigdecimal::BigDecimal` because:
1. The wrapper doesn't provide mutable access to the inner type
2. The underlying `bigdecimal` crate's `AddAssign` consumes `self`

This is acceptable because:
- It's better than forcing downstream users to write `*target = target.clone() + value`
- The clone happens inside the operator, making code cleaner
- Future optimization could expose mutable access if needed

## Testing

After implementing, verify with:

```rust
#[test]
fn test_bigdecimal_add_assign() {
    let mut a = BigDecimal::from(100);
    let b = BigDecimal::from(50);
    a += b;
    assert_eq!(a, BigDecimal::from(150));
}

#[test]
fn test_bigdecimal_add_assign_ref() {
    let mut a = BigDecimal::from(100);
    let b = BigDecimal::from(50);
    a += &b;
    assert_eq!(a, BigDecimal::from(150));
}

#[test]
fn test_bigdecimal_sub_assign() {
    let mut a = BigDecimal::from(100);
    let b = BigDecimal::from(30);
    a -= b;
    assert_eq!(a, BigDecimal::from(70));
}

#[test]
fn test_bigdecimal_sub_assign_ref() {
    let mut a = BigDecimal::from(100);
    let b = BigDecimal::from(30);
    a -= &b;
    assert_eq!(a, BigDecimal::from(70));
}
```

## Benefits for Downstream Crates

Once implemented, `substreams-sink-database-changes` can simplify its code:

**Before:**
```rust
fn accumulate_add<T: NumericAddable>(&mut self, value: T, negate: bool) {
    let mut target = BigDecimal::from_str(&existing.value).unwrap();

    // Forced to clone because AddAssign doesn't exist
    if negate {
        *target = target.clone() - BigDecimal::from(value);
    } else {
        *target = target.clone() + BigDecimal::from(value);
    }
}
```

**After:**
```rust
fn accumulate_add<T: NumericAddable>(&mut self, value: T, negate: bool) {
    let mut target = BigDecimal::from_str(&existing.value).unwrap();

    // Clean, idiomatic Rust
    if negate {
        target -= BigDecimal::from(value);
    } else {
        target += BigDecimal::from(value);
    }
}
```

## Files to Modify

1. **substreams/src/scalar.rs**
   - Add imports (line 1-4)
   - Add `AddAssign` implementations (~4 lines, after line 294)
   - Add `SubAssign` implementations (~4 lines, after AddAssign)

2. **substreams/src/scalar.rs** (tests section, if exists)
   - Add unit tests for AddAssign/SubAssign

Total changes: ~30 lines of code

## Version Bump

This is a **minor version bump** (e.g., 0.7.0 → 0.8.0) because:
- Adds new public API (trait implementations)
- Fully backward compatible
- No breaking changes

## Alternative: MulAssign and DivAssign

Consider also adding `MulAssign` and `DivAssign` for consistency:

```rust
impl<T> MulAssign<T> for BigDecimal
where
    T: Into<BigDecimal>,
{
    fn mul_assign(&mut self, other: T) {
        self.0 = self.0.clone() * other.into().0;
    }
}

impl<T> DivAssign<T> for BigDecimal
where
    T: Into<BigDecimal>,
{
    fn div_assign(&mut self, other: T) {
        let other = other.into();
        if other.is_zero() {
            panic!("attempt to divide by zero");
        }
        self.0 = self.0.clone() / other.0;
    }
}
```

These would provide the complete set of compound assignment operators.
