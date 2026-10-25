# order-book

[![CI](https://github.com/ISch-1/order-book/actions/workflows/ci.yml/badge.svg)](https://github.com/ISch-1/order-book/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-1.80-orange)](https://www.rust-lang.org/)
[![gRPC](https://img.shields.io/badge/gRPC-tonic-blue)](https://github.com/hyperium/tonic)
[![License](https://img.shields.io/badge/License-MIT-blue)](LICENSE)

> A low-latency limit-order matching engine in Rust. One `tokio` task owns each symbol's book — no locks in the hot path — integer-tick prices, gRPC surface (`tonic`), Prometheus + OTel out of the box, property-based tests on the book invariants, criterion benchmarks that spit out µs latencies.

The point isn't a toy exchange. The point is **to demonstrate the design intuitions that separate acceptable systems code from good systems code**: choose the right data structure (`BTreeMap<Price, VecDeque<Order>>`, ADR 0003), choose the right concurrency model (actor per symbol, ADR 0001), choose the right numeric representation (integer ticks, never floats, ADR 0002), test invariants not examples (`proptest`, ADR 0005), and let the benchmarks — not the vibes — say whether the code is fast.

---

## What it does

- **Limit and market orders**, buy and sell.
- **Price-time priority** — within a price level, older orders fill first.
- **Partial fills**, with the residual either resting (limit) or dropped (market).
- **Cancel by order id**, O(log n + queue-depth), idempotent.
- **Top-of-book snapshot** with configurable depth.
- **gRPC service** at `:50051`, protobuf-defined.
- **Prometheus /metrics** at `:9100`, plus `/health`.
- **OTLP tracing** to any collector at `OTEL_EXPORTER_OTLP_ENDPOINT`.

Deferred to phase 2 (roadmap section below): event-log persistence, snapshots, streaming market data over gRPC, cross-symbol coordinator.

---

## Quickstart

Prerequisites: Rust 1.80+ (`rustup`), Docker for the observability stack, `grpcurl` for hand-calling the API.

```bash
# 1. build (protoc is required at compile time — the Dockerfile handles that; locally, install it)
cargo build --release

# 2. start observability stack
make up
# → Grafana at http://localhost:3000, Prometheus at :9090, Jaeger at :16686

# 3. run the engine
make run
# gRPC listening on :50051, HTTP (metrics + health) on :9100

# 4. place an order via grpcurl (the server has no reflection, so pass the .proto)
grpcurl -plaintext -import-path proto -proto order_book.proto \
  -d '{"symbol":"BTC-USD","side":"SIDE_SELL","orderType":"ORDER_TYPE_LIMIT","priceTicks":10100,"quantity":50}' \
  localhost:50051 orderbook.v1.OrderBookService/PlaceOrder

# 5. place a crossing market order — should fill
grpcurl -plaintext -import-path proto -proto order_book.proto \
  -d '{"symbol":"BTC-USD","side":"SIDE_BUY","orderType":"ORDER_TYPE_MARKET","quantity":30}' \
  localhost:50051 orderbook.v1.OrderBookService/PlaceOrder

# 6. peek at the book
grpcurl -plaintext -import-path proto -proto order_book.proto \
  -d '{"symbol":"BTC-USD","depth":5}' \
  localhost:50051 orderbook.v1.OrderBookService/GetTopOfBook
```

Then open Grafana → **Order Book — Engine** dashboard and you'll see the counters climb: orders/s, trades/s, match latency histograms, book depth per side.

---

## Design at a glance

- **[ADR 0001](docs/adr/0001-actor-per-symbol.md)** — one `SymbolActor` per symbol owns the book by move; callers talk over `mpsc`. Zero locks in the hot path, tail latency stays flat under contention.
- **[ADR 0002](docs/adr/0002-integer-ticks-for-price.md)** — prices are `i64` ticks. Never `f64`. Sums reconcile.
- **[ADR 0003](docs/adr/0003-btreemap-plus-vecdeque.md)** — `BTreeMap<Price, VecDeque<Order>>` for each side plus a `HashMap<OrderId, (Side, Price)>` for O(log n) cancel-by-id.
- **[ADR 0004](docs/adr/0004-event-sourcing-with-snapshots.md)** — engine already emits `EngineEvent`s; a persistence layer (append-only log + periodic snapshots) is the next step and hooks in without changing engine code.
- **[ADR 0005](docs/adr/0005-proptest-for-invariants.md)** — `proptest` covers the book invariants: no self-cross, cancel idempotence, duplicate-id rejection, quantity conservation.

Full sequence diagrams in [`docs/architecture.md`](docs/architecture.md).

---

## Running the tests

```bash
cargo test --all            # unit + integration + proptest
cargo bench --bench matching
open target/criterion/report/index.html   # or xdg-open on Linux
```

Test suites:
- `src/engine/book.rs` (`#[cfg(test)]`) — example-based unit tests for the book.
- `tests/actor_it.rs` — spawns a real `SymbolActor` and drives it via the mpsc channel; verifies end-to-end and event broadcast.
- `tests/proptest_invariants.rs` — 256 randomised sequences per property; failures shrink to a minimal repro.

CI runs `fmt --check`, `clippy -D warnings`, all tests, plus a bench smoke pass on `main`.

---

## Benchmarks

`cargo bench` produces a criterion report for each scenario (mean and median with confidence intervals, plus outlier analysis):

- **Non-crossing limit place** on a book of 1 000 resting orders — measures the pure insert path.
- **Market buy** consuming 5 units on the same book — measures a small match.
- **Market sweep** consuming 50 units — measures the multi-level match path.
- **Cancel** an arbitrary resting order — measures the index + VecDeque walk.

The numbers matter less than the shape of the distributions. Publish them in the repo (`target/criterion/report/`) alongside the pull request that changes the engine — regressions get caught here, not in production.

---

## Repo layout

```
order-book/
├── Cargo.toml            # tokio, tonic, proptest, criterion, metrics, opentelemetry
├── build.rs              # tonic-build compiles proto/order_book.proto
├── rust-toolchain.toml   # pinned 1.80.1
├── rustfmt.toml
├── Dockerfile            # multi-stage → distroless runtime
├── docker-compose.yml    # Prometheus + Grafana + OTel + Jaeger (observability only)
├── Makefile
│
├── proto/
│   └── order_book.proto  # gRPC surface — Side, OrderType, PlaceOrder, CancelOrder, GetTopOfBook
│
├── docs/
│   ├── architecture.md
│   └── adr/              # 5 ADRs
│
├── infra/
│   ├── prometheus/prometheus.yml
│   ├── grafana/          # provisioned datasources + Engine dashboard JSON
│   └── otel/otel-collector-config.yaml
│
├── src/
│   ├── main.rs           # binary entry, wires gRPC + HTTP servers
│   ├── lib.rs            # crate root, re-exports, proto module
│   ├── config.rs         # clap-derive CLI + env config
│   ├── engine/
│   │   ├── book.rs       # THE OrderBook — pure, single-threaded, testable
│   │   ├── order.rs, price.rs, trade.rs, events.rs, error.rs
│   │   └── mod.rs
│   ├── actor/
│   │   ├── command.rs    # Command enum with oneshot reply channels
│   │   ├── symbol_actor.rs # SymbolActor::spawn — the task body
│   │   └── mod.rs
│   ├── grpc/
│   │   ├── service.rs    # tonic OrderBookService impl + SymbolRegistry
│   │   └── mod.rs
│   └── telemetry/
│       ├── metrics.rs    # PrometheusHandle + bucket setup + record helpers
│       ├── tracing_setup.rs # tracing_subscriber + OTLP layer
│       └── mod.rs
│
├── tests/
│   ├── actor_it.rs           # end-to-end through the actor
│   └── proptest_invariants.rs # book invariants (256 cases per property)
│
├── benches/
│   └── matching.rs       # criterion — place / crossing / sweep / cancel
│
└── load/                 # future load-gen scripts (ghz for gRPC)
```

---

## Roadmap

- [x] Matching engine (limit + market, partial fills, cancels)
- [x] Actor per symbol (tokio mpsc + broadcast)
- [x] gRPC service (tonic)
- [x] Prometheus metrics + OTel tracing
- [x] `proptest` invariants + criterion benchmarks
- [ ] **Event log persistence** — append-only, per-symbol, with fsync interval config
- [ ] **Snapshots** every N events + replay on startup
- [ ] **gRPC streaming market data** — top-of-book updates and trade prints over server streaming
- [ ] **Multi-symbol coordinator** — cross-symbol operations (spread trades, atomic quotes)
- [ ] **ghz gRPC load generator** in `load/` + a documented headline number in this README

---

## Why this project exists

Together with [ecommerce-polyglot](https://github.com/ISch-1/ecommerce-polyglot) (Java · polyglot persistence · outbox) and [event-stream-platform](https://github.com/ISch-1/event-stream-platform) (Java · Kafka · ClickHouse · Avro), this repo shows the third leg of a senior backend portfolio: **systems code**. Rust, no locks, integer arithmetic, property-tested invariants, and benchmarks that don't lie.

## License

MIT — see [LICENSE](LICENSE).
