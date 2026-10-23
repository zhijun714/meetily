use super::{
    protocol::{Action, Request},
    CompanionState, Dispatch,
};
use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use std::{io::Write, sync::Arc};
use tauri::{Emitter, Manager};

#[derive(Clone)]
struct ServerState {
    app: tauri::AppHandle,
    bridge: CompanionState,
    token: Arc<String>,
    host: Arc<String>,
}

type ApiError = (StatusCode, Json<serde_json::Value>);
fn conflict(error: impl ToString) -> ApiError {
    (
        StatusCode::CONFLICT,
        Json(json!({"error": error.to_string()})),
    )
}

fn authorized(headers: &HeaderMap, host: &str, token: &str) -> bool {
    !headers.contains_key("origin")
        && headers.get_all("host").iter().count() == 1
        && headers.get("host").and_then(|v| v.to_str().ok()) == Some(host)
        && headers.get_all("authorization").iter().count() == 1
        && headers.get("authorization").and_then(|v| v.to_str().ok())
            == Some(format!("Bearer {token}").as_str())
}

async fn authenticate(
    State(state): State<ServerState>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if !authorized(request.headers(), &state.host, &state.token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Unauthorized"})),
        )
            .into_response();
    }
    next.run(request).await
}

async fn status(State(state): State<ServerState>) -> impl IntoResponse {
    Json(state.bridge.0.lock().unwrap().snapshot())
}

async fn operation(
    State(state): State<ServerState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let mut bridge = state.bridge.0.lock().unwrap();
    bridge.snapshot();
    bridge
        .operations
        .iter()
        .find(|op| op.operation_id == id)
        .cloned()
        .map(Json)
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Operation not found"})),
        ))
}

async fn start(
    State(state): State<ServerState>,
    Json(request): Json<Request>,
) -> Result<impl IntoResponse, ApiError> {
    submit(state, Action::Start, request)
}
async fn stop(
    State(state): State<ServerState>,
    Json(request): Json<Request>,
) -> Result<impl IntoResponse, ApiError> {
    submit(state, Action::Stop, request)
}
async fn show(
    State(state): State<ServerState>,
    Json(request): Json<Request>,
) -> Result<impl IntoResponse, ApiError> {
    submit(state, Action::Show, request)
}

fn submit(
    state: ServerState,
    action: Action,
    request: Request,
) -> Result<impl IntoResponse, ApiError> {
    let (operation, fresh) = state
        .bridge
        .0
        .lock()
        .unwrap()
        .accept(action, request)
        .map_err(conflict)?;
    if fresh {
        if action == Action::Show {
            let result = state
                .app
                .get_webview_window("main")
                .ok_or_else(|| "Main window unavailable".to_string())
                .and_then(|window| {
                    window
                        .unminimize()
                        .and_then(|_| window.show())
                        .and_then(|_| window.set_focus())
                        .map_err(|e| e.to_string())
                });
            let mut bridge = state.bridge.0.lock().unwrap();
            match result {
                Ok(()) => bridge.succeed(&operation.operation_id),
                Err(error) => bridge.fail(&operation.operation_id, error),
            }
        } else {
            let result = state.app.emit_to(
                "main",
                "companion-command",
                Dispatch {
                    operation_id: operation.operation_id.clone(),
                    action,
                },
            );
            if let Err(error) = result {
                state
                    .bridge
                    .0
                    .lock()
                    .unwrap()
                    .fail(&operation.operation_id, error.to_string());
            }
        }
    }
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"operation_id": operation.operation_id})),
    ))
}

fn publish(path: &std::path::Path, value: &serde_json::Value) -> anyhow::Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    let mut file = tempfile::NamedTempFile::new_in(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(&serde_json::to_vec(value)?)?;
    file.as_file().sync_all()?;
    file.persist(path.join("bridge.json"))?;
    Ok(())
}

pub(super) async fn serve(app: tauri::AppHandle, bridge: CompanionState) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let port = listener.local_addr()?.port();
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let instance_id = bridge.0.lock().unwrap().status.instance_id.clone();
    let state = ServerState {
        app: app.clone(),
        bridge,
        token: Arc::new(token.clone()),
        host: Arc::new(format!("127.0.0.1:{port}")),
    };
    let router = Router::new()
        .route("/v1/status", get(status))
        .route("/v1/operations/:id", get(operation))
        .route("/v1/recording/start", post(start))
        .route("/v1/recording/stop", post(stop))
        .route("/v1/window/show", post(show))
        .layer(DefaultBodyLimit::max(2048))
        .layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state);
    publish(
        &app.path().app_data_dir()?.join("companion"),
        &json!({"protocol_version":1,"port":port,"token":token,"instance_id":instance_id}),
    )?;
    let result = axum::serve(listener, router).await;
    super::cleanup(&app);
    result?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auth_rejects_origin_wrong_host_and_missing_bearer() {
        let mut h = HeaderMap::new();
        h.insert("host", "127.0.0.1:1234".parse().unwrap());
        assert!(!authorized(&h, "127.0.0.1:1234", "secret"));
        h.insert("authorization", "Bearer secret".parse().unwrap());
        assert!(authorized(&h, "127.0.0.1:1234", "secret"));
        h.insert("origin", "null".parse().unwrap());
        assert!(!authorized(&h, "127.0.0.1:1234", "secret"));
        h.remove("origin");
        assert!(!authorized(&h, "localhost:1234", "secret"));
        assert!(!authorized(&h, "127.0.0.1:1234", "wrong"));
        h.append("authorization", "Bearer secret".parse().unwrap());
        assert!(!authorized(&h, "127.0.0.1:1234", "secret"));
    }
    #[test]
    fn discovery_is_private_and_atomically_replaceable() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("companion");
        publish(&dir, &json!({"instance_id":"one"})).unwrap();
        publish(&dir, &json!({"instance_id":"two"})).unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("bridge.json")).unwrap()).unwrap();
        assert_eq!(value["instance_id"], "two");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                std::fs::metadata(dir.join("bridge.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}
