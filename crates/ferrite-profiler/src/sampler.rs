//! Background sampling of the scopes registered threads are inside.

use std::collections::HashMap;
use std::fmt;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::Serialize;

use crate::timings::{MAX_DEPTH, Node, Timings};

/// Periodically samples the scope stacks of registered threads.
///
/// Scopes push their node onto a small per-thread atomic stack and pop it when
/// the guard drops, so the sampler can read a consistent-enough picture of what
/// every thread is doing without stopping it. Each pass increments the sample
/// count of every scope on every non-empty stack, which turns into a
/// flame-graph-like tree rooted at the outermost scopes.
///
/// The sampler is optional: start one when a `/spark`-style profile is
/// requested and drop it (or call [`Sampler::stop`]) afterwards. It runs on a
/// dedicated `ferrite-sampler` thread and only allocates on that thread.
///
/// ```
/// use std::sync::Arc;
/// use std::time::Duration;
/// use ferrite_profiler::{Sampler, Timings};
///
/// let timings = Arc::new(Timings::new());
/// let sampler = Sampler::start(&timings, Duration::from_millis(5)).expect("sampler");
/// {
///     let _work = timings.scope("work");
///     std::thread::sleep(Duration::from_millis(50));
/// }
/// sampler.stop();
/// let tree = sampler.tree();
/// assert!(tree.roots.iter().any(|root| root.path == "work"));
/// ```
pub struct Sampler {
    inner: Arc<Inner>,
    stop: Mutex<Option<Sender<()>>>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

/// Aggregated sample tree exported by a [`Sampler`].
#[derive(Debug, Clone, Serialize)]
pub struct SampleTree {
    /// Sampling interval in milliseconds.
    pub interval_ms: f64,
    /// Number of sampling passes performed.
    pub sweeps: u64,
    /// Registered threads visible at export time.
    pub threads: usize,
    /// Thread stacks observed across all passes.
    pub samples_taken: u64,
    /// Observations where the thread was inside at least one scope.
    ///
    /// Percentages are relative to this value.
    pub samples_attributed: u64,
    /// Root scopes, sorted by sample count descending.
    pub roots: Vec<SampleNode>,
}

/// One node of a [`SampleTree`].
#[derive(Debug, Clone, Serialize)]
pub struct SampleNode {
    /// Scope label.
    pub name: String,
    /// Slash-separated path from the root, for example `tick/entity-tick`.
    pub path: String,
    /// Samples that had this scope anywhere on the stack.
    pub samples: u64,
    /// Samples where this scope was the innermost one.
    pub self_samples: u64,
    /// Share of [`SampleTree::samples_attributed`], in percent.
    pub percent: f64,
    /// Direct child scopes, sorted by sample count descending.
    pub children: Vec<SampleNode>,
}

impl SampleTree {
    /// Plain-text flame graph, one row per scope.
    pub fn to_text(&self) -> String {
        self.to_string()
    }

    /// Compact JSON document.
    ///
    /// # Panics
    ///
    /// Panics if the tree contains a value `serde_json` cannot represent,
    /// which cannot happen because every float is finite.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("sample tree is always serializable")
    }

    /// Pretty-printed JSON document.
    ///
    /// # Panics
    ///
    /// Panics if the tree contains a value `serde_json` cannot represent,
    /// which cannot happen because every float is finite.
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).expect("sample tree is always serializable")
    }
}

impl fmt::Display for SampleTree {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "Sampler: {} samples in scopes, {} thread observations, {} sweeps, {:.2} ms interval",
            self.samples_attributed, self.samples_taken, self.sweeps, self.interval_ms
        )?;
        for root in &self.roots {
            write_sample(formatter, root, 0)?;
        }
        Ok(())
    }
}

fn write_sample(
    formatter: &mut fmt::Formatter<'_>,
    node: &SampleNode,
    depth: usize,
) -> fmt::Result {
    for _ in 0..depth {
        write!(formatter, "  ")?;
    }
    writeln!(
        formatter,
        "{:<44} {:>10} samples {:>5.1}%",
        node.path, node.samples, node.percent
    )?;
    for child in &node.children {
        write_sample(formatter, child, depth + 1)?;
    }
    Ok(())
}

impl Sampler {
    /// Default sampling interval.
    pub const DEFAULT_INTERVAL: Duration = Duration::from_millis(5);

