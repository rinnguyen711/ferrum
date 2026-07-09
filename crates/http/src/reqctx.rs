//! Per-request context (IP, User-Agent, request id) captured once and stored as
//! a request extension for handlers (e.g. audit logging) to read.

use axum::{extract::Request, http::HeaderMap, middleware::Next, response::Response};
use ferrum_core::RequestContext;
use uuid::Uuid;

fn client_ip(headers: &HeaderMap) -> Option<String> {
    // Prefer the first hop in X-Forwarded-For; fall back to X-Real-IP.
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = xff.split(',').next() {
            let ip = first.trim();
            if !ip.is_empty() {
                return Some(ip.to_string());
            }
        }
    }
    headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

pub async fn layer(mut req: Request, next: Next) -> Response {
    let headers = req.headers();
    let ctx = RequestContext {
        ip: client_ip(headers),
        user_agent: headers
            .get(axum::http::header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string),
        request_id: Some(Uuid::new_v4().to_string()),
    };
    req.extensions_mut().insert(ctx);
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn client_ip_prefers_first_forwarded_hop() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
        assert_eq!(client_ip(&h), Some("1.2.3.4".to_string()));
    }

    #[test]
    fn client_ip_falls_back_to_real_ip() {
        let mut h = HeaderMap::new();
        h.insert("x-real-ip", "9.9.9.9".parse().unwrap());
        assert_eq!(client_ip(&h), Some("9.9.9.9".to_string()));
    }

    #[test]
    fn client_ip_none_when_absent() {
        let h = HeaderMap::new();
        assert_eq!(client_ip(&h), None);
    }
}
