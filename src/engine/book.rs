//! The `OrderBook` type: pure, single-threaded, testable.
//!
//! Concurrency lives one layer up in [`crate::actor::SymbolActor`]. Here we
//! keep the state machine synchronous so unit and property tests can pound
//! it without a runtime.

use std::collections::{BTreeMap, HashMap, VecDeque};

use super::{
    error::EngineError,
    events::EngineEvent,
    order::{Order, OrderId, OrderKind, Side},
    price::Price,
    trade::Trade,
};

/// Aggregated view of the book — one snapshot returned to callers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookSnapshot {
    pub symbol: String,
    pub seq: i64,
    pub bids: Vec<LevelSnapshot>, // descending by price
    pub asks: Vec<LevelSnapshot>, // ascending by price
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LevelSnapshot {
    pub price: Price,
    pub quantity: u64,
    pub orders: u32,
}

/// Outcome of a `place` call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaceOutcome {
    pub order_id: OrderId,
    pub filled_qty: u64,
    pub remaining_qty: u64,
    pub trades: Vec<Trade>,
    pub events: Vec<EngineEvent>,
}

/// Bookkeeping entry that lets us cancel by id in O(log n + k) where k is the
/// number of orders sitting at the same price level.
#[derive(Clone, Copy, Debug)]
struct OrderLocation {
    side: Side,
    price: Price,
}

/// The book. One instance per symbol; owned by exactly one actor task.
///
/// * Bids are stored so the best (highest) is at the map's last entry.
/// * Asks are stored so the best (lowest) is at the map's first entry.
/// * Within a price level, orders are a `VecDeque` — FIFO time priority.
#[derive(Debug)]
pub struct OrderBook {
    symbol: String,
    seq: i64,
    bids: BTreeMap<Price, VecDeque<Order>>,
    asks: BTreeMap<Price, VecDeque<Order>>,
    index: HashMap<OrderId, OrderLocation>,
}

