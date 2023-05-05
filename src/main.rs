use std::{net::SocketAddr, sync::Arc};

use axum::{extract::State, response::IntoResponse, routing::get, Router};
use nomad_vmonitor::Client;
use tracing::instrument;
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};

#[derive(Debug)]
struct AppState {
    client: Arc<Client>,
}

#[tokio::main]
async fn main() {
    let machine_log = std::env::var("LOG_MACHINE").is_ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nomad_vmonitor=info".into()),
        )
        .with((machine_log).then(|| tracing_subscriber::fmt::layer().json()))
        .with((!machine_log).then(|| tracing_subscriber::fmt::layer().pretty()))
        .init();

    let address = std::env::var("NOMAD_ADDR").unwrap_or_else(|_| "localhost".to_string());
    let port = std::env::var("NOMAD_PORT").unwrap_or_else(|_| "4646".to_string());

    let client = Arc::new(Client::new(format!("http://{address}:{port}")));

    tokio::spawn(client.clone().run());

    let app = Router::new()
        .route("/metrics", get(metrics))
        .with_state(Arc::new(AppState { client }));

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

#[instrument(skip(state))]
async fn metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.client.get_metrics()
}
