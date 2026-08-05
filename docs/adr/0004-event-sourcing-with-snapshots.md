# 0004 — Event sourcing with periodic snapshots

**Status:** Accepted (scaffolding only in V1 — see roadmap)
**Date:** 2026-07-13

## Context

The book is pure in-memory state. A crash without persistence means restarting from an empty book, which is fine for a demo and unacceptable for anything else. There is also a subtler property we want: **any state the engine has ever been in should be reproducible** — for auditing, for debugging a report of "the book looked wrong at 14:23:07", and for replaying against a new version of the engine.

## Decision

Persistence is an **append-only log of engine events**:
- `OrderPlaced { id, side, kind, price_ticks?, qty, ts }`
- `OrderCancelled { id, ts }`
- `Traded { taker, maker, price_ticks, qty, ts }`

Events are serialised (`bincode` for speed, `serde_json` for humans in dev builds) and appended to a per-symbol file. A background task fsyncs on a configurable interval — a bounded delay in exchange for orders-of-magnitude better throughput.

Every N events (default 100 000) the actor emits a **snapshot** — the full state of the book — to a second file, and truncates the event log up to that snapshot. Recovery replays snapshot + tail.

## Consequences

**Positive**
- Recovery is boring: load snapshot, replay events, ready.
- The log is the same substrate a market-data feed could consume — a v2 could publish events to a Kafka topic instead of / in addition to disk.
- Testing time-travel scenarios (feed the same event stream to two engine versions and compare) is straightforward.

**Negative**
- fsync-per-event kills throughput; batched fsync means you can lose the tail on crash. Tunable, documented.
- The engine now has a background writer task. Its failure modes (disk full, IO error) must not stall the actor — writes go through a bounded queue, overflow logs a metric and drops the event.

## Alternatives considered

1. **No persistence.** Accepted for V1. The interface for the persistence layer is defined; the implementation is a stub. See roadmap in the README.
2. **`SELECT * FROM Postgres`.** A full RDBMS in the hot path adds ~1 ms per commit — three orders of magnitude worse than matching itself.
3. **WAL-only, no snapshots.** Fine for a few hours; catastrophic when the log grows to gigabytes. Snapshots bound recovery time.

## References

- Martin Kleppmann, *Designing Data-Intensive Applications*, chapter 11
- Fowler & Rinat Abdullin, *Event Sourcing*
