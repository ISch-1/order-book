//! End-to-end test of the actor + book pipeline (without gRPC or telemetry).
//!
//! Spawns a `SymbolActor`, drives it through the mpsc command channel, and
//! observes fills and cancels via the reply channels + the broadcast event stream.

use order_book::{
    actor::{command::Command, symbol_actor::SymbolActor},
    engine::{order::{Order, Side}, price::Price},
};
use tokio::sync::oneshot;
use uuid::Uuid;

fn ts(n: i64) -> i64 { n }

async fn place(
    tx: &tokio::sync::mpsc::Sender<Command>,
    order: Order,
) -> order_book::engine::book::PlaceOutcome {
    let (rtx, rrx) = oneshot::channel();
    tx.send(Command::Place { order, reply: rtx }).await.unwrap();
    rrx.await.unwrap().unwrap()
}

async fn cancel(tx: &tokio::sync::mpsc::Sender<Command>, id: Uuid) -> Option<u64> {
    let (rtx, rrx) = oneshot::channel();
    tx.send(Command::Cancel { id, reply: rtx }).await.unwrap();
    rrx.await.unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn end_to_end_place_match_cancel_via_actor() {
    let handle = SymbolActor::spawn("BTC-USD", 64, 64);
    let tx = handle.tx.clone();
    let mut events = handle.events.subscribe();

    // Two sells rest on the book.
    let sell_a = Uuid::new_v4();
    let sell_b = Uuid::new_v4();
    let _ = place(&tx, Order::limit(sell_a, Side::Sell, Price(101), 3, ts(1))).await;
    let _ = place(&tx, Order::limit(sell_b, Side::Sell, Price(102), 10, ts(2))).await;

    // A market buy sweeps both.
    let outcome = place(&tx, Order::market(Uuid::new_v4(), Side::Buy, 7, ts(3))).await;
    assert_eq!(outcome.filled_qty, 7);
    assert_eq!(outcome.trades.len(), 2);

    // Cancel the residual on the ask side (6 units left on sell_b).
    let removed = cancel(&tx, sell_b).await;
    assert_eq!(removed, Some(6));

    // We should have seen order placed and traded events.
    let mut placed = 0;
    let mut traded = 0;
    while let Ok(ev) = events.try_recv() {
        match ev {
            order_book::engine::events::EngineEvent::OrderPlaced { .. } => placed += 1,
            order_book::engine::events::EngineEvent::Traded(_) => traded += 1,
            _ => {}
        }
    }
    assert!(placed >= 3, "expected 3 placed events, got {placed}");
    assert!(traded >= 2, "expected 2 trade events, got {traded}");
}

#[tokio::test(flavor = "current_thread")]
async fn snapshot_reflects_current_top_of_book() {
    let handle = SymbolActor::spawn("BTC-USD", 64, 64);
    let tx = handle.tx.clone();

    // 3 bids, 2 asks
    for (price, qty) in [(99, 1), (100, 2), (101, 3)] {
        place(&tx, Order::limit(Uuid::new_v4(), Side::Buy, Price(price), qty, ts(0))).await;
    }
    for (price, qty) in [(102, 5), (103, 6)] {
        place(&tx, Order::limit(Uuid::new_v4(), Side::Sell, Price(price), qty, ts(0))).await;
    }

    let (rtx, rrx) = oneshot::channel();
    tx.send(Command::Snapshot { depth: 5, reply: rtx }).await.unwrap();
    let snap = rrx.await.unwrap();

    assert_eq!(snap.bids.len(), 3);
    assert_eq!(snap.bids[0].price, Price(101), "best bid first");
    assert_eq!(snap.bids[2].price, Price(99));

    assert_eq!(snap.asks.len(), 2);
    assert_eq!(snap.asks[0].price, Price(102), "best ask first");
}
