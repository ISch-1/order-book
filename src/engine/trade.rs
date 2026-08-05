use serde::{Deserialize, Serialize};

use super::{order::OrderId, price::Price};

/// A recorded fill between a taker (aggressive order) and a maker (resting
/// order). Emitted by the engine, consumed by market-data subscribers and
/// the persistence layer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trade {
    pub taker: OrderId,
    pub maker: OrderId,
    pub price: Price,
    pub quantity: u64,
    pub ts_micros: i64,
}
