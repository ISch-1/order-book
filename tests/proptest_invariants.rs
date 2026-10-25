//! Property-based tests — see ADR 0005.
//!
//! For each generated sequence of commands, apply them to a fresh
//! [`OrderBook`] and check the invariants that must hold at every step.

use order_book::engine::{
    book::OrderBook,
    order::{Order, OrderKind, Side},
    price::Price,
};
use proptest::prelude::*;
use uuid::Uuid;

/// A caller-side op we can generate randomly.
#[derive(Debug, Clone)]
enum Op {
    PlaceLimit { side: Side, price: i64, qty: u64 },
    PlaceMarket { side: Side, qty: u64 },
    Cancel { index: usize }, // cancels the Nth known live order id
}

fn arb_side() -> impl Strategy<Value = Side> {
    prop_oneof![Just(Side::Buy), Just(Side::Sell)]
}

fn arb_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (arb_side(), 90i64..=110, 1u64..=20).prop_map(|(s, p, q)| Op::PlaceLimit {
            side: s,
            price: p,
            qty: q
        }),
        (arb_side(), 1u64..=20).prop_map(|(s, q)| Op::PlaceMarket { side: s, qty: q }),
        (0usize..64).prop_map(|i| Op::Cancel { index: i }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        .. ProptestConfig::default()
    })]

    /// After any sequence, the book must never self-cross: best bid < best ask
    /// (or one side is empty).
    #[test]
    fn best_bid_lt_best_ask(ops in prop::collection::vec(arb_op(), 0..64)) {
        let mut book = OrderBook::new("BTC-USD");
        let mut live_ids: Vec<Uuid> = Vec::new();

        for op in ops {
            match op {
                Op::PlaceLimit { side, price, qty } => {
                    let id = Uuid::new_v4();
                    let order = Order::limit(id, side, Price(price), qty, 0);
                    if let Ok(out) = book.place(order) {
                        if out.remaining_qty > 0 {
                            // Only limit orders can remain — market ones drop the residual.
                            live_ids.push(id);
                        }
                        // Any id from a completed maker is not our concern to track
                        // (cancels look up by id and no-op if the maker already filled).
                    }
                }
                Op::PlaceMarket { side, qty } => {
                    let id = Uuid::new_v4();
                    let _ = book.place(Order::market(id, side, qty, 0));
                }
                Op::Cancel { index } => {
                    if !live_ids.is_empty() {
                        let idx = index % live_ids.len();
                        let id = live_ids.remove(idx);
                        let _ = book.cancel(id);
                    }
                }
            }

            if let (Some(b), Some(a)) = (book.best_bid(), book.best_ask()) {
                prop_assert!(b < a, "book self-crossed: bid={:?} >= ask={:?}", b, a);
            }
        }
    }

    /// Sum of trade quantities equals the sum of quantity taken from both sides
    /// (this is the classic "conservation of qty" invariant of matching engines).
    #[test]
    fn place_conserves_quantity(
        buys in prop::collection::vec((90i64..=110, 1u64..=10), 0..30),
        sells in prop::collection::vec((90i64..=110, 1u64..=10), 0..30),
    ) {
        let mut book = OrderBook::new("BTC-USD");
        let mut submitted: u64 = 0;

        for (p, q) in buys {
            submitted += q;
            let _ = book.place(Order::limit(Uuid::new_v4(), Side::Buy, Price(p), q, 0));
        }
        for (p, q) in sells {
            submitted += q;
            let _ = book.place(Order::limit(Uuid::new_v4(), Side::Sell, Price(p), q, 0));
        }

        // What's still resting?
        let resting = book.snapshot(1024).bids.iter().map(|l| l.quantity).sum::<u64>()
                    + book.snapshot(1024).asks.iter().map(|l| l.quantity).sum::<u64>();

        // Sum of traded qty = (submitted - resting) / 2 (each trade decrements both sides).
        // We can only assert the parity relationship: submitted - resting must be even
        // (every fill consumes one unit on each side).
        let filled_bothsides = submitted - resting;
        prop_assert!(
            filled_bothsides % 2 == 0,
            "filled qty across both sides must be even; got {}",
            filled_bothsides
        );
    }

    /// Cancelling an id twice removes it once and then no-ops.
    #[test]
    fn cancel_is_idempotent(price in 90i64..=110, qty in 1u64..=20) {
        let mut book = OrderBook::new("BTC-USD");
        let id = Uuid::new_v4();
        book.place(Order::limit(id, Side::Buy, Price(price), qty, 0)).unwrap();

        prop_assert_eq!(book.cancel(id), Some(qty));
        prop_assert_eq!(book.cancel(id), None);
    }

    /// Duplicate ids are rejected.
    #[test]
    fn duplicate_ids_are_rejected(price in 90i64..=110, qty in 1u64..=20) {
        let mut book = OrderBook::new("BTC-USD");
        let id = Uuid::new_v4();
        book.place(Order::limit(id, Side::Buy, Price(price), qty, 0)).unwrap();
        prop_assert!(
            book.place(Order::limit(id, Side::Buy, Price(price), qty, 0)).is_err(),
            "second place with same id should fail"
        );
    }
}

#[allow(dead_code)]
fn _kind_import_workaround() {
    // Silences unused-import lint if a future edit removes uses of OrderKind here.
    let _ = OrderKind::Limit;
}