impl OrderBook {
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            seq: 0,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            index: HashMap::new(),
        }
    }

    #[inline]
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    #[inline]
    pub fn seq(&self) -> i64 {
        self.seq
    }

    /// Highest resting bid, or `None` if the bid side is empty.
    pub fn best_bid(&self) -> Option<Price> {
        self.bids.keys().next_back().copied()
    }

    /// Lowest resting ask, or `None`.
    pub fn best_ask(&self) -> Option<Price> {
        self.asks.keys().next().copied()
    }

    /// Total number of resting orders on the bid side. O(levels).
    pub fn bid_order_count(&self) -> usize {
        self.bids.values().map(VecDeque::len).sum()
    }

    /// Total number of resting orders on the ask side. O(levels).
    pub fn ask_order_count(&self) -> usize {
        self.asks.values().map(VecDeque::len).sum()
    }

    /// Submit an order. Returns the fills produced plus the residual state.
    pub fn place(&mut self, mut order: Order) -> Result<PlaceOutcome, EngineError> {
        Self::validate(&order)?;
        if self.index.contains_key(&order.id) {
            return Err(EngineError::DuplicateOrderId);
        }

        let mut trades = Vec::new();
        let mut events = Vec::new();
        let order_id = order.id;

        events.push(EngineEvent::OrderPlaced { order: order.clone() });
        self.seq = self.seq.saturating_add(1);

        self.match_against(&mut order, &mut trades, &mut events);

        // Handle residual.
        let filled_qty = order.filled_qty();
        let remaining_qty = order.quantity;
        if remaining_qty > 0 {
            match order.kind {
                OrderKind::Limit => {
                    let price = order.price.expect("limit orders have a price");
                    self.rest(order);
                    // Index is written by `rest`.
                    let _ = price; // kept for readability
                }
                OrderKind::Market => {
                    events.push(EngineEvent::MarketResidualDropped {
                        id: order_id,
                        dropped_qty: remaining_qty,
                    });
                }
            }
        }

        Ok(PlaceOutcome {
            order_id,
            filled_qty,
            remaining_qty: if order.kind == OrderKind::Market { 0 } else { remaining_qty },
            trades,
            events,
        })
    }

    /// Cancel a resting order by id. Returns the remaining quantity that was
    /// removed, or `None` if the id was not on the book.
    pub fn cancel(&mut self, id: OrderId) -> Option<u64> {
        let loc = self.index.remove(&id)?;
        let map = match loc.side {
            Side::Buy => &mut self.bids,
            Side::Sell => &mut self.asks,
        };

        let queue = map.get_mut(&loc.price)?;
        let pos = queue.iter().position(|o| o.id == id)?;
        let order = queue.remove(pos).expect("index says pos exists");
        if queue.is_empty() {
            map.remove(&loc.price);
        }
        self.seq = self.seq.saturating_add(1);
        Some(order.quantity)
    }

    /// Aggregated snapshot up to `depth` levels per side (0 → top-of-book only).
    pub fn snapshot(&self, depth: usize) -> BookSnapshot {
        let n = depth.max(1);
        let bids: Vec<LevelSnapshot> = self.bids.iter().rev().take(n).map(level_snapshot).collect();
        let asks: Vec<LevelSnapshot> = self.asks.iter().take(n).map(level_snapshot).collect();
        BookSnapshot {
            symbol: self.symbol.clone(),
            seq: self.seq,
            bids,
            asks,
        }
    }

    // ─────────────────────────── internals ───────────────────────────

    fn validate(order: &Order) -> Result<(), EngineError> {
        if order.quantity == 0 {
            return Err(EngineError::ZeroQuantity);
        }
        match order.kind {
            OrderKind::Limit => {
                if order.price.is_none() {
                    return Err(EngineError::LimitWithoutPrice);
                }
            }
            OrderKind::Market => {
                if order.price.is_some() {
                    return Err(EngineError::MarketWithPrice);
                }
            }
        }
        Ok(())
    }

    /// Walk the opposite side and match. Consumes maker quantity from the
    /// front of each level's queue in FIFO order.
    fn match_against(
        &mut self,
        taker: &mut Order,
        trades: &mut Vec<Trade>,
        events: &mut Vec<EngineEvent>,
    ) {
        let opposite = taker.side.opposite();

        while taker.quantity > 0 {
            let best_price = match opposite {
                Side::Buy => self.bids.keys().next_back().copied(),
                Side::Sell => self.asks.keys().next().copied(),
            };
            let Some(price) = best_price else { break };

            // Does the taker want to cross this price?
            let can_cross = match taker.kind {
                OrderKind::Market => true,
                OrderKind::Limit => match taker.side {
                    Side::Buy => taker.price.expect("limit price") >= price,
                    Side::Sell => taker.price.expect("limit price") <= price,
                },
            };
            if !can_cross {
                break;
            }

            let map = match opposite {
                Side::Buy => &mut self.bids,
                Side::Sell => &mut self.asks,
            };
            let queue = map.get_mut(&price).expect("best_price found → level exists");

            while taker.quantity > 0 {
                let Some(maker) = queue.front_mut() else { break };

                let fill = taker.quantity.min(maker.quantity);
                let trade = Trade {
                    taker: taker.id,
                    maker: maker.id,
                    price,
                    quantity: fill,
                    ts_micros: taker.ts_micros,
                };
                trades.push(trade.clone());
                events.push(EngineEvent::Traded(trade));

                taker.quantity -= fill;
                maker.quantity -= fill;

                if maker.quantity == 0 {
                    let filled_id = maker.id;
                    queue.pop_front();
                    self.index.remove(&filled_id);
                }
            }

            if queue.is_empty() {
                map.remove(&price);
            }
        }
    }

    fn rest(&mut self, order: Order) {
        let price = order.price.expect("rest only takes priced orders");
        let map = match order.side {
            Side::Buy => &mut self.bids,
            Side::Sell => &mut self.asks,
        };
        self.index.insert(order.id, OrderLocation { side: order.side, price });
        map.entry(price).or_default().push_back(order);
    }
}

