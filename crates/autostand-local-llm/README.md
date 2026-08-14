# autostand-local-llm

Process-isolated JSONL sidecar used by Autostand's built-in local AI provider.
It delegates inference to the platform-specific `llama-completion` binary when
available, with the bundled/pinned `llama-cli` as the compatible fallback.
Requests may either run without reusable state or
use llama.cpp's persistent prompt/KV cache; each inference process still exits
after its response, so the sidecar does not claim to keep model weights resident
in RAM.
