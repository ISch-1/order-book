use std::net::SocketAddr;

use clap::Parser;

/// CLI + env config for the binary.
#[derive(Debug, Clone, Parser)]
#[command(name = "order-book", about = "Low-latency matching engine")]
pub struct Config {
    /// gRPC listen address.
    #[arg(long, env = "OB_GRPC_ADDR", default_value = "0.0.0.0:50051")]
    pub grpc_addr: SocketAddr,

    /// HTTP listen address for /metrics and /health.
    #[arg(long, env = "OB_HTTP_ADDR", default_value = "0.0.0.0:9100")]
    pub http_addr: SocketAddr,

    /// Comma-separated list of symbols to pre-spawn actors for. Symbols not on
    /// this list are still spawned lazily on the first request.
    #[arg(long, env = "OB_SYMBOLS", value_delimiter = ',', default_value = "BTC-USD")]
    pub symbols: Vec<String>,
}