    /// Starts a sampler for `timings`.
    ///
    /// Intervals shorter than one millisecond are clamped to one millisecond.
    /// The sampler keeps the [`Timings`] alive through an `Arc` until it is
    /// stopped.
    pub fn start(timings: &Arc<Timings>, interval: Duration) -> io::Result<Self> {
        let interval = interval.max(Duration::from_millis(1));
        let inner = Arc::new(Inner {
            timings: Arc::clone(timings),
            interval,
            sweeps: AtomicU64::new(0),
            samples_taken: AtomicU64::new(0),
            samples_attributed: AtomicU64::new(0),
        });
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let thread_inner = Arc::clone(&inner);
        let handle = thread::Builder::new()
            .name("ferrite-sampler".to_owned())
            .spawn(move || run(thread_inner, stop_rx))?;
        Ok(Self {
            inner,
            stop: Mutex::new(Some(stop_tx)),
            handle: Mutex::new(Some(handle)),
        })
    }

    /// Sampling interval, after clamping.
    pub fn interval(&self) -> Duration {
        self.inner.interval
    }

    /// Number of sampling passes performed.
    pub fn sweeps(&self) -> u64 {
        self.inner.sweeps.load(Ordering::Relaxed)
    }

    /// Thread stacks observed across all passes.
    pub fn samples_taken(&self) -> u64 {
        self.inner.samples_taken.load(Ordering::Relaxed)
    }

    /// Observations where the thread was inside at least one scope.
    pub fn samples_attributed(&self) -> u64 {
        self.inner.samples_attributed.load(Ordering::Relaxed)
    }

    /// Stops the background thread and waits for it to exit. Idempotent.
    pub fn stop(&self) {
        let sender = lock(&self.stop).take();
        drop(sender);
        let handle = lock(&self.handle).take();
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }

