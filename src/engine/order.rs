use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::price::Price;

pub type OrderId = Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    #[inline]
    pub const fn opposite(self) -> Self {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }

    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            Side::Buy => "buy",
            Side::Sell => "sell",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderKind {
    Limit,
    Market,
}

/// An order inside the engine.
///
/// `quantity` is decremented as the order fills; `original_quantity` is
/// preserved for reporting. `price` is `None` for market orders.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub side: Side,
    pub kind: OrderKind,
    pub price: Option<Price>,
    pub quantity: u64,
    pub original_quantity: u64,
    /// Timestamp with microsecond resolution; used only for FIFO time priority.
    pub ts_micros: i64,
}

impl Order {
    /// Build a resting limit order.
    pub fn limit(id: OrderId, side: Side, price: Price, quantity: u64, ts_micros: i64) -> Self {
        Self {
            id,
            side,
            kind: OrderKind::Limit,
            price: Some(price),
            quantity,
            original_quantity: quantity,
            ts_micros,
        }
    }

    /// Build a market (immediate-or-cancel style) order.
    pub fn market(id: OrderId, side: Side, quantity: u64, ts_micros: i64) -> Self {
        Self {
            id,
            side,
            kind: OrderKind::Market,
            price: None,
            quantity,
            original_quantity: quantity,
            ts_micros,
        }
    }

    #[inline]
    pub fn is_filled(&self) -> bool {
        self.quantity == 0
    }

    #[inline]
    pub fn filled_qty(&self) -> u64 {
        self.original_quantity - self.quantity
    }
}
