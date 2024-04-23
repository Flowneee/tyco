#![allow(unused)]

use axum::{
    extract::Request,
    middleware::{self, Next},
    response::Response,
    routing::get,
    Router,
};
use reqwest::Client;
use tyco::{context, FutureExt, TypedContext};

use self::trace_id::TraceId;

mod trace_id {
    use super::*;

    pub const HEADER_NAME: &str = "trace-id";

    #[derive(Clone, Debug, PartialEq)]
    pub struct TraceId(pub String);

    impl TraceId {
        pub fn new(v: impl ToString) -> Self {
            Self(v.to_string())
        }

        pub fn from_axum_request(req: &Request) -> Option<Self> {
            let value = req
                .headers()
                .get(HEADER_NAME)
                .and_then(|x| x.to_str().ok())
                .map(|x| x.to_owned())?;
            Some(Self(value))
        }
    }

    context!(TraceId);
}

// Extraction middleware
async fn extract_tracing_id(req: Request, next: Next) -> Response {
    let trace_id = TraceId::from_axum_request(&req);
    next.run(req).with_opt(trace_id).await
}

async fn handler() {
    // Initiate "background" task, which make request to external resource
    tokio::spawn(make_request().with_current::<TraceId>());
}

async fn make_request() {
    let trace_id = TraceId::current();
    println!("Current {trace_id:?}");

    let client = Client::new();
    let mut req = client.get("https://example.com/");
    if let Some(x) = trace_id {
        req = req.header(self::trace_id::HEADER_NAME, x.0);
    }
    let resp = req.send().await;
    // do something with response
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(handler))
        .layer(middleware::from_fn(extract_tracing_id));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
