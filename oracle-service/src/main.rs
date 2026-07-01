use axum::{routing::get, Json, Router};
use serde::Serialize;
use chrono::Utc;

#[derive(Serialize)]
struct HealthStatus {
    status: String,
    network: String,
    contract_address: String,
    last_checked_at: String,
}

async fn health_check() -> Json<HealthStatus> {
    Json(HealthStatus {
        status: "healthy".to_string(),
        network: "testnet".to_string(),
        contract_address: "CB...".to_string(),
        last_checked_at: Utc::now().to_rfc3339(),
    })
}