    /// Builds the aggregated sample tree from the counters collected so far.
    pub fn tree(&self) -> SampleTree {
        let inner = &self.inner;
        let nodes = inner.timings.node_snapshot();
        let threads = inner.timings.thread_count();
        let attributed = inner.samples_attributed.load(Ordering::Relaxed);

        let mut counts = vec![0u64; nodes.len()];
        for node in &nodes {
            counts[node.index as usize] = node.samples.load(Ordering::Relaxed);
        }

        let mut roots = Vec::new();
        let mut children: HashMap<u32, Vec<&'static Node>> = HashMap::new();
        for node in nodes {
            let index = node.index as usize;
            if counts[index] == 0 {
                continue;
            }
            match node.parent {
                Some(parent) if counts[parent.index as usize] > 0 => {
                    children.entry(parent.index).or_default().push(node);
                }
                _ => roots.push(node),
            }
        }
        roots.sort_by(|left, right| counts[right.index as usize].cmp(&counts[left.index as usize]));

        let mut built = Vec::with_capacity(roots.len());
        for root in roots {
            built.push(build_node(
                root,
                root.name.to_owned(),
                attributed,
                &mut children,
                &counts,
            ));
        }

        SampleTree {
            interval_ms: inner.interval.as_secs_f64() * 1000.0,
            sweeps: inner.sweeps.load(Ordering::Relaxed),
            threads,
            samples_taken: inner.samples_taken.load(Ordering::Relaxed),
            samples_attributed: attributed,
            roots: built,
        }
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Inner {
    timings: Arc<Timings>,
    interval: Duration,
    sweeps: AtomicU64,
    samples_taken: AtomicU64,
    samples_attributed: AtomicU64,
}

fn run(inner: Arc<Inner>, stop: mpsc::Receiver<()>) {
    while let Err(RecvTimeoutError::Timeout) = stop.recv_timeout(inner.interval) {
        sample_once(&inner);
    }
}

fn sample_once(inner: &Inner) {
    let nodes = inner.timings.node_snapshot();
    let slots = inner.timings.lock_slots();
    let mut taken = 0u64;
    let mut attributed = 0u64;

    for slot in slots.iter() {
        taken += 1;
        let depth = (slot.depth.load(Ordering::Acquire) as usize).min(MAX_DEPTH);
        if depth == 0 {
            continue;
        }
        attributed += 1;
        for frame in &slot.stack[..depth] {
            let index = frame.load(Ordering::Acquire) as usize;
            if let Some(node) = nodes.get(index) {
                node.add_sample();
            }
        }
    }
    drop(slots);

    inner.samples_taken.fetch_add(taken, Ordering::Relaxed);
    inner
        .samples_attributed
        .fetch_add(attributed, Ordering::Relaxed);
    inner.sweeps.fetch_add(1, Ordering::Relaxed);
}

fn build_node(
    node: &'static Node,
    path: String,
    attributed: u64,
    children: &mut HashMap<u32, Vec<&'static Node>>,
    counts: &[u64],
) -> SampleNode {
    let samples = counts[node.index as usize];
    let mut kids = children.remove(&node.index).unwrap_or_default();
    kids.sort_by(|left, right| counts[right.index as usize].cmp(&counts[left.index as usize]));
    let child_samples: u64 = kids.iter().map(|kid| counts[kid.index as usize]).sum();
    let self_samples = samples.saturating_sub(child_samples);
    let percent = if attributed == 0 {
        0.0
    } else {
        samples as f64 / attributed as f64 * 100.0
    };

    let mut child_nodes = Vec::with_capacity(kids.len());
    for kid in kids {
        let kid_path = format!("{path}/{}", kid.name);
        child_nodes.push(build_node(kid, kid_path, attributed, children, counts));
    }

    SampleNode {
        name: node.name.to_owned(),
        path,
        samples,
        self_samples,
        percent,
        children: child_nodes,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use super::*;

    #[test]
    fn samples_are_attributed_to_the_scope_the_thread_is_in() {
        let timings = Arc::new(Timings::new());
        let sampler = Sampler::start(&timings, Duration::from_millis(2)).expect("sampler thread");

        let worker = Arc::clone(&timings);
        let handle = thread::spawn(move || {
            let _outer = worker.scope("outer");
            {
                let _inner = worker.scope("inner");
                thread::sleep(Duration::from_millis(250));
            }
            {
                let _cold = worker.scope("cold");
                // The sampler ticks every 2 ms; a 5 ms window can be missed entirely on a busy
                // machine, so keep this scope wide enough for several ticks.
                thread::sleep(Duration::from_millis(40));
            }
        });
        handle.join().expect("worker panicked");
        sampler.stop();

        let tree = sampler.tree();
        assert!(tree.samples_attributed > 0, "{tree}");
        assert!(tree.samples_taken >= tree.samples_attributed, "{tree}");

        let outer = tree
            .roots
            .iter()
            .find(|node| node.name == "outer")
            .expect("outer root");
        let inner = outer
            .children
            .iter()
            .find(|node| node.name == "inner")
            .expect("inner child");
        let cold = outer
            .children
            .iter()
            .find(|node| node.name == "cold")
            .expect("cold child of outer");

        assert_eq!(inner.path, "outer/inner");
        assert_eq!(cold.path, "outer/cold");
        assert!(inner.samples > 0, "{tree}");
        assert!(inner.samples > cold.samples, "{tree}");
        assert!(inner.percent > 50.0, "{tree}");
        assert!(outer.samples >= inner.samples, "{tree}");
        assert!(inner.self_samples > 0, "{tree}");
        assert!(cold.percent > 0.0, "{tree}");
    }

    #[test]
    fn exports_plain_text_and_json() {
        let timings = Arc::new(Timings::new());
        let sampler = Sampler::start(&timings, Duration::from_millis(1)).expect("sampler thread");

        let worker = Arc::clone(&timings);
        let handle = thread::spawn(move || {
            let _work = worker.scope("work");
            thread::sleep(Duration::from_millis(120));
        });
        handle.join().expect("worker panicked");
        sampler.stop();

        let tree = sampler.tree();
        let text = tree.to_text();
        assert!(text.contains("work"), "{text}");

        let json = tree.to_json_pretty();
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        let roots = parsed["roots"].as_array().expect("roots array");
        assert!(roots.iter().any(|root| root["name"] == "work"), "{json}");
        assert!(parsed["samples_attributed"].as_u64().is_some(), "{json}");
    }

    #[test]
    fn stop_is_idempotent_and_safe_before_any_sample() {
        let timings = Arc::new(Timings::new());
        let sampler = Sampler::start(&timings, Duration::from_millis(50)).expect("sampler thread");
        sampler.stop();
        sampler.stop();
        let tree = sampler.tree();
        assert!(tree.roots.is_empty());
        assert_eq!(tree.samples_attributed, 0);
    }
}
