# 0005 — Property-based testing for book invariants

**Status:** Accepted
**Date:** 2026-07-13

## Context

Example-based tests (`test_market_buy_matches_best_ask`) are necessary but insufficient. A matching engine has too many combinations of order types × book states × arrival orders for any one engineer to enumerate. Bugs live in the sequences nobody thought of.

## Decision

For every invariant that must hold **after every operation**, write a `proptest` that:

1. Generates a random sequence of valid `Command`s (place, cancel).
2. Applies them to a fresh `OrderBook`.
3. Asserts the invariant.

The invariants:

- **Ordering** — best bid ≤ best ask, always. A book that self-crosses is broken.
- **Conservation** — sum of `filled_qty` over all resulting trades equals sum of matched quantities on both sides. Nothing appears or disappears.
- **FIFO within a level** — if two orders sit at the same price, the older one fills first.
- **Cancel idempotence** — cancelling a non-existent id is a no-op, cancelling twice is a no-op.
- **Price-time priority** — a market buy of size `k` fills against the lowest-price asks first, and within each price level against the oldest order first.

`proptest` shrinks failing sequences to a minimal repro automatically. When a bug is found, the shrunk sequence goes into the example suite as a permanent regression test.

## Consequences

**Positive**
- Bugs get caught in sequences that no example test would have hit.
- Refactors on the engine (say, swapping the data structure per ADR 0003 v2) are protected — the invariants ride along and either pass or fail loudly.
- Documentation-by-code: the invariants are literally the contract.

**Negative**
- ~2–3× the time of example tests. Bounded — each proptest runs 256 iterations by default; that's < 1 second per property on the engine.
- Understanding a proptest failure takes practice. The shrunk minimal repro is usually obvious, but not always.

## Alternatives considered

1. **Example tests only.** Sufficient for happy paths, catastrophic for tail cases. Every non-trivial matching engine bug I have seen in the wild would have been caught by a property test.
2. **Fuzzing via `cargo-fuzz`.** Great for discovering panics; less good at asserting semantic invariants. `proptest` and fuzzing are complementary; fuzz is on the roadmap.

## References

- John Hughes — *QuickCheck: A Lightweight Tool for Random Testing of Haskell Programs*
- [`proptest` book](https://proptest-rs.github.io/proptest/)
