# 0003 — `BTreeMap<Price, VecDeque<Order>>` for the book

**Status:** Accepted
**Date:** 2026-07-13

## Context

The book has to answer four questions cheaply, in this order of frequency:

1. What is the best price on this side? (`O(log n)` acceptable — millions of times a second.)
2. Insert an order at a known price level. (Also very frequent.)
3. Match the taker against resting orders, in price-then-time order, until either the taker is exhausted or the crossing prices run out.
4. Cancel a specific order by id.

## Decision

The book on each side is a `BTreeMap<Price, PriceLevel>`, where `PriceLevel` holds a `VecDeque<Rc<RefCell<Order>>>` — a FIFO queue of orders at that price, plus an aggregate quantity cached for cheap depth reads.

- **Best price**: `map.first_key_value()` on the ask side, `map.last_key_value()` on the bid side. `O(log n)` and cache-friendly.
- **Insert at a level**: `map.entry(price).or_default().push_back(order)`. Amortised `O(log n)`.
- **Match**: iterate `.range_mut()` in the crossing direction, consume the front of each `VecDeque` until the taker is done. Nothing moves in memory except the front pointer of the queue.
- **Cancel by id**: an auxiliary `HashMap<OrderId, PriceLocation>` records `(side, price, index-into-queue)`. Cancels are `O(log n) + O(k)` where k is the position in the queue; for hot orders (near the front) this is essentially `O(log n)`.

## Consequences

**Positive**
- Off-the-shelf `std` types. No unsafe. No custom data structure to maintain and prove correct.
- Zero allocation in the steady-state match path — orders are popped from an existing `VecDeque`.
- Iteration over price levels is naturally sorted, which is exactly what matching needs.

**Negative**
- `O(log n)` on inserts and best-price lookup rather than the `O(1)` a skiplist with maintained top pointers could offer. In practice, the constants on `BTreeMap` are excellent — for symbols with < ~10k price levels (which is essentially all of them), we're at 15–20 comparisons per operation.
- Cancels of orders deep in a queue are linear in queue depth. If this becomes a hot path, promote `VecDeque` to an intrusive doubly-linked list where `HashMap<OrderId, _>` gives O(1) removal. Deferred until benchmarks show it matters.

## Alternatives considered

1. **Custom skiplist with `AtomicPtr` levels.** More complex, harder to prove correct, ~2× faster on tight microbenchmarks. Rejected — the actor model already removes contention, so the marginal speedup is on top of a constant that doesn't cost us the tail.
2. **Array of price levels indexed by tick offset from mid.** Blazing fast for equities with narrow spreads. Fails on symbols that trade wildly across price ranges. Rejected as too domain-specific for a portfolio engine.
3. **`SortedVec<Order>` — one flat sorted array per side.** `O(n)` inserts. Trivially wrong for anything but toy sizes.

## References

- CppCon 2017 — Carl Cook, "When a Microsecond Is an Eternity"
- Cboe Global Markets, US Options Book — Depth of Book white paper (2019)
