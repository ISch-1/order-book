//! Prometheus metrics — thin wrappers over the `metrics` crate.
//!
//! Metric names follow Prometheus conventions (`_total` on counters, base unit
//! spelled in the name for histograms). All engine metrics are prefixed `ob_`.

use std::time::Duration;

use anyhow::Context;
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};

use crate::engine::{book::PlaceOutcome, order::Side};

/// Sets up the metric registry + a `/metrics` renderer.
///
/// Returns the handle Axum will use to render.
pub fn init_metrics() -> anyhow::Result<PrometheusHandle> {
    let handle = PrometheusBuilder::new()
        // Histogram bucket boundaries chosen to make µs latency readable.
        .set_buckets_for_metric(
            Matcher::Full("ob_match_latency_micros".to_string()),
            &[
                1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1_000.0, 2_000.0, 5_000.0,
                10_000.0, 50_000.0, 100_000.0,
            ],
        )
        .context("configure histogram buckets")?
        .install_recorder()
        .context("install prometheus recorder")?;

    // Describe metrics up-front so scrapes always see them.
    metrics::describe_counter!("ob_orders_placed_total", "orders accepted by the engine");
    metrics::describe_counter!("ob_orders_rejected_total", "orders rejected by the engine");
    metrics::describe_counter!("ob_orders_cancelled_total", "cancels removed from the book");
    metrics::describe_counter!("ob_trades_executed_total", "trades emitted by matches");
    metrics::describe_counter!("ob_trades_qty_total", "sum of quantities across executed trades");
    metrics::describe_histogram!(
        "ob_match_latency_micros",
        metrics::Unit::Microseconds,
        "time from command receipt to reply (µs)"
    );
    metrics::describe_gauge!("ob_book_bid_orders", "number of resting bid orders");
    metrics::describe_gauge!("ob_book_ask_orders", "number of resting ask orders");

    Ok(handle)
}

pub fn record_place_metrics(side: Side, outcome: &PlaceOutcome, elapsed: Duration) {
    metrics::counter!("ob_orders_placed_total", "side" => side.as_str()).increment(1);
    if !outcome.trades.is_empty() {
        metrics::counter!("ob_trades_executed_total").increment(outcome.trades.len() as u64);
        let total_qty: u64 = outcome.trades.iter().map(|t| t.quantity).sum();
        metrics::counter!("ob_trades_qty_total").increment(total_qty);
    }
    let micros = elapsed.as_micros() as f64;
    metrics::histogram!("ob_match_latency_micros").record(micros);
}

pub fn cancelled_inc() {
    metrics::counter!("ob_orders_cancelled_total").increment(1);
}

pub fn rejected_inc(reason: &'static str) {
    metrics::counter!("ob_orders_rejected_total", "reason" => reason).increment(1);
}

pub fn update_book_depth(symbol: &str, side: Side, orders: usize) {
    match side {
        Side::Buy => {
            metrics::gauge!("ob_book_bid_orders", "symbol" => symbol.to_string())
                .set(orders as f64);
        }
        Side::Sell => {
            metrics::gauge!("ob_book_ask_orders", "symbol" => symbol.to_string())
                .set(orders as f64);
        }
    }
}
