use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use uuid::Uuid;

pub const FRONTEND_TTL: Duration = Duration::from_secs(8);
const OPERATION_TTL: Duration = Duration::from_secs(180);
const MAX_OPERATIONS: usize = 128;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Idle,
    Starting,
    Recording,
    Paused,
    Stopping,
    Processing,
    Saving,
    Completed,
    Error,
}

#[derive(Clone, Serialize)]
pub struct Status {
    pub protocol_version: u8,
    pub instance_id: String,
    pub revision: u64,
    pub phase: Phase,
    pub session_id: Option<String>,
    pub recording_elapsed_ms: Option<u64>,
    pub active_elapsed_ms: Option<u64>,
    pub can_start: bool,
    pub can_stop: bool,
    pub frontend_ready: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub request_id: String,
    pub expected_instance_id: String,
    pub expected_session_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Start,
    Stop,
    Show,
}

#[derive(Clone, Serialize)]
pub struct Operation {
    pub operation_id: String,
    pub status: String,
    pub error: Option<String>,
    #[serde(skip)]
    pub action: Action,
    #[serde(skip)]
    request: Request,
    #[serde(skip)]
    created: Instant,
}

pub struct Bridge {
    pub status: Status,
    pub heartbeat: Option<Instant>,
    pub ready: bool,
    pub operations: VecDeque<Operation>,
    pub backend_recording: bool,
    pub executing: Option<String>,
    frontend_phase: Phase,
    stop_processing_seen: bool,
    stop_saving_seen: bool,
}

impl Bridge {
    pub fn new() -> Self {
        Self {
            status: Status {
                protocol_version: 1,
                instance_id: Uuid::new_v4().to_string(),
                revision: 0,
                phase: Phase::Idle,
                session_id: None,
                recording_elapsed_ms: None,
                active_elapsed_ms: None,
                can_start: false,
                can_stop: false,
                frontend_ready: false,
                error: None,
            },
            heartbeat: None,
            ready: false,
            operations: VecDeque::new(),
            backend_recording: false,
            executing: None,
            frontend_phase: Phase::Idle,
            stop_processing_seen: false,
            stop_saving_seen: false,
        }
    }

    pub fn pending(&self) -> Option<&Operation> {
        self.operations
            .iter()
            .find(|op| op.status == "pending" && op.action != Action::Show)
    }

    pub fn begin_execution(&mut self, id: &str) -> Result<Action, String> {
        if !self.snapshot().frontend_ready {
            return Err("Meetily frontend unavailable".into());
        }
        if self.executing.is_some() {
            return Err("Companion command already executing".into());
        }
        let op = self
            .pending()
            .filter(|op| op.operation_id == id)
            .cloned()
            .ok_or("Operation no longer pending")?;
        if op.action == Action::Stop
            && (!self.backend_recording || op.request.expected_session_id != self.status.session_id)
        {
            self.fail(id, "Recording session changed".into());
            return Err("Recording session changed".into());
        }
        if op.action == Action::Start && self.backend_recording {
            self.fail(id, "Another recording already started".into());
            return Err("Another recording already started".into());
        }
        self.executing = Some(id.into());
        Ok(op.action)
    }

    pub fn fail(&mut self, id: &str, error: String) {
        if let Some(op) = self
            .operations
            .iter_mut()
            .find(|op| op.operation_id == id && op.status == "pending")
        {
            op.status = "failed".into();
            op.error = Some(error.clone());
            self.status.phase = Phase::Error;
            self.status.error = Some(error);
            self.status.revision += 1;
        }
    }

    pub fn succeed(&mut self, id: &str) {
        if let Some(op) = self
            .operations
            .iter_mut()
            .find(|op| op.operation_id == id && op.status == "pending")
        {
            op.status = "succeeded".into();
            self.status.revision += 1;
        }
    }

    pub fn snapshot(&mut self) -> Status {
        let fresh = self.heartbeat.is_some_and(|t| t.elapsed() < FRONTEND_TTL) && self.ready;
        if let Some(op) = self.pending().cloned() {
            if !fresh || op.created.elapsed() > OPERATION_TTL {
                self.fail(
                    &op.operation_id,
                    if fresh {
                        "Operation timed out"
                    } else {
                        "Meetily frontend unavailable"
                    }
                    .into(),
                );
            }
        }
        self.status.frontend_ready = fresh;
        self.status.can_start = fresh
            && self.pending().is_none()
            && self.executing.is_none()
            && !self.backend_recording
            && matches!(self.frontend_phase, Phase::Idle | Phase::Error)
            && matches!(self.status.phase, Phase::Idle | Phase::Error);
        self.status.can_stop = fresh
            && self.pending().is_none()
            && self.executing.is_none()
            && self.backend_recording
            && matches!(
                self.status.phase,
                Phase::Recording | Phase::Paused | Phase::Error
            );
        self.status.clone()
    }

