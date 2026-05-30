use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use utoipa::{OpenApi, ToSchema};

use crate::client::{self, DepError};

pub const SERVICE: &str = "srvcs-root";
pub const CONCERN: &str = "arithmetic: nth root of value";
pub const DEPENDS_ON: &[&str] = &["srvcs-floatpower", "srvcs-floatdivide"];

/// Dependency endpoints, injected as router state so tests can point them at
/// mock services.
#[derive(Clone)]
pub struct Deps {
    pub floatpower_url: String,
    pub floatdivide_url: String,
}

#[derive(Serialize, ToSchema)]
pub struct Info {
    pub service: &'static str,
    pub concern: &'static str,
    pub depends_on: Vec<&'static str>,
}

/// `GET /` — service identity (srvcs service standard).
#[utoipa::path(get, path = "/", responses((status = 200, body = Info)))]
pub async fn index() -> Json<Info> {
    Json(Info {
        service: SERVICE,
        concern: CONCERN,
        depends_on: DEPENDS_ON.to_vec(),
    })
}

#[derive(Deserialize, ToSchema)]
pub struct EvalRequest {
    pub value: f64,
    pub n: f64,
}

#[derive(Serialize, ToSchema)]
pub struct RootResponse {
    pub value: f64,
    pub n: f64,
    pub result: f64,
}

fn degraded(dependency: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "dependency unavailable", "dependency": dependency })),
    )
        .into_response()
}

fn forward(status: u16, body: Value) -> Response {
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    (code, Json(body)).into_response()
}

/// A reachable dependency answered `200` but its body lacked a numeric
/// `result`. That is a contract violation we cannot recover from, so surface a
/// `500` rather than guessing.
fn malformed(dependency: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(
            json!({ "error": "dependency returned a malformed result", "dependency": dependency }),
        ),
    )
        .into_response()
}

/// Call one dependency at `url` with `body`, mapping its outcome to either the
/// parsed response body (on `200`) or an early-return `Response` the caller
/// should surface verbatim:
///
/// - unreachable / non-`200`/`422` -> `503` degraded
/// - `422` -> forwarded `422` (the dependency rejected the input)
async fn ask(url: &str, body: &Value, dependency: &str) -> Result<Value, Response> {
    match client::call(url, body).await {
        Err(DepError::Unreachable) => Err(degraded(dependency)),
        Ok((200, body)) => Ok(body),
        Ok((422, body)) => Err(forward(422, body)),
        Ok(_) => Err(degraded(dependency)),
    }
}

/// `POST /` — compute `root(value, n)` by composing two primitives.
///
/// This service owns the *control flow* but delegates every arithmetic step to
/// its dependencies, exactly as specified:
///
/// 1. ask `srvcs-floatdivide` for `exp = 1 / n`;
/// 2. ask `srvcs-floatpower` for `result = value ^ exp` — i.e.
///    `root(value, n) = value ^ (1 / n)`.
///
/// `srvcs-floatpower` returns `422` if the result is not real (e.g. an even
/// root of a negative value); that `422` is forwarded. If a dependency is
/// unreachable it reports itself degraded (`503`).
#[utoipa::path(
    post,
    path = "/",
    request_body = EvalRequest,
    responses(
        (status = 200, body = RootResponse),
        (status = 422, description = "a dependency rejected the input (forwarded)"),
        (status = 500, description = "a dependency returned a malformed result"),
        (status = 503, description = "a dependency is unavailable")
    )
)]
pub async fn evaluate(State(deps): State<Deps>, Json(req): Json<EvalRequest>) -> Response {
    let (value, n) = (req.value, req.n);

    // 1. exp = 1 / n
    let divide_body = match ask(
        &deps.floatdivide_url,
        &json!({ "a": 1, "b": n }),
        "srvcs-floatdivide",
    )
    .await
    {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let exp = match divide_body.get("result").and_then(Value::as_f64) {
        Some(e) => e,
        None => return malformed("srvcs-floatdivide"),
    };

    // 2. result = value ^ exp
    let power_body = match ask(
        &deps.floatpower_url,
        &json!({ "base": value, "exp": exp }),
        "srvcs-floatpower",
    )
    .await
    {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let result = match power_body.get("result").and_then(Value::as_f64) {
        Some(r) => r,
        None => return malformed("srvcs-floatpower"),
    };

    (
        StatusCode::OK,
        Json(json!({ "value": value, "n": n, "result": result })),
    )
        .into_response()
}

#[derive(OpenApi)]
#[openapi(
    paths(index, evaluate),
    components(schemas(Info, EvalRequest, RootResponse))
)]
pub struct ApiDoc;

/// Serve OpenAPI document
pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_documents_routes() {
        let doc = ApiDoc::openapi();
        let root = doc.paths.paths.get("/").expect("path / present");
        assert!(root.get.is_some());
        assert!(root.post.is_some());
    }

    #[tokio::test]
    async fn index_reports_all_dependencies() {
        let Json(info) = index().await;
        assert_eq!(info.service, "srvcs-root");
        assert_eq!(info.concern, "arithmetic: nth root of value");
        assert_eq!(
            info.depends_on,
            vec!["srvcs-floatpower", "srvcs-floatdivide"]
        );
    }
}
