//! T15: off-thread indexed history/body reader.
//!
//! Owns a separate read-only `SqliteV4` connection. Pages use indexed keyset
//! queries pinned to a commit watermark; report bodies are read one at a time
//! only when a result is displayed. Requests/responses use bounded channels and
//! carry a caller request id so a stale response can never clear or replace a
//! newer request's UI state.

use crate::sqlite_v4::{ActivityCursor, HistoryPage, HistoryRow, SqliteV4};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::thread::{self, JoinHandle};

pub(crate) const REQUEST_CAPACITY: usize = 16;
pub(crate) const RESPONSE_CAPACITY: usize = 32;

pub(crate) enum ReaderRequest {
    Page {
        request_id: u64,
        automation: u64,
        cursor: Option<ActivityCursor>,
        watermark: Option<u64>,
        limit: usize,
    },
    Body {
        request_id: u64,
        run_id: u64,
    },
    Shutdown,
}

pub(crate) enum ReaderResponse {
    Page {
        request_id: u64,
        automation: u64,
        result: Result<HistoryPage, String>,
    },
    Body {
        request_id: u64,
        run_id: u64,
        result: Result<Option<String>, String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HistoryLoad {
    NotRequested,
    Loading { request_id: u64 },
    Loaded,
    LoadedEmpty,
    Failed { error: String, retry_at: std::time::Instant },
}

impl Default for HistoryLoad {
    fn default() -> Self {
        Self::NotRequested
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BodyLoad {
    NotRequested,
    Loading { request_id: u64 },
    Loaded,
    Absent,
    Failed { error: String, retry_at: std::time::Instant },
}

impl Default for BodyLoad {
    fn default() -> Self {
        Self::NotRequested
    }
}

/// UI-owned accumulated page state for one automation. Rows are keyed by
/// occurrence id and stay ordered newest-first.
#[derive(Default)]
pub(crate) struct HistoryCache {
    pub rows: Vec<HistoryRow>,
    pub next: Option<ActivityCursor>,
    pub watermark: Option<u64>,
    pub total: usize,
    pub state: HistoryLoad,
    /// True while a first/reset page is being loaded; additional pages append.
    pub reset: bool,
}

pub(crate) struct HistoryReader {
    requests: SyncSender<ReaderRequest>,
    responses: Receiver<ReaderResponse>,
    join: Option<JoinHandle<()>>,
}

impl HistoryReader {
    pub(crate) fn start(
        database: &Path,
        root: &Path,
        incarnation: u64,
    ) -> Result<Self, String> {
        let (request_tx, request_rx) = mpsc::sync_channel(REQUEST_CAPACITY);
        let (response_tx, response_rx) = mpsc::sync_channel(RESPONSE_CAPACITY);
        let database = database.to_path_buf();
        let root = root.to_path_buf();
        let join = thread::Builder::new()
            .name("t15-history-reader".into())
            .spawn(move || {
                let handle = SqliteV4::open(&database, &root, incarnation);
                match handle {
                    Ok(database) => {
                        for request in request_rx {
                            match request {
                                ReaderRequest::Page {
                                    request_id,
                                    automation,
                                    cursor,
                                    watermark,
                                    limit,
                                } => {
                                    let result = database.page(&crate::sqlite_v4::PageQuery {
                                        automation: Some(automation),
                                        cursor,
                                        watermark,
                                        limit,
                                    });
                                    if response_tx
                                        .send(ReaderResponse::Page {
                                            request_id,
                                            automation,
                                            result,
                                        })
                                        .is_err()
                                    {
                                        return;
                                    }
                                }
                                ReaderRequest::Body { request_id, run_id } => {
                                    let result = database.body(run_id);
                                    if response_tx
                                        .send(ReaderResponse::Body {
                                            request_id,
                                            run_id,
                                            result,
                                        })
                                        .is_err()
                                    {
                                        return;
                                    }
                                }
                                ReaderRequest::Shutdown => return,
                            }
                        }
                    }
                    Err(error) => {
                        // Report the open failure on every queued request instead
                        // of hanging the UI request state.
                        for request in request_rx {
                            match request {
                                ReaderRequest::Page {
                                    request_id,
                                    automation,
                                    ..
                                } => {
                                    let _ = response_tx.send(ReaderResponse::Page {
                                        request_id,
                                        automation,
                                        result: Err(error.clone()),
                                    });
                                }
                                ReaderRequest::Body { request_id, run_id } => {
                                    let _ = response_tx.send(ReaderResponse::Body {
                                        request_id,
                                        run_id,
                                        result: Err(error.clone()),
                                    });
                                }
                                ReaderRequest::Shutdown => return,
                            }
                        }
                    }
                }
            })
            .map_err(|error| format!("cannot start history reader: {error}"))?;
        Ok(Self {
            requests: request_tx,
            responses: response_rx,
            join: Some(join),
        })
    }

    pub(crate) fn request_page(
        &self,
        request_id: u64,
        automation: u64,
        cursor: Option<ActivityCursor>,
        watermark: Option<u64>,
        limit: usize,
    ) -> Result<(), String> {
        self.requests
            .try_send(ReaderRequest::Page {
                request_id,
                automation,
                cursor,
                watermark,
                limit,
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => "history reader is busy; retry shortly".to_string(),
                TrySendError::Disconnected(_) => "history reader is unavailable".to_string(),
            })
    }

    pub(crate) fn request_body(&self, request_id: u64, run_id: u64) -> Result<(), String> {
        self.requests
            .try_send(ReaderRequest::Body { request_id, run_id })
            .map_err(|error| match error {
                TrySendError::Full(_) => "history reader is busy; retry shortly".to_string(),
                TrySendError::Disconnected(_) => "history reader is unavailable".to_string(),
            })
    }

    /// Bounded drain so a busy reader cannot starve the frame.
    pub(crate) fn poll(&mut self, limit: usize) -> Vec<ReaderResponse> {
        let mut responses = Vec::new();
        for _ in 0..limit {
            match self.responses.try_recv() {
                Ok(response) => responses.push(response),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        responses
    }
}

impl Drop for HistoryReader {
    fn drop(&mut self) {
        // Never join on the UI thread: a slow disk read may still be running.
        let _ = self.requests.try_send(ReaderRequest::Shutdown);
        let _ = self.join.take();
    }
}