fn level_snapshot((price, queue): (&Price, &VecDeque<Order>)) -> LevelSnapshot {
    LevelSnapshot {
        price: *price,
        quantity: queue.iter().map(|o| o.quantity).sum(),
        orders: queue.len() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn ts(n: i64) -> i64 { n }

    #[test]
    fn empty_book_has_no_prices() {
        let b = OrderBook::new("BTC-USD");
        assert!(b.best_bid().is_none());
        assert!(b.best_ask().is_none());
    }

    #[test]
    fn limit_orders_rest_when_not_crossing() {
        let mut b = OrderBook::new("BTC-USD");
        let out = b
            .place(Order::limit(Uuid::new_v4(), Side::Buy, Price(100), 5, ts(1)))
            .unwrap();
        assert!(out.trades.is_empty());
        assert_eq!(out.remaining_qty, 5);
        assert_eq!(b.best_bid(), Some(Price(100)));
        assert!(b.best_ask().is_none());
    }

    #[test]
    fn market_buy_fills_against_best_ask_first() {
        let mut b = OrderBook::new("BTC-USD");
        b.place(Order::limit(Uuid::new_v4(), Side::Sell, Price(101), 3, ts(1))).unwrap();
        b.place(Order::limit(Uuid::new_v4(), Side::Sell, Price(102), 10, ts(2))).unwrap();

        let out = b
            .place(Order::market(Uuid::new_v4(), Side::Buy, 7, ts(3)))
            .unwrap();
        assert_eq!(out.filled_qty, 7);
        assert_eq!(out.trades.len(), 2);
        assert_eq!(out.trades[0].price, Price(101));
        assert_eq!(out.trades[0].quantity, 3);
        assert_eq!(out.trades[1].price, Price(102));
        assert_eq!(out.trades[1].quantity, 4);
        assert_eq!(b.best_ask(), Some(Price(102)));
    }

    #[test]
    fn fifo_within_price_level() {
        let mut b = OrderBook::new("BTC-USD");
        let a = Uuid::new_v4();
        let bb = Uuid::new_v4();
        b.place(Order::limit(a, Side::Sell, Price(100), 5, ts(1))).unwrap();
        b.place(Order::limit(bb, Side::Sell, Price(100), 5, ts(2))).unwrap();

        let out = b
            .place(Order::market(Uuid::new_v4(), Side::Buy, 6, ts(3)))
            .unwrap();
        assert_eq!(out.trades[0].maker, a);
        assert_eq!(out.trades[0].quantity, 5);
        assert_eq!(out.trades[1].maker, bb);
        assert_eq!(out.trades[1].quantity, 1);
    }

    #[test]
    fn cancel_removes_from_book() {
        let mut b = OrderBook::new("BTC-USD");
        let id = Uuid::new_v4();
        b.place(Order::limit(id, Side::Buy, Price(100), 5, ts(1))).unwrap();
        assert_eq!(b.cancel(id), Some(5));
        assert!(b.best_bid().is_none());
        assert_eq!(b.cancel(id), None, "second cancel is a no-op");
    }

    #[test]
    fn limit_partial_fill_rests_residual() {
        let mut b = OrderBook::new("BTC-USD");
        b.place(Order::limit(Uuid::new_v4(), Side::Sell, Price(100), 3, ts(1))).unwrap();
        let out = b
            .place(Order::limit(Uuid::new_v4(), Side::Buy, Price(100), 10, ts(2)))
            .unwrap();
        assert_eq!(out.filled_qty, 3);
        assert_eq!(out.remaining_qty, 7);
        assert_eq!(b.best_bid(), Some(Price(100)));
        assert!(b.best_ask().is_none());
    }

    #[test]
    fn market_order_with_no_cross_drops_residual() {
        let mut b = OrderBook::new("BTC-USD");
        let out = b
            .place(Order::market(Uuid::new_v4(), Side::Buy, 5, ts(1)))
            .unwrap();
        assert_eq!(out.filled_qty, 0);
        assert_eq!(out.remaining_qty, 0);
        assert!(matches!(
            out.events.last(),
            Some(EngineEvent::MarketResidualDropped { dropped_qty: 5, .. })
        ));
    }

    #[test]
    fn zero_quantity_is_rejected() {
        let mut b = OrderBook::new("BTC-USD");
        let err = b
            .place(Order::limit(Uuid::new_v4(), Side::Buy, Price(100), 0, ts(1)))
            .unwrap_err();
        assert_eq!(err, EngineError::ZeroQuantity);
    }
}
