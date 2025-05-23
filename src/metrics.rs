// src/metrics.rs
//
// Prometheus-Client-Integration. 
// Startet einen HTTP-Server auf /metrics-Endpunkt.

use lazy_static::lazy_static;
use prometheus::{
    IntCounter, IntGauge, Registry, Encoder, TextEncoder,
    register_int_counter, register_int_gauge
};
use hyper::{Body, Request, Response, Server};
use hyper::service::{make_service_fn, service_fn};
use std::net::SocketAddr;
use tracing::{info, error};

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();

    // Orders
    pub static ref ORDER_COUNT: IntCounter = register_int_counter!(
        "dex_order_total",
        "Total number of Orders created"
    ).unwrap();

    pub static ref ACTIVE_SWAPS: IntGauge = register_int_gauge!(
        "dex_active_swaps",
        "Number of active Atomic Swaps"
    ).unwrap();

    // Node-Starts (wenn Node re-launched etc.)
    pub static ref DEX_NODE_STARTS: IntCounter = register_int_counter!(
        "dex_node_starts_total",
        "Wie oft ein Node-Prozess startete"
    ).unwrap();

    // CRDT Merges
    pub static ref CRDT_MERGE_COUNT: IntCounter = register_int_counter!(
        "dex_crdt_merge_total",
        "Wie oft CRDT-merge_remote aufgerufen wurde"
    ).unwrap();

    // HTLC / AtomicSwap Kennzahlen
    pub static ref HTLC_REDEEM_COUNT: IntCounter = register_int_counter!(
        "dex_htlc_redeem_total",
        "Anzahl Redeems in HTLC"
    ).unwrap();

    pub static ref HTLC_REFUND_COUNT: IntCounter = register_int_counter!(
        "dex_htlc_refund_total",
        "Anzahl Refunds in HTLC"
    ).unwrap();

    pub static ref SWAP_SELLER_REDEEM_COUNT: IntCounter = register_int_counter!(
        "dex_swap_seller_redeem_total",
        "Wie oft Seller redeem auf AtomicSwap"
    ).unwrap();

    pub static ref SWAP_BUYER_REDEEM_COUNT: IntCounter = register_int_counter!(
        "dex_swap_buyer_redeem_total",
        "Wie oft Buyer redeem auf AtomicSwap"
    ).unwrap();

    pub static ref SWAP_REFUND_COUNT: IntCounter = register_int_counter!(
        "dex_swap_refund_total",
        "Wie oft AtomicSwap refund ausgeführt"
    ).unwrap();

    // partial fill
    pub static ref PARTIAL_FILL_COUNT: IntCounter = register_int_counter!(
        "dex_partial_fill_total",
        "Wie oft eine Partial-Fill Operation ausgeführt wurde"
    ).unwrap();
}

pub fn register_metrics() {
    REGISTRY.register(Box::new(ORDER_COUNT.clone())).unwrap();
    REGISTRY.register(Box::new(ACTIVE_SWAPS.clone())).unwrap();
    REGISTRY.register(Box::new(DEX_NODE_STARTS.clone())).unwrap();
    REGISTRY.register(Box::new(CRDT_MERGE_COUNT.clone())).unwrap();

    REGISTRY.register(Box::new(HTLC_REDEEM_COUNT.clone())).unwrap();
    REGISTRY.register(Box::new(HTLC_REFUND_COUNT.clone())).unwrap();
    REGISTRY.register(Box::new(SWAP_SELLER_REDEEM_COUNT.clone())).unwrap();
    REGISTRY.register(Box::new(SWAP_BUYER_REDEEM_COUNT.clone())).unwrap();
    REGISTRY.register(Box::new(SWAP_REFUND_COUNT.clone())).unwrap();

    REGISTRY.register(Box::new(PARTIAL_FILL_COUNT.clone())).unwrap();
}

pub async fn serve_metrics(addr: SocketAddr) {
    info!("Starting Prometheus metrics endpoint at {:?}", addr);

    let svc = make_service_fn(|_conn| async {
        Ok::<_, hyper::Error>(service_fn(|req: Request<Body>| async move {
            if req.uri().path() == "/metrics" {
                let metric_families = REGISTRY.gather();
                let mut buf = Vec::new();
                let encoder = TextEncoder::new();
                encoder.encode(&metric_families, &mut buf).unwrap();
                Ok::<_, hyper::Error>(Response::new(Body::from(buf)))
            } else {
                Ok::<_, hyper::Error>(
                    Response::builder().status(404).body(Body::from("Not Found")).unwrap()
                )
            }
        }))
    });

    let server = Server::bind(&addr).serve(svc);
    if let Err(e) = server.await {
        error!("Metrics server error: {:?}", e);
    }
}
