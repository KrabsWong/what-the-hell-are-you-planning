//! T14-S0 measurement spans for persistence and query work.
//!
//! The module is compiled in every profile so instrumentation call sites do not
//! need `cfg` attributes. Recording is disabled unless the `qa` feature is on:
//! `Span::start` then stores no timestamp and `Drop` returns immediately.
//!
//! Spans are thread-local. The S0 harness measures on one thread, so no
//! cross-thread aggregation is required. Samples are capped to keep a runaway
//! loop from growing memory.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// True only in QA builds.
pub const ENABLED: bool = cfg!(feature = "qa");

const MAX_SAMPLES_PER_SPAN: usize = 8192;

#[derive(Clone, Debug, Default)]
pub struct SpanStats {
    pub name: &'static str,
    pub count: usize,
    pub total_us: u64,
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
    pub max_us: u64,
}

#[derive(Default)]
struct Recorder {
    samples: BTreeMap<&'static str, Vec<u64>>,
    counters: BTreeMap<&'static str, u64>,
}

thread_local! {
    static RECORDER: RefCell<Recorder> = RefCell::new(Recorder::default());
}

/// Records its lifetime under `name` when dropped. Nested spans are allowed and
/// are reported independently; a parent span is not automatically net of its
/// children.
pub struct Span {
    name: &'static str,
    start: Option<Instant>,
}

impl Span {
    pub fn start(name: &'static str) -> Self {
        Self {
            name,
            start: if ENABLED { Some(Instant::now()) } else { None },
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some(start) = self.start.take() {
            record(self.name, start.elapsed());
        }
    }
}

/// Time a closure with a named span.
pub fn measure<T>(name: &'static str, work: impl FnOnce() -> T) -> T {
    let _span = Span::start(name);
    work()
}

/// Record an externally timed duration (for wall-clock interaction samples).
pub fn record_external(name: &'static str, elapsed: Duration) {
    if ENABLED {
        record(name, elapsed);
    }
}

fn record(name: &'static str, elapsed: Duration) {
    if !ENABLED {
        return;
    }
    let micros = elapsed.as_micros().min(u64::MAX as u128) as u64;
    RECORDER.with(|recorder| {
        let mut recorder = recorder.borrow_mut();
        let samples = recorder.samples.entry(name).or_default();
        if samples.len() < MAX_SAMPLES_PER_SPAN {
            samples.push(micros);
        }
    });
}

/// Increment a named counter (for example body bytes materialized).
pub fn bump(name: &'static str, by: u64) {
    if !ENABLED {
        return;
    }
    RECORDER.with(|recorder| {
        *recorder
            .borrow_mut()
            .counters
            .entry(name)
            .or_insert(0) += by;
    });
}

pub fn snapshot() -> Vec<SpanStats> {
    RECORDER.with(|recorder| {
        let mut recorder = recorder.borrow_mut();
        let mut stats = Vec::with_capacity(recorder.samples.len());
        for (name, samples) in recorder.samples.iter_mut() {
            if samples.is_empty() {
                continue;
            }
            samples.sort_unstable();
            let count = samples.len();
            let total_us = samples.iter().sum();
            stats.push(SpanStats {
                name,
                count,
                total_us,
                p50_us: percentile(samples, 0.50),
                p95_us: percentile(samples, 0.95),
                p99_us: percentile(samples, 0.99),
                max_us: samples[count - 1],
            });
        }
        stats.sort_by(|a, b| a.name.cmp(b.name));
        stats
    })
}

pub fn counters() -> BTreeMap<&'static str, u64> {
    RECORDER.with(|recorder| recorder.borrow().counters.clone())
}

pub fn counter(name: &'static str) -> u64 {
    RECORDER.with(|recorder| recorder.borrow().counters.get(name).copied().unwrap_or(0))
}

pub fn reset() {
    if !ENABLED {
        return;
    }
    RECORDER.with(|recorder| {
        let mut recorder = recorder.borrow_mut();
        recorder.samples.clear();
        recorder.counters.clear();
    });
}

fn percentile(sorted: &[u64], fraction: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_snapshot_is_empty() {
        reset();
        assert!(snapshot().is_empty());
    }

    #[test]
    fn disabled_module_records_nothing_without_qa() {
        reset();
        {
            let _span = Span::start("test.span");
        }
        record_external("test.external", Duration::from_micros(10));
        if !ENABLED {
            assert!(snapshot().is_empty());
            assert!(counters().is_empty());
        }
    }
}