    pub fn accept(
        &mut self,
        action: Action,
        request: Request,
    ) -> Result<(Operation, bool), String> {
        if request.expected_instance_id != self.status.instance_id {
            return Err("Instance changed".into());
        }
        if request.request_id.is_empty() || request.request_id.len() > 128 {
            return Err("Invalid request_id".into());
        }
        if let Some(op) = self
            .operations
            .iter()
            .find(|op| op.request.request_id == request.request_id)
        {
            return if op.request == request && op.action == action {
                Ok((op.clone(), false))
            } else {
                Err("request_id already used with different parameters".into())
            };
        }
        if action == Action::Stop && request.expected_session_id.is_none() {
            return Err("expected_session_id required".into());
        }
        if let Some(expected) = &request.expected_session_id {
            if self.status.session_id.as_ref() != Some(expected) {
                return Err("Session changed".into());
            }
        }
        let status = self.snapshot();
        if action != Action::Show {
            if self.pending().is_some() || self.executing.is_some() {
                return Err("Another companion operation is pending".into());
            }
            self.stop_processing_seen = false;
            self.stop_saving_seen = false;
        }
        if action == Action::Start && !status.can_start {
            return Err("Recording cannot start in current state".into());
        }
        if action == Action::Stop && !status.can_stop {
            return Err("Recording cannot stop in current state".into());
        }
        let op = Operation {
            operation_id: request.request_id.clone(),
            status: "pending".into(),
            error: None,
            action,
            request,
            created: Instant::now(),
        };
        if action != Action::Show {
            self.status.phase = if action == Action::Start {
                Phase::Starting
            } else {
                Phase::Stopping
            };
            self.status.error = None;
        }
        self.status.revision += 1;
        if self.operations.len() >= MAX_OPERATIONS {
            if let Some(index) = self
                .operations
                .iter()
                .position(|old| old.status != "pending")
            {
                self.operations.remove(index);
            } else {
                return Err("Too many pending operations".into());
            }
        }
        self.operations.push_back(op.clone());
        Ok((op, true))
    }

    pub fn backend(
        &mut self,
        recording: bool,
        paused: bool,
        elapsed: Option<u64>,
        active: Option<u64>,
    ) {
        if recording && !self.backend_recording {
            if let Some(op) = self
                .pending()
                .cloned()
                .filter(|op| op.action == Action::Stop)
            {
                self.fail(&op.operation_id, "Recording session changed".into());
            }
            self.status.session_id = Some(Uuid::new_v4().to_string());
            self.status.error = None;
        }
        self.backend_recording = recording;
        if recording {
            self.status.recording_elapsed_ms = elapsed;
            self.status.active_elapsed_ms = active;
            if matches!(
                self.frontend_phase,
                Phase::Stopping | Phase::Processing | Phase::Saving
            ) {
                self.status.phase = self.frontend_phase.clone();
            } else if !matches!(self.pending().map(|op| op.action), Some(Action::Stop)) {
                self.status.phase = if paused {
                    Phase::Paused
                } else {
                    Phase::Recording
                };
            }
        } else if matches!(self.status.phase, Phase::Recording | Phase::Paused) {
            self.status.phase = Phase::Stopping;
        }
        self.status.revision += 1;
    }

