use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crate::popup::{PopupKind, PopupRequest};

#[derive(Default)]
struct Inner {
    items: VecDeque<PopupRequest>,
    showing_preview: bool,
    interrupt: bool,
}

#[derive(Default)]
pub struct PopupQueue {
    inner: Mutex<Inner>,
    ready: Condvar,
}

impl PopupQueue {
    pub fn new() -> PopupQueue {
        PopupQueue::default()
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn push(&self, request: PopupRequest) {
        let mut inner = self.lock();
        if request.kind == PopupKind::Progress {
            let same = inner.items.iter_mut().find(|queued| {
                queued.kind == PopupKind::Progress
                    && queued.app_id == request.app_id
                    && queued.api_name.eq_ignore_ascii_case(&request.api_name)
            });
            match same {
                Some(slot) => *slot = request,
                None => inner.items.push_back(request),
            }
        } else {
            let at = inner
                .items
                .iter()
                .position(|queued| queued.kind == PopupKind::Progress)
                .unwrap_or(inner.items.len());
            inner.items.insert(at, request);
        }
        self.ready.notify_all();
    }

    pub fn push_preview(&self, request: PopupRequest) {
        let mut inner = self.lock();
        inner.items.retain(|queued| !queued.preview);
        inner.items.push_front(request);
        if inner.showing_preview {
            inner.interrupt = true;
        }
        self.ready.notify_all();
    }

    pub fn next(&self) -> PopupRequest {
        let mut inner = self.lock();
        loop {
            if let Some(request) = inner.items.pop_front() {
                inner.showing_preview = request.preview;
                inner.interrupt = false;
                return request;
            }
            inner = self
                .ready
                .wait(inner)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    pub fn wait_or_interrupt(&self, duration: Duration) -> bool {
        let inner = self.lock();
        let (mut inner, _) = self
            .ready
            .wait_timeout_while(inner, duration, |state| !state.interrupt)
            .unwrap_or_else(PoisonError::into_inner);
        let interrupted = inner.interrupt;
        inner.interrupt = false;
        interrupted
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::popup::{PopupKind, PopupRequest};

    fn req(kind: PopupKind, app_id: u32, api_name: &str, preview: bool) -> PopupRequest {
        PopupRequest {
            game: "Game".into(),
            display_name: api_name.into(),
            description: String::new(),
            icon_path: String::new(),
            screenshot: false,
            preset_override: None,
            kind,
            percent: None,
            preview,
            app_id,
            api_name: api_name.into(),
            progress: None,
            total_achievements: 0,
            achievement_icons: Vec::new(),
        }
    }

    #[test]
    fn queue_unlock_jumps_ahead_of_progress() {
        let q = PopupQueue::new();
        q.push(req(PopupKind::Progress, 1, "P", false));
        q.push(req(PopupKind::Unlock, 1, "U", false));
        assert_eq!(q.next().api_name, "U");
        assert_eq!(q.next().api_name, "P");
    }

    #[test]
    fn queue_progress_replaces_same_achievement() {
        let q = PopupQueue::new();
        let mut first = req(PopupKind::Progress, 1, "Mother", false);
        first.progress = Some((74, 100));
        let mut second = req(PopupKind::Progress, 1, "mother", false);
        second.progress = Some((75, 100));
        q.push(first);
        q.push(req(PopupKind::Progress, 2, "Other", false));
        q.push(second);
        assert_eq!(q.next().progress, Some((75, 100)));
        assert_eq!(q.next().api_name, "Other");
        assert!(q.lock().items.is_empty());
    }

    #[test]
    fn queue_keeps_fifo_for_thirty_unlocks() {
        let q = PopupQueue::new();
        for i in 0..30 {
            q.push(req(PopupKind::Unlock, 1, &format!("A{i}"), false));
        }
        let order: Vec<String> = (0..30).map(|_| q.next().api_name).collect();
        let expected: Vec<String> = (0..30).map(|i| format!("A{i}")).collect();
        assert_eq!(order, expected);
    }

    #[test]
    fn preview_interrupts_only_preview() {
        let q = PopupQueue::new();
        q.push(req(PopupKind::Unlock, 1, "Real", false));
        assert!(!q.next().preview);
        q.push_preview(req(PopupKind::Unlock, 0, "Preview1", true));
        assert!(!q.wait_or_interrupt(Duration::from_millis(5)));
        assert!(q.next().preview);
        q.push_preview(req(PopupKind::Unlock, 0, "Preview2", true));
        assert!(q.wait_or_interrupt(Duration::from_secs(5)));
        assert_eq!(q.next().api_name, "Preview2");
    }

    #[test]
    fn preview_goes_before_queued_unlocks() {
        let q = PopupQueue::new();
        q.push(req(PopupKind::Unlock, 1, "Real", false));
        q.push_preview(req(PopupKind::Unlock, 0, "Preview", true));
        assert_eq!(q.next().api_name, "Preview");
        assert_eq!(q.next().api_name, "Real");
    }
}
