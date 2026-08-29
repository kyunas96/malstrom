---
name: tauri-threading
description: Threading and responsiveness rules for Tauri commands - when to use async fn vs spawn_blocking, why sync commands can freeze the UI (macOS beachball), how to report progress from background work, and how to avoid CPU contention with rayon. Use whenever writing or reviewing a #[tauri::command] that does file I/O, parsing, or other non-trivial CPU work.
---

# Tauri threading and UI responsiveness

## The one rule that matters most

**A `#[tauri::command]` fn only moves off the main thread if it's `async fn`.**
A plain sync `fn` command runs its *entire body* on the main thread, including
any blocking wait like `.collect()` on a rayon parallel iterator or
`rayon::join(...)`. This is true even if the actual work is spread across
other OS threads — the main thread blocks waiting for it, which means it
can't pump the event loop (window redraws, IPC, incoming events). On macOS
this shows up as a spinning beachball; it also silently swallows any
progress events you emit, since the webview can't repaint while the main
thread is blocked.

Wrong (blocks main thread even though rayon parallelizes internally):

```rust
#[tauri::command]
pub fn list_projects(root_path: String) -> Result<Vec<Project>, String> {
    paths.into_par_iter().map(|p| process(p)).collect() // main thread blocks here
}
```

Right (main thread just awaits; work runs on a background thread):

```rust
#[tauri::command]
pub async fn list_projects(root_path: String) -> Result<Vec<Project>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        paths.into_par_iter().map(|p| process(p)).collect()
    })
    .await
    .map_err(|e| e.to_string())?
}
```

Checklist for any command doing file I/O, XML/JSON parsing, image work, or
looping over more than a handful of items:
1. Make it `async fn`.
2. Wrap the actual work in `tauri::async_runtime::spawn_blocking(move || { ... })`.
3. `.await` the handle and flatten the `JoinError` with `.map_err(|e| e.to_string())?`.

## Reporting progress on long-running commands

A frontend spinner with no progress looks broken on anything but the
fastest scans, and users can't tell "still working" from "frozen apart
from the beachball you just fixed." Emit a Tauri event from inside the
`spawn_blocking` closure as work completes:

```rust
#[derive(Clone, serde::Serialize)]
struct Progress { completed: usize, total: usize }

#[tauri::command]
pub async fn list_projects(window: tauri::Window, root_path: String) -> Result<Vec<Project>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let total = paths.len();
        let completed = std::sync::atomic::AtomicUsize::new(0);
        paths.into_par_iter().map(|p| {
            let result = process(p);
            let done = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            let _ = window.emit("list-projects-progress", Progress { completed: done, total });
            result
        }).collect()
    })
    .await
    .map_err(|e| e.to_string())?
}
```

`tauri::Window` is `Send + Sync + 'static` and safe to move into the
closure/parallel iterator. On the frontend, listen with
`@tauri-apps/api/event`'s `listen()` before invoking the command, and call
the returned `unlisten()` in a `finally` block.

If a plain function needs to stay testable without a `Window` (e.g. it's
called directly from Rust integration tests), keep the core logic as a
sync helper taking a generic `on_progress: impl Fn(usize, usize) + Sync`
callback, and have the `#[tauri::command]` wrapper supply a closure that
calls `window.emit(...)`; a no-progress caller just passes `|_, _| {}`.

## Avoiding CPU contention even when off the main thread

Moving work off the main thread isn't sufficient by itself if it saturates
every CPU core: the OS scheduler doesn't guarantee the main thread gets a
core, so a rayon pool sized to `num_cpus` can still starve it. Two
complementary mitigations, done once at startup in `run()`:

1. **Leave a core free.** Cap rayon's global pool below `available_parallelism()`:

```rust
let workers = std::thread::available_parallelism()
    .map(|n| n.get().saturating_sub(1).max(1))
    .unwrap_or(1);
rayon::ThreadPoolBuilder::new()
    .num_threads(workers)
    .build_global()
    .expect("failed to configure rayon thread pool");
```

2. **Lower worker QoS on macOS**, so the scheduler always prefers the
   (user-interactive) main thread under contention, not just when there's a
   spare core:

```rust
#[cfg(target_os = "macos")]
fn lower_worker_thread_priority() {
    const QOS_CLASS_UTILITY: libc::c_uint = 0x11;
    extern "C" {
        fn pthread_set_qos_class_self_np(qos_class: libc::c_uint, relative_priority: libc::c_int) -> libc::c_int;
    }
    unsafe { pthread_set_qos_class_self_np(QOS_CLASS_UTILITY, 0); }
}

rayon::ThreadPoolBuilder::new()
    .num_threads(workers)
    .start_handler(|_| lower_worker_thread_priority())
    .build_global()
    .expect("failed to configure rayon thread pool");
```

Needs `libc` as a `[target.'cfg(target_os = "macos")'.dependencies]` entry
in `Cargo.toml`.

## Parallelizing work inside a single command

When one command does multiple independent CPU-bound steps against the
same data (e.g. several extraction passes over a parsed document), prefer
`rayon::join` for two steps, or `par_iter()`/`into_par_iter()` with
`try_fold` + `try_reduce` for a data-parallel loop that can still return a
`Result`:

```rust
let (a, b) = rayon::join(|| step_a(&doc), || step_b(&doc));
```

```rust
let tallies = items
    .into_par_iter()
    .try_fold(HashMap::new, |mut acc, item| -> Result<_> {
        // fold item into acc, propagate errors with `?`
        Ok(acc)
    })
    .try_reduce(HashMap::new, |mut a, b| {
        // merge two partial accumulators
        Ok(a)
    })?;
```

Nested rayon parallelism (an outer `par_iter` over files, each calling
into an inner `par_iter` over that file's items) is safe and expected —
rayon's work-stealing scheduler handles it without deadlocking.

## Quick diagnosis

- **Beachball / frozen window, no crash, command eventually returns** → the
  command (or something it calls) is sync and doing real work on the main
  thread. Make it `async fn` + `spawn_blocking`.
- **Command is `async fn` but still causes visible jank** → likely CPU
  contention from an unbounded rayon/thread pool; cap thread count and/or
  lower worker QoS.
- **Progress events fire in Rust logs but never render** → almost always
  the main-thread-blocking issue above, not a frontend bug — the webview
  can't repaint while the main thread is stuck inside the sync command.
