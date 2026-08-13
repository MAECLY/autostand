/**
 * Settings → Providers: one card per LLM provider.
 *
 * The card owns no data of its own — status comes from `list_llm_providers`
 * and every write is delegated upward so the Settings route can debounce the
 * `set_config` round-trip described in `docs/specs/configuration.md`.
 *
 * SECURITY: the plaintext API key exists only in `keyDraft`, only while the
 * dialog is open, and only travels to `onSaveKey` (which writes it to the OS
 * keychain). It is never logged, echoed back, put in a query cache, or written
 * to config JSON — `ProviderConfig.api_key_ref` holds a keychain name, not a key.
 */

import { useMemo, useState } from "react";
import { KeyRound, Zap } from "lucide-react";

import { Badge } from "@autostand/ui/components/badge";
import { Button } from "@autostand/ui/components/button";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@autostand/ui/components/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@autostand/ui/components/dialog";
import { Input } from "@autostand/ui/components/input";
import { Label } from "@autostand/ui/components/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@autostand/ui/components/select";

import { useProviderModels } from "@/hooks/use-providers";
import { toAppError } from "@/lib/error";
import type {
  ApiKeyMode,
  LlmProviderConfig,
  ProviderMode,
  ProviderTestMode,
  TestProviderResult,
} from "@/lib/types";
import { cn } from "@/lib/utils";

/** Sentinel so the Select can offer a free-text fallback without colliding with a model id. */
const CUSTOM_MODEL = "__custom__";

const PROVIDER_MODES = [
  "CliFirst",
  "ApiFallback",
  "CliOnly",
  "ApiOnly",
] as const satisfies readonly ProviderMode[];

const MODE_LABELS: Record<ProviderMode, string> = {
  CliFirst: "CLI first",
  ApiFallback: "API fallback",
  CliOnly: "CLI only",
  ApiOnly: "API only",
};

const API_KEY_LABELS: Record<ApiKeyMode, string> = {
  keychain: "Key in keychain",
  env: "Key from env",
  none: "No key",
};

function isProviderMode(value: string): value is ProviderMode {
  return (PROVIDER_MODES as readonly string[]).includes(value);
}

/** Which transport a "Test" press should probe, given the configured mode. */
function testModeFor(mode: ProviderMode): ProviderTestMode {
  return mode === "CliOnly" || mode === "CliFirst" ? "cli" : "api";
}

export interface ProviderCardProps {
  provider: LlmProviderConfig;
  /** `ProviderConfig.timeout_secs` — lives in config, not in the status DTO. */
  timeoutSecs: number;
  isPreferred: boolean;
  onSetPreferred: () => void;
  onSetMode: (mode: ProviderMode) => void;
  onSetModel: (model: string) => void;
  onSetTimeout: (seconds: number) => void;
  onTest: (mode: ProviderTestMode) => Promise<TestProviderResult>;
  onSaveKey: (key: string) => Promise<void>;
}

