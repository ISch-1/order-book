use serde::{Deserialize, Serialize};

use super::{order::Order, order::OrderId, trade::Trade};

/// Everything the engine does that a persistence layer or a market-data feed
/// might care about. Producing these makes event sourcing (ADR 0004) a matter
/// of "append this to disk", not "re-derive the state change".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineEvent {
    OrderPlaced { order: Order },
    OrderCancelled { id: OrderId, remaining_qty: u64 },
    Traded(Trade),
    /// A market order that could not fully cross was dropped for the residual.
    MarketResidualDropped { id: OrderId, dropped_qty: u64 },
}
