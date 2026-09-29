//! Cumulative per-skill snapshots, shared by manual and startup batch updates.
//! A dropped operation marks unfinished items as failed so the UI cannot hang.
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

pub const EVENT: &str = "skill-update-progress";

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStatus {
    Pending,
    Checking,
    Updating,
    Updated,
    Unchanged,
    HeldBack,
    Failed,
}

#[derive(Clone, Serialize)]
pub struct UpdateItem {
    pub id: String,
    pub name: String,
    pub status: UpdateStatus,
    pub error: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct UpdateProgressPayload {
    pub run_id: String,
    pub items: Vec<UpdateItem>,
    pub finished: bool,
}

pub struct UpdateProgress {
    payload: UpdateProgressPayload,
    emit: Box<dyn Fn(&UpdateProgressPayload) + Send>,
}

impl UpdateProgress {
    pub fn new<R: Runtime>(app: AppHandle<R>, skills: Vec<(String, String)>) -> Self {
        Self::with_emitter(skills, move |payload| {
            if let Err(err) = app.emit(EVENT, payload) {
                log::debug!("Could not emit skill update progress: {err}");
            }
        })
    }

    fn with_emitter(
        skills: Vec<(String, String)>,
        emit: impl Fn(&UpdateProgressPayload) + Send + 'static,
    ) -> Self {
        let progress = Self {
            payload: UpdateProgressPayload {
                run_id: uuid::Uuid::new_v4().to_string(),
                finished: skills.is_empty(),
                items: skills
                    .into_iter()
                    .map(|(id, name)| UpdateItem {
                        id,
                        name,
                        status: UpdateStatus::Pending,
                        error: None,
                    })
                    .collect(),
            },
            emit: Box::new(emit),
        };
        (progress.emit)(&progress.payload);
        progress
    }

    pub fn report(&mut self, id: &str, status: UpdateStatus, error: Option<String>) {
        for item in self.payload.items.iter_mut().filter(|item| item.id == id) {
            item.status = status.clone();
            item.error = error.clone();
        }
        self.payload.finished = self.payload.items.iter().all(|item| {
            !matches!(
                item.status,
                UpdateStatus::Pending | UpdateStatus::Checking | UpdateStatus::Updating
            )
        });
        (self.emit)(&self.payload);
    }
}

impl Drop for UpdateProgress {
    fn drop(&mut self) {
        if self.payload.finished {
            return;
        }
        for item in &mut self.payload.items {
            if matches!(
                item.status,
                UpdateStatus::Pending | UpdateStatus::Checking | UpdateStatus::Updating
            ) {
                item.status = UpdateStatus::Failed;
                item.error = Some("Update interrupted before completion".into());
            }
        }
        self.payload.finished = true;
        (self.emit)(&self.payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn snapshots_include_names_and_real_completion() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let mut progress = UpdateProgress::with_emitter(
            vec![("a".into(), "Alpha".into()), ("b".into(), "Beta".into())],
            move |payload| sink.lock().unwrap().push(payload.clone()),
        );
        progress.report("a", UpdateStatus::Updating, None);
        progress.report("a", UpdateStatus::Updated, None);
        progress.report("b", UpdateStatus::HeldBack, None);
        drop(progress);
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].items[0].name, "Alpha");
        assert!(!events[2].finished);
        assert!(events[3].finished);
        assert_eq!(events[3].items[1].status, UpdateStatus::HeldBack);
        assert_eq!(events[0].run_id, events[3].run_id);
    }

    #[test]
    fn early_exit_finishes_pending_items_without_losing_results() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let mut progress = UpdateProgress::with_emitter(
            vec![("a".into(), "Alpha".into()), ("b".into(), "Beta".into())],
            move |payload| sink.lock().unwrap().push(payload.clone()),
        );
        progress.report("a", UpdateStatus::Unchanged, None);
        drop(progress);
        let events = events.lock().unwrap();
        let last = events.last().unwrap();
        assert!(last.finished);
        assert_eq!(last.items[0].status, UpdateStatus::Unchanged);
        assert_eq!(last.items[1].status, UpdateStatus::Failed);
        assert!(last.items[1].error.is_some());
    }
}
