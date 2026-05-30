use axum::body::Body;
use axum::extract::Json as AxumJson;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router as AxumRouter};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use srvcs_root::{api::Deps, health, router, telemetry};
use tower::ServiceExt;

const DEAD_URL: &str = "http://127.0.0.1:1";

// --- Computing mocks for each dependency. ---
//
// Each mock genuinely computes its operation so the composition is tested
// against real answers rather than canned values.

/// Mock `srvcs-floatadd`: `{"a", "b"}` -> `{"result": a + b}` (f64).
#[allow(dead_code)]
async fn spawn_floatadd() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let a = body.get("a").and_then(Value::as_f64).unwrap_or(0.0);
            let b = body.get("b").and_then(Value::as_f64).unwrap_or(0.0);
            Json(json!({ "result": a + b }))
        }),
    );
    serve(app).await
}

/// Mock `srvcs-floatmultiply`: `{"a", "b"}` -> `{"result": a * b}` (f64).
#[allow(dead_code)]
async fn spawn_floatmultiply() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let a = body.get("a").and_then(Value::as_f64).unwrap_or(0.0);
            let b = body.get("b").and_then(Value::as_f64).unwrap_or(0.0);
            Json(json!({ "result": a * b }))
        }),
    );
    serve(app).await
}

/// Mock `srvcs-floatdivide`: `{"a", "b"}` -> `{"result": a / b}` (f64), or
/// `422` on divide-by-zero.
async fn spawn_floatdivide() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let a = body.get("a").and_then(Value::as_f64).unwrap_or(0.0);
            let b = body.get("b").and_then(Value::as_f64).unwrap_or(1.0);
            if b == 0.0 {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({ "error": "divide by zero" })),
                );
            }
            (StatusCode::OK, Json(json!({ "result": a / b })))
        }),
    );
    serve(app).await
}

/// Mock `srvcs-floatsubtract`: `{"a", "b"}` -> `{"result": a - b}` (f64).
#[allow(dead_code)]
async fn spawn_floatsubtract() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let a = body.get("a").and_then(Value::as_f64).unwrap_or(0.0);
            let b = body.get("b").and_then(Value::as_f64).unwrap_or(0.0);
            Json(json!({ "result": a - b }))
        }),
    );
    serve(app).await
}

/// Mock `srvcs-floatpower`: `{"base", "exp"}` -> `{"result": base ^ exp}`
/// (f64), or `422` if the result is not real (NaN — e.g. an even root of a
/// negative base).
async fn spawn_floatpower() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let base = body.get("base").and_then(Value::as_f64).unwrap_or(0.0);
            let exp = body.get("exp").and_then(Value::as_f64).unwrap_or(0.0);
            let result = base.powf(exp);
            if !result.is_finite() {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({ "error": "result is not real" })),
                );
            }
            (StatusCode::OK, Json(json!({ "result": result })))
        }),
    );
    serve(app).await
}

/// Mock `srvcs-ln`: `{"value"}` -> `{"result": ln(value)}` (f64).
#[allow(dead_code)]
async fn spawn_ln() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let value = body.get("value").and_then(Value::as_f64).unwrap_or(0.0);
            Json(json!({ "result": value.ln() }))
        }),
    );
    serve(app).await
}

/// Mock `srvcs-multiply`: `{"a", "b"}` -> `{"result": a * b}` (i64).
#[allow(dead_code)]
async fn spawn_multiply() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let a = body.get("a").and_then(Value::as_i64).unwrap_or(0);
            let b = body.get("b").and_then(Value::as_i64).unwrap_or(0);
            Json(json!({ "result": a * b }))
        }),
    );
    serve(app).await
}

/// Mock `srvcs-reciprocal`: `{"value"}` -> `{"result": 1 / value}` (f64).
#[allow(dead_code)]
async fn spawn_reciprocal() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let value = body.get("value").and_then(Value::as_f64).unwrap_or(1.0);
            Json(json!({ "result": 1.0 / value }))
        }),
    );
    serve(app).await
}

/// Mock `srvcs-root`: `{"value", "n"}` -> `{"result": value ^ (1 / n)}` (f64).
#[allow(dead_code)]
async fn spawn_root() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let value = body.get("value").and_then(Value::as_f64).unwrap_or(0.0);
            let n = body.get("n").and_then(Value::as_f64).unwrap_or(1.0);
            Json(json!({ "result": value.powf(1.0 / n) }))
        }),
    );
    serve(app).await
}

/// Spawn a mock returning a fixed status + body (used for error-path tests).
async fn spawn_fixed(status: StatusCode, body: Value) -> String {
    let app = AxumRouter::new().route(
        "/",
        post(move || {
            let body = body.clone();
            async move { (status, Json(body)) }
        }),
    );
    serve(app).await
}

async fn serve(app: AxumRouter) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn app(floatpower_url: &str, floatdivide_url: &str) -> axum::Router {
    router(
        telemetry::metrics_handle_for_tests(),
        Deps {
            floatpower_url: floatpower_url.to_string(),
            floatdivide_url: floatdivide_url.to_string(),
        },
    )
}

