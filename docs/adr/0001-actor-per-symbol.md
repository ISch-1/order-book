# 0001 — Actor-per-symbol via message passing

**Status:** Accepted
**Date:** 2026-07-13

## Context

A matching engine has a single hot invariant: **the book cannot be observed mid-mutation**. If two threads try to match a buy and a sell against the same resting order, one of them can double-fill or race the price update. The classic fix is a mutex around the book. It works. It also destroys tail latency the moment contention rises: threads park, wake, cache lines bounce.

## Decision

There is exactly one owner of each symbol's book, and it is a **task**, not a lock.

- Each symbol runs a `SymbolActor` — a `tokio::spawn`ed task that owns its `OrderBook` by move.
- Callers send `Command`s over a `tokio::sync::mpsc::channel`. The actor receives one command at a time, mutates the book, and answers via a `tokio::sync::oneshot::Sender` embedded in the command.
- Market data (trades, top-of-book updates) fans out via `tokio::sync::broadcast`. Slow subscribers get lag notifications, they don't block the actor.

Rust's ownership rules make this cheap to prove correct: the book is *owned* by the actor's task; no `&mut` reference can leak out. The compiler enforces what a code review would otherwise have to.

## Consequences

**Positive**
- Zero locks in the hot path. Match latency stays predictable — no jitter from lock contention.
- The `OrderBook` type stays pure and testable. No `Arc<Mutex<...>>`, no async colouring. Unit tests hand it commands and inspect state directly.
- Adding a new symbol is spawning a task — no rebalancing, no shared-state extension.
- Trivial horizontal scaling story: partition symbols across processes when a single machine runs out.

**Negative**
- Cross-symbol operations (e.g. an atomic swap between two books) don't have a clean home. Not needed in V1; would require a coordinator actor if introduced.
- Backpressure on the command channel manifests as latency at the caller. Bounded channels (`mpsc::channel(1024)`) turn overload into visible errors instead of unbounded memory growth.

## Alternatives considered

1. **`Arc<Mutex<OrderBook>>`.** The default sitting in most demo repos. Rejected — tail latency degrades under contention and the whole point of this project is to keep the tail flat.
2. **`Arc<RwLock<OrderBook>>`.** Writers still serialise; readers pay less. But the matching engine is write-heavy and short-lived reads (top-of-book snapshots) can happen through the actor with equal cost.
3. **Sharded locks per price level.** Overkill and violates the "one owner of the invariant" principle — cross-level moves (a limit crossing multiple levels) reintroduce the same problem locks were meant to solve.

## References

- Carl Hewitt — original actor model paper
- Tokio docs — [channels](https://tokio.rs/tokio/tutorial/channels)
