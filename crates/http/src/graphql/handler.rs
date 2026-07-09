//! /api/graphql endpoint. POST executes; GET serves GraphiQL when docs_enabled.

use crate::state::AppState;
use async_graphql::http::GraphiQLSource;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use ferrum_core::Principal;

/// POST /api/graphql — execute a query/mutation. AppState + Principal are
/// injected into the request so resolvers reach them via ctx.data.
pub async fn execute(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    req: GraphQLRequest,
) -> Response {
    let Some(schema) = state.gql.current().await else {
        return (StatusCode::SERVICE_UNAVAILABLE, "graphql schema not built").into_response();
    };
    let request = req.into_inner().data(state.clone()).data(principal);
    let resp: GraphQLResponse = schema.execute(request).await.into();
    resp.into_response()
}

/// GET /api/graphql — GraphiQL playground (mounted only when docs_enabled).
pub async fn playground() -> impl IntoResponse {
    Html(GraphiQLSource::build().endpoint("/api/graphql").finish())
}
