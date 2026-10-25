use anyhow::Context;
use axum::{routing::get, Router};
use clap::Parser;
use tokio::signal;
use tonic::transport::Server;
use tracing::info;

use order_book::{
    config::Config,
    grpc::service::{OrderBookGrpc, SymbolRegistry},
    proto::order_book_service_server::OrderBookServiceServer,
    telemetry::{init_metrics, init_tracing},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing("order-book").context("tracing setup")?;
    let prom_handle = init_metrics().context("metrics setup")?;

    let cfg = Config::parse();
    info!(?cfg, "starting");

    // Registry + pre-spawn requested symbols so metrics show up before traffic.
    let registry = SymbolRegistry::default();
    for s in &cfg.symbols {
        registry.get_or_spawn(s).await;
    }

    // ------- gRPC server -------
    let grpc = OrderBookGrpc::new(registry.clone());
    let grpc_task = {
        let addr = cfg.grpc_addr;
        tokio::spawn(async move {
            Server::builder()
                .add_service(OrderBookServiceServer::new(grpc))
                .serve_with_shutdown(addr, shutdown_signal())
                .await
                .context("grpc server")
        })
    };
    info!(addr = %cfg.grpc_addr, "gRPC listening");

    // ------- HTTP server (metrics + health) -------
    let http_app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route(
            "/metrics",
            get(move || {
                let h = prom_handle.clone();
                async move { h.render() }
            }),
        );

    let http_task = {
        let addr = cfg.http_addr;
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(addr)
                .await
                .context("bind http")?;
            axum::serve(listener, http_app)
                .with_graceful_shutdown(shutdown_signal())
                .await
                .context("http server")
        })
    };
    info!(addr = %cfg.http_addr, "HTTP (metrics/health) listening");

    // Wait for either server to exit (should be simultaneous on shutdown).
    let (g, h) = tokio::join!(grpc_task, http_task);
    g.context("join grpc task")??;
    h.context("join http task")??;

    info!("shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    info!("shutdown signal received");
}
