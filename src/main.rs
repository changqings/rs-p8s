use std::sync::Mutex;

use actix_web::{web, App, HttpResponse, HttpServer, Responder, Result};
use prometheus_client::encoding::text::encode;
use prometheus_client::encoding::{EncodeLabelSet, EncodeLabelValue};
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::histogram::{exponential_buckets, Histogram};
use prometheus_client::registry::Registry;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelValue)]
pub enum Method {
    Get,
    Post,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct AppLabels {
    pub method: Method,
    pub script_name: String,
    pub namespace: String,
    pub app: String,
}

pub struct CountMetrics {
    requests: Family<AppLabels, Counter>,
}
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct LatencyLabels {
    pub method: Method,
    pub r#type: String,
    pub module: String,
    pub status: i8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyInfo {
    pub duration: i64,
    pub r#type: String,
    pub module: String,
    pub status: i8,
}
pub struct HisgMetrics {
    requests_hig: Family<LatencyLabels, Histogram>,
}

impl HisgMetrics {
    pub fn hisg_request(&self, ll: &LatencyLabels, d: f64) {
        self.requests_hig.get_or_create(ll).observe(d);
    }
}

impl CountMetrics {
    pub fn inc_requests(&self, app_labels: &AppLabels) {
        self.requests.get_or_create(app_labels).inc();
    }
}

pub struct AppState {
    pub registry: Registry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub script_name: String,
    pub namespace: String,
    pub app: String,
}

pub async fn metrics_handler(state: web::Data<Mutex<AppState>>) -> Result<HttpResponse> {
    let state = state.lock().unwrap();
    let mut body = String::new();
    encode(&mut body, &state.registry).unwrap();
    Ok(HttpResponse::Ok()
        .content_type("application/openmetrics-text; version=1.0.0; charset=utf-8")
        .body(body))
}

pub async fn test_handler(metrics: web::Data<CountMetrics>) -> impl Responder {
    let al = AppLabels {
        method: Method::Get,
        namespace: "test".to_string(),
        script_name: "test-script".to_string(),
        app: "test".to_string(),
    };
    metrics.inc_requests(&al);
    "okay".to_string()
}

pub async fn script_handler(
    metrics: web::Data<CountMetrics>,
    body: web::Json<AppInfo>,
) -> impl Responder {
    let al = AppLabels {
        method: Method::Post,
        namespace: body.namespace.clone(),
        script_name: body.script_name.clone(),
        app: body.app.clone(),
    };
    metrics.inc_requests(&al);
    "post_okay".to_string()
}

pub async fn duration_handler(
    metrics: web::Data<HisgMetrics>,
    body: web::Json<LatencyInfo>,
) -> impl Responder {
    let ll = LatencyLabels {
        method: Method::Post,
        r#type: body.r#type.clone(),
        module: body.module.clone(),
        status: body.status,
    };
    metrics.hisg_request(&ll, body.duration as f64);
    "post_latency_okay".to_string()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let metrics = web::Data::new(CountMetrics {
        requests: Family::default(),
    });
    let latency_metrics = web::Data::new(HisgMetrics {
        requests_hig: Family::<LatencyLabels, Histogram>::new_with_constructor(|| {
            Histogram::new(exponential_buckets(10.0, 5.0, 5))
        }),
    });
    let mut state = AppState {
        registry: Registry::default(),
    };
    state
        .registry
        .register("requests", "Count of requests", metrics.requests.clone());
    state.registry.register(
        "latency",
        "Record latency",
        latency_metrics.requests_hig.clone(),
    );
    let state = web::Data::new(Mutex::new(state));

    HttpServer::new(move || {
        App::new()
            .app_data(metrics.clone())
            .app_data(latency_metrics.clone())
            .app_data(state.clone())
            .service(web::resource("/metrics").route(web::get().to(metrics_handler)))
            .service(web::resource("/test_handler").route(web::get().to(test_handler)))
            .service(web::resource("/script_handler").route(web::post().to(script_handler)))
            .service(web::resource("/duration_handler").route(web::post().to(duration_handler)))
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
