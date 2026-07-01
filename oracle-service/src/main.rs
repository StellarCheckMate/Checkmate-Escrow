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

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/health", get(health_check));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8000")
        .await
        .expect("Failed to bind to port 8000");

    println!("Oracle service listening on http://127.0.0.1:8000");

    axum::serve(listener, app)
        .await
        .expect("Failed to start server");
}