    pub fn report(&mut self, phase: Phase, ready: bool, error: Option<String>) {
        self.heartbeat = Some(Instant::now());
        self.ready = ready;
        self.frontend_phase = phase.clone();
        if self.backend_recording
            && matches!(phase, Phase::Stopping | Phase::Processing | Phase::Saving)
        {
            self.status.phase = phase.clone();
        }
        if let Some(op) = self.pending().cloned() {
            if phase == Phase::Error {
                self.fail(
                    &op.operation_id,
                    error.unwrap_or_else(|| "Recording failed".into()),
                );
            } else if op.action == Action::Stop && !self.backend_recording {
                match phase {
                    Phase::Completed if self.stop_saving_seen => {
                        self.status.phase = Phase::Completed;
                        self.succeed(&op.operation_id);
                    }
                    Phase::Idle if self.stop_processing_seen => self.fail(
                        &op.operation_id,
                        "Recording stopped without confirmed meeting save".into(),
                    ),
                    Phase::Processing | Phase::Saving => {
                        self.stop_processing_seen = true;
                        self.stop_saving_seen |= phase == Phase::Saving;
                        self.status.phase = phase;
                    }
                    _ => {}
                }
            }
        } else if !self.backend_recording && !matches!(self.status.phase, Phase::Error) {
            self.status.phase = phase;
            self.status.error = error;
        } else if phase == Phase::Error {
            self.status.phase = Phase::Error;
            self.status.error = error;
        }
        self.status.revision += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ready() -> Bridge {
        let mut b = Bridge::new();
        b.report(Phase::Idle, true, None);
        b
    }
    fn request(b: &Bridge, id: &str) -> Request {
        Request {
            request_id: id.into(),
            expected_instance_id: b.status.instance_id.clone(),
            expected_session_id: b.status.session_id.clone(),
        }
    }
    #[test]
    fn guards_instance_session_and_deduplicates() {
        let mut b = ready();
        let mut req = request(&b, "start");
        req.expected_instance_id = "old".into();
        assert!(b.accept(Action::Start, req).is_err());
        let req = request(&b, "start");
        let (op, fresh) = b.accept(Action::Start, req.clone()).unwrap();
        assert!(fresh);
        assert_eq!(
            b.accept(Action::Start, req.clone()).unwrap().0.operation_id,
            op.operation_id
        );
        assert!(!b.accept(Action::Start, req.clone()).unwrap().1);
        assert!(b.accept(Action::Show, req).is_err());
        b.succeed(&op.operation_id);
        b.backend(true, false, Some(1000), Some(900));
        let mut stop = request(&b, "stop");
        stop.expected_session_id = Some("old".into());
        assert!(b.accept(Action::Stop, stop).is_err());
        let mut stop = request(&b, "stop");
        stop.expected_session_id = None;
        assert!(b.accept(Action::Stop, stop).is_err());
    }
    #[test]
    fn stop_requires_completed_save_and_errors_stay_visible() {
        for (phase, result) in [
            (Phase::Completed, "succeeded"),
            (Phase::Idle, "failed"),
            (Phase::Error, "failed"),
        ] {
            let mut b = ready();
            b.backend(true, true, Some(2000), Some(1000));
            assert_eq!(b.snapshot().phase, Phase::Paused);
            let req = request(&b, "stop");
            let (op, _) = b.accept(Action::Stop, req).unwrap();
            b.backend(false, false, None, None);
            b.report(Phase::Saving, true, None);
            assert_eq!(b.pending().unwrap().status, "pending");
            b.report(phase, true, Some("save failed".into()));
            assert_eq!(b.operations.back().unwrap().status, result);
            assert_eq!(b.operations.back().unwrap().operation_id, op.operation_id);
            if result == "failed" {
                b.report(Phase::Idle, true, None);
                assert_eq!(b.snapshot().phase, Phase::Error);
            }
        }
    }
    #[test]
    fn stale_frontend_fails_closed_and_serializes() {
        let mut b = ready();
        let req = request(&b, "one");
        b.accept(Action::Start, req).unwrap();
        let req = request(&b, "two");
        assert!(b.accept(Action::Start, req).is_err());
        b.heartbeat = Some(Instant::now() - FRONTEND_TTL);
        let s = b.snapshot();
        assert!(!s.can_start && !s.can_stop && !s.frontend_ready);
        assert_eq!(s.phase, Phase::Error);
        assert_eq!(b.operations.back().unwrap().status, "failed");
    }
    #[test]
    fn show_remains_available_while_recording_operation_is_pending() {
        let mut b = ready();
        b.backend(true, false, Some(1000), Some(1000));
        let req = request(&b, "stop");
        let (stop, _) = b.accept(Action::Stop, req).unwrap();
        b.backend(false, false, None, None);
        b.report(Phase::Saving, true, None);
        let req = request(&b, "show");
        let (show, _) = b.accept(Action::Show, req).unwrap();
        b.succeed(&show.operation_id);
        b.report(Phase::Completed, true, None);
        assert_eq!(
            b.operations
                .iter()
                .find(|op| op.operation_id == stop.operation_id)
                .unwrap()
                .status,
            "succeeded"
        );
    }
    #[test]
    fn external_stop_stays_busy_and_completed_waits_for_idle_cleanup() {
        let mut b = ready();
        b.backend(true, false, Some(1000), Some(1000));
        b.report(Phase::Stopping, true, None);
        b.backend(true, false, None, None);
        assert_eq!(b.snapshot().phase, Phase::Stopping);
        assert!(!b.snapshot().can_stop);
        b.backend(false, false, None, None);
        b.report(Phase::Completed, true, None);
        assert!(!b.snapshot().can_start);
        b.report(Phase::Idle, true, None);
        assert!(b.snapshot().can_start);
    }
    #[test]
    fn pending_stop_cannot_execute_against_replacement_session() {
        let mut b = ready();
        b.backend(true, false, Some(1000), Some(1000));
        let req = request(&b, "old-stop");
        let (op, _) = b.accept(Action::Stop, req).unwrap();
        b.backend(false, false, None, None);
        assert!(b.begin_execution(&op.operation_id).is_err());
        b.backend(true, false, Some(0), Some(0));
        assert!(b.begin_execution(&op.operation_id).is_err());
        assert_eq!(b.operations.back().unwrap().status, "failed");
    }
    #[test]
    fn expired_operation_cannot_enable_start_while_frontend_saves() {
        let mut b = ready();
        b.backend(true, false, Some(1000), Some(1000));
        let req = request(&b, "stop");
        b.accept(Action::Stop, req).unwrap();
        b.report(Phase::Processing, true, None);
        b.heartbeat = Some(Instant::now() - FRONTEND_TTL);
        b.snapshot();
        b.backend(false, false, None, None);
        b.report(Phase::Saving, true, None);
        assert!(!b.snapshot().can_start);
    }
    #[test]
    fn bounds_retained_operations() {
        let mut b = ready();
        for n in 0..140 {
            let req = request(&b, &n.to_string());
            let (op, _) = b.accept(Action::Show, req).unwrap();
            b.succeed(&op.operation_id);
        }
        assert_eq!(b.operations.len(), MAX_OPERATIONS);
    }
}