export function ProviderCard({
  provider,
  timeoutSecs,
  isPreferred,
  onSetPreferred,
  onSetMode,
  onSetModel,
  onSetTimeout,
  onTest,
  onSaveKey,
}: ProviderCardProps) {
  const [modelDraft, setModelDraft] = useState(provider.model);
  const [customModel, setCustomModel] = useState(false);
  const [timeoutDraft, setTimeoutDraft] = useState(String(timeoutSecs));
  const [testResult, setTestResult] = useState<TestProviderResult | null>(null);
  const [testing, setTesting] = useState(false);
  const [keyDialogOpen, setKeyDialogOpen] = useState(false);
  const [keyDraft, setKeyDraft] = useState("");
  const [savingKey, setSavingKey] = useState(false);

  const modelsQuery = useProviderModels(provider.id);
  const discovered = modelsQuery.data;
  const modelOptions = useMemo(() => {
    const seen = new Set<string>();
    const options: string[] = [];
    for (const id of [...(discovered ?? []), provider.model]) {
      const trimmed = id.trim();
      if (trimmed.length === 0 || seen.has(trimmed)) continue;
      seen.add(trimmed);
      options.push(trimmed);
    }
    return options;
  }, [discovered, provider.model]);
  // An empty probe is not a verdict — CLI-only setups keep the free-text field.
  const showModelSelect = (discovered ?? []).length > 0;

  const modelId = `provider-${provider.id}-model`;
  const modeId = `provider-${provider.id}-mode`;
  const timeoutId = `provider-${provider.id}-timeout`;
  const keyId = `provider-${provider.id}-key`;

  function commitModel() {
    const next = modelDraft.trim();
    if (next.length === 0) {
      setModelDraft(provider.model);
      return;
    }
    if (next !== provider.model) onSetModel(next);
  }

  function commitTimeout() {
    const next = Number.parseInt(timeoutDraft, 10);
    if (Number.isNaN(next) || next <= 0) {
      setTimeoutDraft(String(timeoutSecs));
      return;
    }
    if (next !== timeoutSecs) onSetTimeout(next);
  }

  async function runTest() {
    setTesting(true);
    setTestResult(null);
    try {
      setTestResult(await onTest(testModeFor(provider.mode)));
    } catch (error) {
      setTestResult({
        ok: false,
        message: toAppError(error).message,
        latency_ms: 0,
      });
    } finally {
      setTesting(false);
    }
  }

  function setKeyDialog(open: boolean) {
    setKeyDialogOpen(open);
    // Drop the plaintext key the moment the dialog goes away.
    if (!open) setKeyDraft("");
  }

  async function saveKey() {
    if (keyDraft.length === 0) return;
    setSavingKey(true);
    try {
      await onSaveKey(keyDraft);
      setKeyDialog(false);
    } catch {
      // `useStoreApiKey` owns the toast; keep the dialog open so the user can
      // retry, and never surface the rejection text next to the key field.
    } finally {
      setSavingKey(false);
    }
  }

  return (
    <Card>
      <CardHeader className="flex flex-row items-start justify-between gap-4">
        <div className="flex items-center gap-3">
          <button
            type="button"
            role="radio"
            aria-checked={isPreferred}
            aria-label={`Prefer ${provider.label}`}
            onClick={onSetPreferred}
            className={cn(
              "flex size-4 shrink-0 items-center justify-center rounded-full border",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
              isPreferred ? "border-primary" : "border-border-strong",
            )}
          >
            {isPreferred ? (
              <span className="size-2 rounded-full bg-primary" />
            ) : null}
          </button>
          <div>
            <CardTitle>{provider.label}</CardTitle>
            {isPreferred ? (
              <p className="text-xs text-primary">Preferred provider</p>
            ) : null}
          </div>
        </div>

        <div className="flex flex-wrap items-center justify-end gap-2">
          <Badge variant={provider.cli.found ? "success" : "secondary"}>
            {provider.cli.found ? "CLI found" : "CLI not found"}
          </Badge>
          <Badge variant={provider.api_key.set ? "success" : "secondary"}>
            {API_KEY_LABELS[provider.api_key.mode]}
          </Badge>
        </div>
      </CardHeader>

      <CardContent className="flex flex-col gap-4">
        {provider.cli.found ? (
          <p className="font-mono text-xs text-muted-foreground">
            {provider.cli.path}
            {provider.cli.version.length > 0 ? ` · ${provider.cli.version}` : ""}
          </p>
        ) : null}

        <div className="grid gap-4 sm:grid-cols-3">
          <div className="flex flex-col gap-2">
            <Label htmlFor={modelId}>Model</Label>
            {showModelSelect ? (
              <>
                <Select
                  value={customModel ? CUSTOM_MODEL : modelDraft || undefined}
                  onValueChange={(value) => {
                    if (value === CUSTOM_MODEL) {
                      setCustomModel(true);
                      return;
                    }
                    setCustomModel(false);
                    setModelDraft(value);
                    if (value !== provider.model) onSetModel(value);
                  }}
                >
                  <SelectTrigger id={modelId} className="font-mono">
                    <SelectValue placeholder="Choose a model" />
                  </SelectTrigger>
                  <SelectContent>
                    {modelOptions.map((id) => (
                      <SelectItem key={id} value={id} className="font-mono">
                        {id}
                      </SelectItem>
                    ))}
                    <SelectItem value={CUSTOM_MODEL}>Custom…</SelectItem>
                  </SelectContent>
                </Select>
                {customModel ? (
                  <Input
                    value={modelDraft}
                    spellCheck={false}
                    autoComplete="off"
                    className="font-mono"
                    placeholder="model-id"
                    aria-label={`${provider.label} custom model`}
                    onChange={(event) => setModelDraft(event.target.value)}
                    onBlur={commitModel}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") commitModel();
                    }}
                  />
                ) : null}
              </>
            ) : (
              <Input
                id={modelId}
                value={modelDraft}
                spellCheck={false}
                autoComplete="off"
                className="font-mono"
                onChange={(event) => setModelDraft(event.target.value)}
                onBlur={commitModel}
                onKeyDown={(event) => {
                  if (event.key === "Enter") commitModel();
                }}
              />
            )}
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor={modeId}>Mode</Label>
            <Select
              value={provider.mode}
              onValueChange={(value) => {
                if (isProviderMode(value)) onSetMode(value);
              }}
            >
              <SelectTrigger id={modeId}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {PROVIDER_MODES.map((mode) => (
                  <SelectItem key={mode} value={mode}>
                    {MODE_LABELS[mode]}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor={timeoutId}>Timeout (s)</Label>
            <Input
              id={timeoutId}
              type="number"
              min={1}
              inputMode="numeric"
              value={timeoutDraft}
              onChange={(event) => setTimeoutDraft(event.target.value)}
              onBlur={commitTimeout}
              onKeyDown={(event) => {
                if (event.key === "Enter") commitTimeout();
              }}
            />
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            variant="outline"
            disabled={testing}
            onClick={() => {
              void runTest();
            }}
          >
            <Zap className="size-4" aria-hidden="true" />
            {testing ? "Testing…" : "Test"}
          </Button>

          <Button
            type="button"
            variant="secondary"
            onClick={() => setKeyDialog(true)}
          >
            <KeyRound className="size-4" aria-hidden="true" />
            Store key
          </Button>

          {testResult !== null ? (
            <>
              <Badge variant={testResult.ok ? "success" : "error"}>
                {testResult.ok
                  ? `OK · ${testResult.latency_ms} ms`
                  : "Test failed"}
              </Badge>
              {testResult.message.length > 0 ? (
                <span className="min-w-0 truncate text-xs text-muted-foreground">
                  {testResult.message}
                </span>
              ) : null}
            </>
          ) : null}
        </div>
      </CardContent>

      <Dialog open={keyDialogOpen} onOpenChange={setKeyDialog}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{provider.label} API key</DialogTitle>
            <DialogDescription>
              Stored in the OS keychain as{" "}
              <code className="font-mono">autostand.{provider.id}</code>.
              The key never reaches the config file and is not readable from the
              UI afterwards.
            </DialogDescription>
          </DialogHeader>

          <div className="flex flex-col gap-2">
            <Label htmlFor={keyId}>API key</Label>
            <Input
              id={keyId}
              type="password"
              value={keyDraft}
              autoComplete="off"
              spellCheck={false}
              placeholder="Paste the key"
              onChange={(event) => setKeyDraft(event.target.value)}
            />
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setKeyDialog(false)}
            >
              Cancel
            </Button>
            <Button
              type="button"
              disabled={savingKey || keyDraft.length === 0}
              onClick={() => {
                void saveKey();
              }}
            >
              {savingKey ? "Saving…" : "Save to keychain"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  );
}
