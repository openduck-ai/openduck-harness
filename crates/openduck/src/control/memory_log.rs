use axum::{extract::Query, Json};
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct MemoryLogQuery {
    pub limit: Option<usize>,
}

pub async fn get_memory_log(
    Query(query): Query<MemoryLogQuery>,
) -> Json<crate::memory_log::MemoryLogView> {
    let limit = query.limit.unwrap_or(crate::memory_log::DEFAULT_LIMIT);
    Json(crate::memory_log::recent(limit))
}
