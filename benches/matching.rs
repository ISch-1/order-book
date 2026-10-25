//! Criterion benchmarks — synchronous, hit the pure `OrderBook`.
//!
//! Run with: `cargo bench --bench matching`
//! Report:   `target/criterion/report/index.html`

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use order_book::engine::{
    book::OrderBook,
    order::{Order, Side},
    price::Price,
};
use uuid::Uuid;

fn build_prefilled(n_per_side: usize) -> OrderBook {
    let mut b = OrderBook::new("BTC-USD");
    for i in 0..n_per_side as i64 {
        b.place(Order::limit(
            Uuid::new_v4(),
            Side::Buy,
            Price(100 - i % 10),
            1 + (i as u64 % 5),
            i,
        ))
        .unwrap();
        b.place(Order::limit(
            Uuid::new_v4(),
            Side::Sell,
            Price(101 + i % 10),
            1 + (i as u64 % 5),
            i,
        ))
        .unwrap();
    }
    b
}

fn bench_place_non_crossing(c: &mut Criterion) {
    let mut group = c.benchmark_group("place_non_crossing");
    group.throughput(Throughput::Elements(1));

    let mut book = build_prefilled(1_000);
    let mut counter: i64 = 0;
    group.bench_function("limit_buy_deep_book", |bencher| {
        bencher.iter(|| {
            counter += 1;
            let o = Order::limit(Uuid::new_v4(), Side::Buy, Price(50), 1, counter);
            black_box(book.place(o).unwrap());
        });
    });

    group.finish();
}

fn bench_place_crossing(c: &mut Criterion) {
    let mut group = c.benchmark_group("place_crossing");
    group.throughput(Throughput::Elements(1));

    group.bench_function("market_buy_5_units_deep_book", |bencher| {
        bencher.iter_batched(
            || build_prefilled(1_000),
            |mut book| {
                let o = Order::market(Uuid::new_v4(), Side::Buy, 5, 0);
                black_box(book.place(o).unwrap());
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("large_market_sweeps_50_units", |bencher| {
        bencher.iter_batched(
            || build_prefilled(1_000),
            |mut book| {
                let o = Order::market(Uuid::new_v4(), Side::Buy, 50, 0);
                black_box(book.place(o).unwrap());
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_cancel(c: &mut Criterion) {
    let mut group = c.benchmark_group("cancel");
    group.throughput(Throughput::Elements(1));

    group.bench_function("cancel_random_resting", |bencher| {
        bencher.iter_batched(
            || {
                let mut b = OrderBook::new("BTC-USD");
                let mut ids = Vec::with_capacity(1_000);
                for i in 0..1_000i64 {
                    let id = Uuid::new_v4();
                    ids.push(id);
                    b.place(Order::limit(id, Side::Buy, Price(90 + i % 20), 1, i))
                        .unwrap();
                }
                (b, ids)
            },
            |(mut b, ids)| {
                let id = ids[ids.len() / 2];
                black_box(b.cancel(id));
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_place_non_crossing,
    bench_place_crossing,
    bench_cancel
);
criterion_main!(benches);
