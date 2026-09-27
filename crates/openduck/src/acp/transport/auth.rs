use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use subtle::ConstantTimeEq;

pub fn token_matches(candidate: Option<&str>, expected: &str) -> bool {
    candidate
        .map(|key| bool::from(key.as_bytes().ct_eq(expected.as_bytes())))
        .unwrap_or(false)
}

pub async fn check_acp_token(
    State(state): State<String>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if request.method() == axum::http::Method::OPTIONS {
        return Ok(next.run(request).await);
    }

    let header_token = request
        .headers()
        .get("X-Secret-Key")
        .and_then(|value| value.to_str().ok());

    let query_token = request.uri().query().and_then(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.into_owned())
    });

    if token_matches(header_token, &state) || token_matches(query_token.as_deref(), &state) {
        Ok(next.run(request).await)
    } else {
        tracing::warn!(
            target: "goose::serve::auth",
            method = %request.method(),
            path = %request.uri().path(),
            "ACP authentication failed: missing or invalid secret key"
        );
        Err(StatusCode::UNAUTHORIZED)
    }
}
