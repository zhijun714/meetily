mod protocol;
mod server;

use protocol::{Action, Bridge, Phase};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Listener, Manager};

#[derive(Clone)]
pub struct CompanionState(pub Arc<Mutex<Bridge>>, pub Arc<Mutex<Option<Arc<crate::audio::RecordingState>>>>);

#[derive(Clone, Serialize)]
struct Dispatch {
    operation_id: String,
    action: Action,
}

#[derive(Deserialize)]
pub struct FrontendReport {
    phase: Phase,
    ready: bool,
    error: Option<String>,
}

#[tauri::command]
pub fn companion_report(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, CompanionState>,
    report: FrontendReport,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Main window required".into());
    }
    state
        .0
        .lock()
        .unwrap()
        .report(report.phase, report.ready, report.error);
    Ok(())
}

#[tauri::command]
pub fn companion_fail(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, CompanionState>,
    operation_id: String,
    error: String,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Main window required".into());
    }
    state.0.lock().unwrap().fail(&operation_id, error);
    Ok(())
}

// Only the main frontend can execute an accepted operation. Transcript preparation
// follows the existing native recording-starting event after duplicate validation.
#[tauri::command]
pub async fn companion_execute(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: tauri::State<'_, CompanionState>,
    operation_id: String,
    meeting_name: Option<String>,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Main window required".into());
    }
    let native = crate::audio::recording_commands::get_recording_state().await;
    let (action, expected_session) = {
        let mut bridge = state.0.lock().unwrap();
        update_native(&mut bridge, &native);
        let action = bridge.begin_execution(&operation_id)?;
        (action, state.1.lock().unwrap().clone())
    };
    let result = async {
        match action {
            Action::Start => {
                crate::start_recording_with_devices_and_meeting(
                    app.clone(),
                    None,
                    None,
                    meeting_name,
                )
                .await
            }
            Action::Stop => {
                let path = app
                    .path()
                    .app_data_dir()
                    .map_err(|e| e.to_string())?
                    .join(format!(
                        "recording-{}.wav",
                        chrono::Local::now().format("%Y-%m-%dT%H-%M-%S")
                    ));
                let expected_session = expected_session.ok_or("Recording session unavailable")?;
                crate::audio::recording_commands::stop_recording_for_session(
                    app.clone(),
                    crate::audio::recording_commands::RecordingArgs {
                        save_path: path.to_string_lossy().into_owned(),
                    },
                    Some(expected_session),
                )
                .await
                .and_then(|_| {
                    app.emit("recording-stop-complete", true)
                        .map_err(|e| e.to_string())
                })
            }
            Action::Show => Err("Window commands are handled natively".into()),
        }
    }
    .await;
    let native = crate::audio::recording_commands::get_recording_state().await;
    let mut bridge = state.0.lock().unwrap();
    bridge.executing = None;
    update_native(&mut bridge, &native);
    match &result {
        Ok(()) if action == Action::Start => {
            if bridge.backend_recording {
                bridge.succeed(&operation_id);
            } else {
                bridge.fail(&operation_id, "Recording did not start".into());
            }
        }
        Err(error) => bridge.fail(&operation_id, error.clone()),
        _ => {} // Stop completes only when the existing frontend save flow reports completion.
    }
    result
}

fn update_native(bridge: &mut Bridge, native: &serde_json::Value) {
    let milliseconds = |key: &str| {
        native[key]
            .as_f64()
            .map(|seconds| (seconds.max(0.0) * 1000.0) as u64)
    };
    bridge.backend(
        native["is_recording"].as_bool().unwrap_or(false),
        native["is_paused"].as_bool().unwrap_or(false),
        milliseconds("recording_duration"),
        milliseconds("active_duration"),
    );
}

pub fn setup(app: &tauri::AppHandle) {
    let state = CompanionState(Arc::new(Mutex::new(Bridge::new())), Arc::new(Mutex::new(None)));
    app.manage(state.clone());
    let start_state = state.clone();
    app.listen("recording-started", move |_| {
        let native_session = crate::audio::recording_commands::current_session();
        let mut bridge = start_state.0.lock().unwrap();
        // Publish the protocol session and native identity under the same lock.
        // Execution always acquires bridge first, then the native identity lock.
        bridge.backend(true, false, Some(0), Some(0));
        *start_state.1.lock().unwrap() = native_session;
    });
    let stop_state = state.clone();
    app.listen("recording-stopped", move |_| {
        stop_state
            .0
            .lock()
            .unwrap()
            .backend(false, false, None, None);
    });
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let poll_state = state.clone();
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            loop {
                interval.tick().await;
                let native = crate::audio::recording_commands::get_recording_state().await;
                let mut bridge = poll_state.0.lock().unwrap();
                update_native(&mut bridge, &native);
                bridge.snapshot();
            }
        });
        if let Err(error) = server::serve(handle, state).await {
            log::error!("Companion bridge unavailable: {error}");
        }
    });
}

pub fn cleanup(app: &tauri::AppHandle) {
    if let Ok(dir) = app.path().app_data_dir() {
        let path = dir.join("companion/bridge.json");
        if let (Some(state), Ok(bytes)) = (app.try_state::<CompanionState>(), std::fs::read(&path))
        {
            if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if value["instance_id"].as_str()
                    == Some(&state.0.lock().unwrap().status.instance_id)
                {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
    }
}
