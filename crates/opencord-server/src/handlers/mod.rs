//! Request handlers, one module per group.

use std::sync::Arc;

use opencord_proto::v1 as proto;

use crate::error::ApiError;
use crate::gateway::session::Session;
use crate::state::AppState;

pub async fn handle(
    state: &Arc<AppState>,
    session: &Arc<Session>,
    request: proto::Request,
) -> proto::response::Result {
    match dispatch(state, session, request).await {
        Ok(result) => result,
        Err(error) => proto::response::Result::Error(error.to_proto()),
    }
}

async fn dispatch(
    _state: &Arc<AppState>,
    _session: &Arc<Session>,
    _request: proto::Request,
) -> Result<proto::response::Result, ApiError> {
    Err(ApiError::invalid_argument(
        "this request is not supported yet",
    ))
}
