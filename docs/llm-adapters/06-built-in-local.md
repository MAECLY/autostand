# Built-in Local AI

Autostand includes a curated GGUF model manager and the `builtin-local` LLM provider. It is separate from Ollama: model files live under `<state_dir>/models/local`, no API key is used, and inference does not open a listening network socket.

## Curated catalog

The catalog is compiled into `commands/local_models.rs`; arbitrary download URLs cannot cross IPC.

| Id | Display name | Tier | Quantization | Download size | Context | License |
| --- | --- | --- | --- | ---: | ---: | --- |
| `gemma3:1b` | Gemma 3 1B (Fast) | `extra_small` | Q8_0 | 1,069,306,624 bytes | 32,768 | Gemma Terms of Use |
| `qwen3.5:2b` | Qwen 3.5 2B (Balanced) | `small` | Q4_K_M | 1,280,835,840 bytes | 32,768 | Apache-2.0 |
| `gemma3:4b` | Gemma 3 4B (Balanced) | `medium` | Q4_K_M | 2,489,758,112 bytes | 32,768 | Gemma Terms of Use |
| `qwen3.5:4b` | Qwen 3.5 4B (High Quality) | `large` | Q4_K_M | 2,740,937,888 bytes | 32,768 | Apache-2.0 |

Each entry pins an immutable Hugging Face revision, exact byte count, SHA-256, filename, context length, license URL, and any required terms version. Model licenses are independent of the Autostand repository license.

Gemma downloads require explicit acceptance of the catalog's exact terms version (`2025-03-25`). Acceptance and the selected model are stored together in `local-models.json`; changing the catalog terms version requires acceptance again. Qwen entries do not require an additional in-app acceptance step.

## Download lifecycle

Downloads begin only after a user action in Settings → Local AI. The command accepts a catalog `modelId`, never a URL or destination path.

1. Stream into `<filename>.part`; use an HTTP Range request when a partial file exists.
2. Emit `local-model-progress` with downloaded/total bytes and observed bytes per second.
3. Retain `.part` on cancellation so the next download can resume.
4. Require the exact catalog size, compute SHA-256, and reject mismatches as `corrupted`.
5. Rename the verified partial file to the final GGUF name and sync the directory.

`list_local_models` derives `not_downloaded`, `downloading`, `available`, `corrupted`, or `error` from disk. A failed non-cancelled download persists a small `.error` state for the UI; retry removes it. Selection is allowed only for an available, size-valid entry and is persisted in `local-models.json`. Deleting a model removes its final, partial, and error files and clears selection when necessary.

## Provider and sidecar

`builtin-local` implements `LlmAdapter` as a CLI-only provider. An empty configured model uses the selection in `local-models.json`; a catalog id selects that model directly. An explicit absolute `.gguf` path is also accepted only when it already exists. A missing selection/file produces `model_not_installed` and lets the ordered provider chain continue.

The adapter starts `autostand-local-llm` as a process-isolated sidecar, sends one JSONL protocol-v1 request on stdin, and reads the ready/result response from stdout. The sidecar delegates generation to a sibling `llama-cli` (or the test/development override `AUTOSTAND_LLAMA_CLI`) with bounded context, output tokens, and temperature. Each request is one-shot and both child processes receive `AUTOSTAND_RENDER=1`.

Before generation, the adapter applies the catalog model's Gemma or Qwen chat template and neutralizes matching control markers in gathered user content. This prevents an activity note containing `<|im_end|>`/turn markers from breaking out of its prompt role.

Runtime lookup expects `autostand-local-llm` and the platform-specific llama.cpp `llama-cli` beside the application executable. A configured `ProviderConfig.cli_path` may override only the sidecar path; the provider never falls back to an HTTP API.

Release builds use `tauri.release.conf.json`: the release workflow compiles the Rust sidecar for the exact target, builds the pinned llama.cpp `llama-cli`, copies both into Tauri's target-suffixed `binaries/` layout, and enables them as `externalBin` entries. Ordinary source/development runs do not invoke that release-only build step; place both binaries as siblings (or put the sidecar on `PATH` and set `AUTOSTAND_LLAMA_CLI`).

## Security boundary

- GGUF and state paths are chosen from the built-in catalog or an existing absolute `.gguf` path; download callers cannot inject a destination.
- Downloads use pinned revisions and SHA-256 verification before installation.
- Inference uses stdin/stdout JSONL and opens no port.
- `AUTOSTAND_RENDER=1` prevents local generation from triggering a recursive standup run.
- stderr from llama.cpp is converted to a stable sidecar error code before provider telemetry; it is not copied into notifications or audit health fields.
- Model weights and license acceptance remain outside the repository in the platform state directory and are never committed.

## Settings behavior

The Local AI tab shows tier, quality, GGUF size, context, license, status, progress, and selection. Actions are Download/Resume, Cancel, Use model, and Delete. Gemma displays the required terms action before download. Models are never downloaded or selected automatically.
