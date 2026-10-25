use tokio::sync::oneshot;

use crate::engine::{
    book::{BookSnapshot, PlaceOutcome},
    error::EngineError,
    order::{Order, OrderId},
};

/// One-shot reply channel type alias. The actor writes the result, then drops.
type Reply<T> = oneshot::Sender<T>;

/// Every command the [`crate::actor::SymbolActor`] understands.
///
/// Callers construct a command with a `oneshot::Sender`, `.send()` the whole
/// message to the actor's `mpsc::Sender`, and await the receiver half. The
/// actor is the sole mutator of its book.
#[derive(Debug)]
pub enum Command {
    Place {
        order: Order,
        reply: Reply<Result<PlaceOutcome, EngineError>>,
    },
    Cancel {
        id: OrderId,
        reply: Reply<Option<u64>>,
    },
    Snapshot {
        depth: usize,
        reply: Reply<BookSnapshot>,
    },
    /// Graceful shutdown — the actor loop returns after handling this.
    Shutdown { reply: Reply<()> },
}
