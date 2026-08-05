use serde::{Deserialize, Serialize};

/// A price expressed in integer ticks. One tick = the smallest tradeable
/// increment for the symbol. Never a floating-point number in the engine.
///
/// See [ADR 0002](../../../docs/adr/0002-integer-ticks-for-price.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Price(pub i64);

impl Price {
    #[inline]
    pub const fn ticks(&self) -> i64 {
        self.0
    }
}

impl std::fmt::Display for Price {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<i64> for Price {
    #[inline]
    fn from(v: i64) -> Self {
        Price(v)
    }
}
