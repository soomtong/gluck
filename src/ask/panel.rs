//! Ask panel state: the streamed answer, its status and scroll position.

use super::AskMessage;
use ratatui::text::Line;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelStatus {
    Closed,
    Streaming,
    Done,
    Failed,
}

pub struct AskPanel {
    pub status: PanelStatus,
    /// What was asked about, e.g. `page · src/app.rs`.
    pub title: String,
    /// Endpoint label, e.g. `deepseek/deepseek-chat (env)`.
    pub label: String,
    pub text: String,
    pub error: Option<String>,
    pub truncated: bool,
    /// Markdown-highlighted `text`, filled once the stream is done.
    pub rendered: Vec<Line<'static>>,
    /// First visible row; clamped at render time.
    pub scroll: usize,
    /// Stick to the bottom while streaming until the user scrolls up.
    pub follow: bool,
    pub started: Instant,
    rx: Option<Receiver<AskMessage>>,
}

impl Default for AskPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl AskPanel {
    pub fn new() -> Self {
        Self {
            status: PanelStatus::Closed,
            title: String::new(),
            label: String::new(),
            text: String::new(),
            error: None,
            truncated: false,
            rendered: Vec::new(),
            scroll: 0,
            follow: true,
            started: Instant::now(),
            rx: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.status != PanelStatus::Closed
    }

    pub fn is_streaming(&self) -> bool {
        self.status == PanelStatus::Streaming
    }

    fn reset(&mut self, title: String, label: String) {
        *self = Self {
            title,
            label,
            ..Self::new()
        };
    }

    /// Replaces any in-flight request; the old receiver is dropped.
    pub fn start(&mut self, title: String, label: String, rx: Receiver<AskMessage>) {
        self.reset(title, label);
        self.status = PanelStatus::Streaming;
        self.rx = Some(rx);
    }

    pub fn fail(&mut self, title: String, label: String, error: String) {
        self.reset(title, label);
        self.status = PanelStatus::Failed;
        self.error = Some(error);
    }

    pub fn close(&mut self) {
        self.reset(String::new(), String::new());
    }

    /// Pull pending stream messages. Returns true when the stream just
    /// finished successfully (caller renders the markdown once).
    pub fn drain(&mut self) -> bool {
        let Some(rx) = &self.rx else {
            return false;
        };
        let mut finished = false;
        loop {
            match rx.try_recv() {
                Ok(AskMessage::Delta(d)) => self.text.push_str(&d),
                Ok(AskMessage::Done { truncated }) => {
                    self.truncated = truncated;
                    self.status = PanelStatus::Done;
                    finished = true;
                    break;
                }
                Ok(AskMessage::Failed(e)) => {
                    self.status = PanelStatus::Failed;
                    self.error = Some(e);
                    break;
                }
                Err(TryRecvError::Empty) => return false,
                Err(TryRecvError::Disconnected) => {
                    self.status = PanelStatus::Failed;
                    self.error = Some("request thread ended unexpectedly".into());
                    break;
                }
            }
        }
        self.rx = None;
        finished
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_add(n);
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
        self.follow = false;
    }

    /// Clamp `scroll` for `rows` rendered rows in a `height`-row viewport;
    /// scrolling back to the bottom re-enables follow.
    pub fn clamp_scroll(&mut self, rows: usize, height: usize) {
        let max = rows.saturating_sub(height);
        if self.follow || self.scroll >= max {
            self.scroll = max;
            self.follow = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn test_drain_accumulates_then_finishes() {
        let (tx, rx) = mpsc::channel();
        let mut p = AskPanel::new();
        p.start("t".into(), "l".into(), rx);
        tx.send(AskMessage::Delta("ab".into())).unwrap();
        assert!(!p.drain());
        assert!(p.is_streaming());
        tx.send(AskMessage::Delta("c".into())).unwrap();
        tx.send(AskMessage::Done { truncated: true }).unwrap();
        assert!(p.drain());
        assert_eq!(p.text, "abc");
        assert!(p.truncated);
        assert_eq!(p.status, PanelStatus::Done);
    }

    #[test]
    fn test_disconnect_is_failure() {
        let (tx, rx) = mpsc::channel::<AskMessage>();
        let mut p = AskPanel::new();
        p.start("t".into(), "l".into(), rx);
        drop(tx);
        p.drain();
        assert_eq!(p.status, PanelStatus::Failed);
    }

    #[test]
    fn test_restart_drops_old_stream() {
        let (old_tx, old_rx) = mpsc::channel();
        let mut p = AskPanel::new();
        p.start("a".into(), "l".into(), old_rx);
        let (_new_tx, new_rx) = mpsc::channel();
        p.start("b".into(), "l".into(), new_rx);
        // The worker notices the dropped receiver on its next send.
        assert!(old_tx.send(AskMessage::Delta("x".into())).is_err());
        p.drain();
        assert!(p.text.is_empty());
        assert_eq!(p.title, "b");
    }

    #[test]
    fn test_close_resets() {
        let (_tx, rx) = mpsc::channel();
        let mut p = AskPanel::new();
        p.start("t".into(), "l".into(), rx);
        p.close();
        assert!(!p.is_open());
        assert!(!p.drain());
    }

    #[test]
    fn test_scroll_follow_and_clamp() {
        let mut p = AskPanel::new();
        p.clamp_scroll(30, 10);
        assert_eq!(p.scroll, 20);
        p.scroll_up(5);
        assert!(!p.follow);
        p.clamp_scroll(40, 10);
        assert_eq!(p.scroll, 15);
        p.scroll_down(100);
        p.clamp_scroll(40, 10);
        assert_eq!(p.scroll, 30);
        assert!(p.follow);
    }
}
