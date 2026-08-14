# autostand-runlog

Neutral run-log abstraction shared by every autostand crate.

Two responsibilities:

1. **A sink.** [`RunSink`] receives [`LogLine`]s. The current sink lives in a Tokio
   task-local, so domain code deep inside the pipeline can log to whatever the caller
   attached (the Tauri Terminal panel, a test recorder, nothing at all) without taking a
   dependency on `tauri`.
2. **The single process spawner.** `proc::run_process` / `proc::run_process_piped` are the
   only places in the workspace allowed to start a child process. Every spawn therefore
   shows up in the Terminal, with a privacy policy that is enforced in one file instead of
   at thirty call sites.

## The `tokio::spawn` trap

Task-locals are **not** inherited by `tokio::spawn`. A task spawned without re-entering the
scope logs into `NullSink` and its work disappears from the Terminal:

```rust
// WRONG — the child task has no sink.
tokio::spawn(async move { do_work().await });

// RIGHT — `inherit` captures the caller's sink and re-installs it inside the task.
tokio::spawn(autostand_runlog::inherit(async move { do_work().await }));
```

Use [`inherit`] for `tokio::spawn`, `JoinSet::spawn` and `tauri::async_runtime::spawn`.
Use [`scoped`] when you have the sink in hand (opening a run) rather than inheriting one.

## Privacy

`docs/specs/audit.md` forbids stderr and API response bodies in telemetry, and a git/gh
argv carries repository paths and branch names. So:

- `StreamPolicy::Summary` never renders argv, stdout or stderr — only the program's file
  name (or a caller-supplied `label`), the exit code, the duration and byte counts.
- `StreamPolicy::Silent` emits nothing at all.
- `StreamPolicy::Lines` echoes child stdout and is opt-in per call site. It must never be
  used for an LLM render: that stdout *is* the standup body.
- stderr is captured into `ProcOutput` for the caller's error mapping and is never logged
  by this crate.
