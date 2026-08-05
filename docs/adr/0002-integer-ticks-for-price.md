# 0002 — Prices as integer ticks

**Status:** Accepted
**Date:** 2026-07-13

## Context

The tempting representation for a price is `f64`. It is also the wrong one.

Every senior engineer who has ever touched financial code has a story: an aggregated position drifts by fractions of a cent, a comparison `a == b` returns false on values that should be equal, a total is off by `1e-15` and reconciliation blows up. IEEE-754 does not support fractional decimal arithmetic exactly. `0.1 + 0.2 != 0.3` is not folklore, it is spec.

## Decision

Prices are `i64` **ticks**. A tick is the smallest tradeable price increment for the symbol.

```rust
pub struct Price(pub i64);   // number of ticks
```

The type is a newtype (not `type Price = i64`) so the compiler distinguishes "a price" from "a raw integer that happens to be an int".

Quantities are `u64` units, at whatever fractional scale the symbol requires (fixed-point at the caller's chosen scale). Conversions to/from human-readable strings happen at the API boundary only.

`==`, `<`, `>`, add, subtract, multiply-by-quantity are all exact integer operations. Sums of trades reconcile down to the last tick, always.

## Consequences

**Positive**
- Exact arithmetic. `SUM(price * quantity)` for a trade set is the same value on the venue and on the broker.
- Ordering in the book is deterministic. `BTreeMap<Price, _>` gives a totally-ordered structure with zero surprises.
- Fast — 64-bit integer ops are one cycle on the hot path.

**Negative**
- Every API boundary needs a scale. The proto file documents that all `price_ticks` are int64; per-symbol conversion tables live at the client (or a `symbols` catalog service in a v2).
- New developers must learn "no floats in the engine, ever". A `#[deny(clippy::float_arithmetic)]` on the engine module keeps that honest.

## Alternatives considered

1. **`f64`.** Rejected — see context.
2. **`rust_decimal`.** Correct, but ~10× slower than `i64` on hot paths (allocations, wider ops). We do not need general decimals, we need integer ticks. Reserve `rust_decimal` for the API layer where humans read values.
3. **Basis points as `i32`.** Fine for FX and interest rates, awkward for equities and crypto where tick sizes vary wildly. Ticks generalise.

## References

- Douglas Crockford, *JavaScript: The Good Parts*, appendix on floating-point comparison
- Nomura Financial Engineering — internal style guide (leaked years ago; still correct)
