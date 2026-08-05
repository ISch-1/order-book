use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::{oneshot, RwLock};
use tonic::{Request, Response, Status};
use tracing::instrument;
use uuid::Uuid;

use crate::{
    actor::{command::Command, symbol_actor::SymbolHandle},
    engine::{
        order::{Order, OrderKind, Side},
        price::Price,
    },
    proto::{
        order_book_service_server::OrderBookService, CancelOrderRequest, CancelOrderResponse,
        GetTopOfBookRequest, Level, PlaceOrderRequest, PlaceOrderResponse, Side as PbSide,
        TopOfBookSnapshot, Trade as PbTrade,
    },
};

/// Registry maps symbol → actor handle. Actors are created lazily on the first
/// request for a symbol, so callers don't need to pre-declare them.
#[derive(Clone, Default)]
pub struct SymbolRegistry {
    inner: Arc<RwLock<HashMap<String, SymbolHandle>>>,
}

impl SymbolRegistry {
    /// Return the handle for `symbol`, spawning a new actor if this is the
    /// first time we see it.
    pub async fn get_or_spawn(&self, symbol: &str) -> SymbolHandle {
        {
            let g = self.inner.read().await;
            if let Some(h) = g.get(symbol) {
                return h.clone();
            }
        }
        let mut g = self.inner.write().await;
        g.entry(symbol.to_string())
            .or_insert_with(|| crate::actor::symbol_actor::SymbolActor::spawn(symbol, 1024, 1024))
            .clone()
    }
}

/// gRPC surface. Every RPC translates its request into a `Command`, awaits
/// the actor's reply, and translates the reply back into a proto message.
pub struct OrderBookGrpc {
    pub registry: SymbolRegistry,
}

impl OrderBookGrpc {
    pub fn new(registry: SymbolRegistry) -> Self {
        Self { registry }
    }
}

#[tonic::async_trait]
impl OrderBookService for OrderBookGrpc {
    #[instrument(skip(self, req))]
    async fn place_order(
        &self,
        req: Request<PlaceOrderRequest>,
    ) -> Result<Response<PlaceOrderResponse>, Status> {
        let r = req.into_inner();

        if r.quantity == 0 {
            return Err(Status::invalid_argument("quantity must be > 0"));
        }
        let side = match PbSide::try_from(r.side).unwrap_or(PbSide::Unspecified) {
            PbSide::Buy => Side::Buy,
            PbSide::Sell => Side::Sell,
            _ => return Err(Status::invalid_argument("side must be BUY or SELL")),
        };

        let order = match crate::proto::OrderType::try_from(r.order_type).unwrap_or_default() {
            crate::proto::OrderType::Limit => {
                Order::limit(Uuid::new_v4(), side, Price(r.price_ticks), r.quantity, Utc::now().timestamp_micros())
            }
            crate::proto::OrderType::Market => {
                Order::market(Uuid::new_v4(), side, r.quantity, Utc::now().timestamp_micros())
            }
            _ => return Err(Status::invalid_argument("order_type must be LIMIT or MARKET")),
        };
        let _ = OrderKind::Limit;   // silence unused-import warning if pruned

        let handle = self.registry.get_or_spawn(&r.symbol).await;
        let (tx, rx) = oneshot::channel();
        handle
            .tx
            .send(Command::Place { order: order.clone(), reply: tx })
            .await
            .map_err(|_| Status::internal("actor mailbox closed"))?;

        let outcome = rx
            .await
            .map_err(|_| Status::internal("actor dropped reply"))?
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let trades: Vec<PbTrade> = outcome
            .trades
            .into_iter()
            .map(|t| PbTrade {
                taker_order_id: t.taker.to_string(),
                maker_order_id: t.maker.to_string(),
                price_ticks: t.price.0,
                quantity: t.quantity,
                ts_epoch_micros: t.ts_micros,
            })
            .collect();

        Ok(Response::new(PlaceOrderResponse {
            order_id: outcome.order_id.to_string(),
            filled_qty: outcome.filled_qty,
            remaining_qty: outcome.remaining_qty,
            trades,
        }))
    }

    #[instrument(skip(self, req))]
    async fn cancel_order(
        &self,
        req: Request<CancelOrderRequest>,
    ) -> Result<Response<CancelOrderResponse>, Status> {
        let r = req.into_inner();
        let id = Uuid::parse_str(&r.order_id)
            .map_err(|_| Status::invalid_argument("order_id must be a UUID"))?;

        let handle = self.registry.get_or_spawn(&r.symbol).await;
        let (tx, rx) = oneshot::channel();
        handle
            .tx
            .send(Command::Cancel { id, reply: tx })
            .await
            .map_err(|_| Status::internal("actor mailbox closed"))?;

        let removed = rx.await.map_err(|_| Status::internal("actor dropped reply"))?;
        Ok(Response::new(CancelOrderResponse {
            removed: removed.is_some(),
            remaining_qty: removed.unwrap_or(0),
        }))
    }

    #[instrument(skip(self, req))]
    async fn get_top_of_book(
        &self,
        req: Request<GetTopOfBookRequest>,
    ) -> Result<Response<TopOfBookSnapshot>, Status> {
        let r = req.into_inner();
        let depth = r.depth.clamp(1, 100) as usize;

        let handle = self.registry.get_or_spawn(&r.symbol).await;
        let (tx, rx) = oneshot::channel();
        handle
            .tx
            .send(Command::Snapshot { depth, reply: tx })
            .await
            .map_err(|_| Status::internal("actor mailbox closed"))?;

        let snap = rx.await.map_err(|_| Status::internal("actor dropped reply"))?;
        Ok(Response::new(TopOfBookSnapshot {
            symbol: snap.symbol,
            bids: snap.bids.into_iter().map(to_pb_level).collect(),
            asks: snap.asks.into_iter().map(to_pb_level).collect(),
            seq: snap.seq,
        }))
    }
}

fn to_pb_level(l: crate::engine::book::LevelSnapshot) -> Level {
    Level {
        price_ticks: l.price.0,
        quantity: l.quantity,
        orders: l.orders,
    }
}
