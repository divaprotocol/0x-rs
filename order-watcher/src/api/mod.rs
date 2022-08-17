//! Implements the SRA v4 order submit protocol
//!
//! See <https://0x.org/docs/api#post-srav4order>
//! See <https://0x.org/docs/api#post-srav4orders>

mod error;

use core::{convert::Infallible, future::Future};
use std::{net::SocketAddr, sync::Arc};

use anyhow::{Context as _, Result as AnyResult};
use hyper::{
    body::Buf as _,
    header,
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server, StatusCode,
};
use once_cell::sync::Lazy;
use prometheus::{
    exponential_buckets, register_histogram, register_int_counter, register_int_counter_vec,
    Histogram, IntCounter, IntCounterVec,
};
use serde::de::DeserializeOwned;
use serde_json::{self};
use tracing::info;

pub use self::error::Error;
use crate::{orders::SignedOrder, App};

const CONTENT_JSON: &str = "application/json";

static ORDER: Lazy<IntCounter> =
    Lazy::new(|| register_int_counter!("api_order", "Number of API /order requests.").unwrap());
static ORDERS: Lazy<Histogram> = Lazy::new(|| {
    register_histogram!(
        "api_orders",
        "Number of API /orders requests by number of orders.",
        exponential_buckets(1.0, 2.0, 10).unwrap()
    )
    .unwrap()
});
static STATUS: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "api_response_status",
        "The API responses by status code.",
        &["status_code"]
    )
    .unwrap()
});
static LATENCY: Lazy<Histogram> = Lazy::new(|| {
    register_histogram!("api_latency_seconds", "The API latency in seconds.").unwrap()
});

/// Parse a [`Request<Body>`] as JSON using Serde and handle using the provided
/// method.
async fn json_middleware<F, T, S, U>(request: Request<Body>, mut next: F) -> Result<U, Error>
where
    T: DeserializeOwned + Send,
    F: FnMut(T) -> S + Send,
    S: Future<Output = Result<U, Error>> + Send,
{
    if request.method() != Method::POST {
        return Err(Error::InvalidMethod);
    }
    let valid_content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .map_or(false, |content_type| content_type == CONTENT_JSON);
    if !valid_content_type {
        return Err(Error::InvalidContentType);
    }
    let body = hyper::body::aggregate(request).await?;
    let value = serde_json::from_reader(body.reader())?;
    next(value).await
}

/// Route requests based on path
async fn route(app: Arc<App>, request: Request<Body>) -> Result<Response<Body>, Infallible> {
    let _timer = LATENCY.start_timer(); // Observes on drop

    let response = match request.uri().path() {
        "/order" => {
            json_middleware(request, |req| {
                ORDER.inc();
                app.order(req)
            })
            .await
        }
        "/orders" => {
            json_middleware(request, |req: Vec<SignedOrder>| {
                #[allow(clippy::cast_precision_loss)]
                ORDERS.observe(req.len() as f64);
                app.orders(req)
            })
            .await
        }
        _ => Err(Error::NotFound),
    }
    .map_or_else(Error::into_response, |_| {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::OK;
        response
    });

    STATUS
        .with_label_values(&[response.status().as_str()])
        .inc();
    Ok(response)
}

/// Run a http server on [`socket_address`]
pub(super) async fn serve(app: App, socket_address: &SocketAddr) -> AnyResult<()> {
    // Wrap app in an Arc to make cloning cheaper
    let app = Arc::new(app);

    let service = make_service_fn(move |_connection| {
        let app = app.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |request| {
                let app = app.clone();
                route(app, request)
            }))
        }
    });

    let listener = Server::try_bind(socket_address)
        .with_context(|| format!("error binding {} for submit server", socket_address))?;

    let server = listener.serve(service);
    info!("Listening on http://{}", socket_address);

    // TODO: Graceful shutdown
    // See <https://hyper.rs/guides/server/graceful-shutdown/>

    // Service requests
    server
        .await
        .context("internal server error in submit RPC")?;

    Ok(())
}