async fn root(
    floatpower_url: &str,
    floatdivide_url: &str,
    value: f64,
    n: f64,
) -> (StatusCode, Value) {
    let res = app(floatpower_url, floatdivide_url)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "value": value, "n": n }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn status_of(uri: &str) -> StatusCode {
    app(DEAD_URL, DEAD_URL)
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

fn result_f64(body: &Value) -> f64 {
    body["result"].as_f64().expect("result is a JSON number")
}

// --- Standard endpoints. ---

#[tokio::test]
async fn healthz_ok() {
    assert_eq!(status_of("/healthz").await, StatusCode::OK);
}

#[tokio::test]
async fn readyz_reflects_state() {
    health::set_ready(true);
    assert_eq!(status_of("/readyz").await, StatusCode::OK);
}

#[tokio::test]
async fn metrics_ok() {
    assert_eq!(status_of("/metrics").await, StatusCode::OK);
}

#[tokio::test]
async fn openapi_ok() {
    assert_eq!(status_of("/openapi.json").await, StatusCode::OK);
}

#[tokio::test]
async fn generates_request_id_when_absent() {
    let res = app(DEAD_URL, DEAD_URL)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        res.headers().contains_key("x-request-id"),
        "response must carry a generated x-request-id"
    );
}

#[tokio::test]
async fn index_reports_identity() {
    let res = app(DEAD_URL, DEAD_URL)
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["service"], "srvcs-root");
    assert_eq!(body["concern"], "arithmetic: nth root of value");
    assert_eq!(
        body["depends_on"],
        json!(["srvcs-floatpower", "srvcs-floatdivide"])
    );
}

// --- Correctness cases, against the computing mocks. ---

#[tokio::test]
async fn cube_root_of_27_is_3() {
    let (p, d) = (spawn_floatpower().await, spawn_floatdivide().await);
    let (status, body) = root(&p, &d, 27.0, 3.0).await;
    assert_eq!(status, StatusCode::OK);
    // exp = 1/3; 27 ^ (1/3) = 3
    assert!((result_f64(&body) - 3.0).abs() < 1e-9);
}

#[tokio::test]
async fn square_root_of_16_is_4() {
    let (p, d) = (spawn_floatpower().await, spawn_floatdivide().await);
    let (status, body) = root(&p, &d, 16.0, 2.0).await;
    assert_eq!(status, StatusCode::OK);
    // exp = 1/2; 16 ^ (1/2) = 4
    assert!((result_f64(&body) - 4.0).abs() < 1e-9);
}

#[tokio::test]
async fn square_root_of_2_is_irrational() {
    let (p, d) = (spawn_floatpower().await, spawn_floatdivide().await);
    let (status, body) = root(&p, &d, 2.0, 2.0).await;
    assert_eq!(status, StatusCode::OK);
    let expected = 2.0_f64.powf(0.5);
    assert!((result_f64(&body) - expected).abs() < 1e-9);
    // echoed inputs
    assert!((body["value"].as_f64().unwrap() - 2.0).abs() < 1e-9);
    assert!((body["n"].as_f64().unwrap() - 2.0).abs() < 1e-9);
}

#[tokio::test]
async fn fifth_root_of_32_is_2() {
    let (p, d) = (spawn_floatpower().await, spawn_floatdivide().await);
    let (status, body) = root(&p, &d, 32.0, 5.0).await;
    assert_eq!(status, StatusCode::OK);
    assert!((result_f64(&body) - 2.0).abs() < 1e-9);
}

// --- Error / degraded paths. ---

#[tokio::test]
async fn degrades_when_floatdivide_unreachable() {
    let p = spawn_floatpower().await;
    let (status, body) = root(&p, DEAD_URL, 27.0, 3.0).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["dependency"], "srvcs-floatdivide");
}

#[tokio::test]
async fn degrades_when_floatpower_unreachable() {
    // floatdivide is reachable, so the pipeline reaches the floatpower call.
    let d = spawn_floatdivide().await;
    let (status, body) = root(DEAD_URL, &d, 27.0, 3.0).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["dependency"], "srvcs-floatpower");
}

#[tokio::test]
async fn forwards_422_from_floatdivide() {
    // n = 0 -> floatdivide rejects 1/0 -> forward 422.
    let p = spawn_floatpower().await;
    let d = spawn_floatdivide().await;
    let (status, _) = root(&p, &d, 27.0, 0.0).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn forwards_422_from_floatpower_when_not_real() {
    // even root of a negative base is not real -> floatpower 422 -> forwarded.
    let (p, d) = (spawn_floatpower().await, spawn_floatdivide().await);
    let (status, _) = root(&p, &d, -16.0, 2.0).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn malformed_floatdivide_result_is_500() {
    // floatdivide answers 200 but with no numeric result -> 500.
    let p = spawn_floatpower().await;
    let d = spawn_fixed(StatusCode::OK, json!({ "result": "not-a-number" })).await;
    let (status, body) = root(&p, &d, 27.0, 3.0).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["dependency"], "srvcs-floatdivide");
}

#[tokio::test]
async fn malformed_floatpower_result_is_500() {
    let p = spawn_fixed(StatusCode::OK, json!({ "result": "not-a-number" })).await;
    let d = spawn_floatdivide().await;
    let (status, body) = root(&p, &d, 27.0, 3.0).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["dependency"], "srvcs-floatpower");
}
