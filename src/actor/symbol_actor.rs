use std::time::Instant;

use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info};

use crate::{
    actor::command::Command,
    engine::{book::OrderBook, events::EngineEvent, order::Side},
    telemetry::metrics as m,
};

/// Handle callers hold to talk to an actor without touching the task directly.
///
/// Cheap to clone — the underlying channels are `Arc`d.
#[derive(Clone)]
pub struct SymbolHandle {
    pub tx: mpsc::Sender<Command>,
    pub events: broadcast::Sender<EngineEvent>,
    pub symbol: String,
}

/// A `SymbolActor` owns exactly one `OrderBook`. The `run` method is the task
/// body: read commands, mutate state, reply.
pub struct SymbolActor {
    book: OrderBook,
    rx: mpsc::Receiver<Command>,
    events: broadcast::Sender<EngineEvent>,
}

impl SymbolActor {
    /// Spawn a fresh actor for the given symbol. Returns a handle callers use
    /// to send commands and to subscribe to the event broadcast.
    pub fn spawn(symbol: impl Into<String>, mailbox: usize, events_buffer: usize) -> SymbolHandle {
        let symbol = symbol.into();
        let (cmd_tx, cmd_rx) = mpsc::channel(mailbox);
        let (evt_tx, _) = broadcast::channel(events_buffer);

        let actor = SymbolActor {
            book: OrderBook::new(symbol.clone()),
            rx: cmd_rx,
            events: evt_tx.clone(),
        };
        let symbol_for_task = symbol.clone();
        tokio::spawn(async move { actor.run(symbol_for_task).await });

        SymbolHandle {
            tx: cmd_tx,
            events: evt_tx,
            symbol,
        }
    }

    async fn run(mut self, symbol: String) {
        info!(symbol = %symbol, "symbol actor started");
        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                Command::Place { order, reply } => {
                    let started = Instant::now();
                    let side = order.side;
                    let result = self.book.place(order);
                    let elapsed = started.elapsed();

                    if let Ok(ref out) = result {
                        // Fan out events; ignore lag errors (no subscribers, slow subscribers).
                        for ev in &out.events {
                            let _ = self.events.send(ev.clone());
                        }
                        m::record_place_metrics(side, out, elapsed);
                    } else {
                        m::rejected_inc("engine_error");
                    }
                    let _ = reply.send(result);
                }
                Command::Cancel { id, reply } => {
                    let removed = self.book.cancel(id);
                    if let Some(rem) = removed {
                        m::cancelled_inc();
                        let _ = self.events.send(EngineEvent::OrderCancelled {
                            id,
                            remaining_qty: rem,
                        });
                    }
                    let _ = reply.send(removed);
                }
                Command::Snapshot { depth, reply } => {
                    let _ = reply.send(self.book.snapshot(depth));
                }
                Command::Shutdown { reply } => {
                    let _ = reply.send(());
                    debug!(symbol = %symbol, "shutdown received, closing actor");
                    break;
                }
            }

            m::update_book_depth(&symbol, Side::Buy, self.book.bid_order_count());
            m::update_book_depth(&symbol, Side::Sell, self.book.ask_order_count());
        }
        info!(symbol = %symbol, "symbol actor stopped");
    }
}
