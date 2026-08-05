use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    #[error("order quantity must be > 0")]
    ZeroQuantity,

    #[error("limit orders must carry a price")]
    LimitWithoutPrice,

    #[error("market orders must not carry a price")]
    MarketWithPrice,

    #[error("duplicate order id")]
    DuplicateOrderId,
}
