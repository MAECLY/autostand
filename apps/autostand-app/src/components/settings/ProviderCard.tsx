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

import { useEffect, useMemo, useState } from "react";
import { ArrowDown, ArrowUp, ChevronDown, KeyRound, Zap } from "lucide-react";

import { Badge } from "@autostand/ui/components/badge";
import { Button } from "@autostand/ui/components/button";
import {
  Card,
  CardContent,
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
import { Switch } from "@autostand/ui/components/switch";
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
  canMoveUp: boolean;
  canMoveDown: boolean;
  onSetPreferred: () => void;
  onSetEnabled: (enabled: boolean) => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
  onSetMode: (mode: ProviderMode) => void;
  onSetModel: (model: string) => void;
  onSetTimeout: (seconds: number) => void;
  onTest: (mode: ProviderTestMode) => Promise<TestProviderResult>;
  onSaveKey: (key: string) => Promise<void>;
  /** Only one provider should expose its advanced controls at a time. */
  expanded?: boolean;
  onToggleDetails?: () => void;
}

export function ProviderCard({
  provider,
  timeoutSecs,
  isPreferred,
  canMoveUp,
  canMoveDown,
  onSetPreferred,
  onSetEnabled,
  onMoveUp,
  onMoveDown,
  onSetMode,
  onSetModel,
  onSetTimeout,
  onTest,
  onSaveKey,
  expanded = true,
  onToggleDetails,
}: ProviderCardProps) {
  const [modelDraft, setModelDraft] = useState(provider.model);
  const [customModel, setCustomModel] = useState(false);
  const [timeoutDraft, setTimeoutDraft] = useState(String(timeoutSecs));
  const [testResult, setTestResult] = useState<TestProviderResult | null>(null);
  const [testing, setTesting] = useState(false);
  const [keyDialogOpen, setKeyDialogOpen] = useState(false);
  const [keyDraft, setKeyDraft] = useState("");
  const [savingKey, setSavingKey] = useState(false);
  const isBuiltinLocal = provider.id === "builtin-local";

  useEffect(() => {
    setModelDraft(provider.model);
    setCustomModel(false);
  }, [provider.model]);

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
  const enabledId = `provider-${provider.id}-enabled`;

  const cliLabel = provider.cli.found
    ? isBuiltinLocal
      ? "Runtime found"
      : "CLI found"
    : isBuiltinLocal
      ? "Runtime missing"
      : "CLI not found";
  const cliDetail = provider.cli.found
    ? provider.cli.path +
      (provider.cli.version.length > 0 ? ` · ${provider.cli.version}` : "")
    : null;

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
      {/* One spine. The preference radio owns a fixed 1rem gutter and the header
          is a grid, so every provider's name, status strip and expanded controls
          start on the same left edge however wide the right cluster grows.
          A plain div rather than CardHeader: that component hard-codes a single
          `gap`, which would fight the asymmetric row/column gaps this grid needs. */}
      <div className="grid grid-cols-[1rem_minmax(0,1fr)_auto] items-center gap-x-3 gap-y-1.5 p-3">
        <button
          type="button"
          role="radio"
          aria-checked={isPreferred}
          aria-label={`Prefer ${provider.label}`}
          onClick={onSetPreferred}
          className={cn(
            "col-start-1 row-start-1 flex size-4 shrink-0 items-center justify-center rounded-full border",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
            isPreferred ? "border-primary" : "border-border-strong",
          )}
        >
          {isPreferred ? (
            <span className="size-2 rounded-full bg-primary" />
          ) : null}
        </button>

        <div className="col-start-2 row-start-1 flex min-w-0 items-center gap-2">
          <CardTitle className="truncate text-sm">{provider.label}</CardTitle>
          {isPreferred ? (
            <Badge
              variant="outline"
              className="shrink-0 border-primary/50 px-1.5 text-primary"
            >
              Preferred
            </Badge>
          ) : null}
        </div>

        <div className="col-start-3 row-start-1 flex shrink-0 items-center gap-1">
          <Label
            htmlFor={enabledId}
            className="mr-1 text-xs font-normal text-muted-foreground"
          >
            Enabled
          </Label>
          <Switch
            id={enabledId}
            className="mr-1"
            checked={provider.enabled}
            onCheckedChange={onSetEnabled}
          />
          {/* Reordering is a list operation, so it stays reachable without
              opening the card — the failover chain is the thing this screen
              is about. */}
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="size-7"
            disabled={!canMoveUp}
            aria-label={`Move ${provider.label} up`}
            onClick={onMoveUp}
          >
            <ArrowUp aria-hidden="true" />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="size-7"
            disabled={!canMoveDown}
            aria-label={`Move ${provider.label} down`}
            onClick={onMoveDown}
          >
            <ArrowDown aria-hidden="true" />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs"
            aria-expanded={expanded}
            onClick={onToggleDetails}
          >
            {expanded ? "Hide" : "Configure"}
            <ChevronDown
              className={cn("transition-transform", expanded && "rotate-180")}
              aria-hidden="true"
            />
          </Button>
        </div>

        {/* Status strip, constant in position. Collapsed it reports what this
            card would actually run with; mode and model used to be invisible
            until the card was opened. */}
        <div className="col-start-2 row-start-2 flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
          <Badge variant={provider.cli.found ? "success" : "secondary"}>
            {cliLabel}
          </Badge>
          {!isBuiltinLocal ? (
            <Badge variant={provider.api_key.set ? "success" : "secondary"}>
              {API_KEY_LABELS[provider.api_key.mode]}
            </Badge>
          ) : null}
          {expanded ? (
            cliDetail !== null ? (
              <span className="min-w-0 truncate font-mono">{cliDetail}</span>
            ) : null
          ) : (
            <>
              {!isBuiltinLocal ? (
                <>
                  <span>{MODE_LABELS[provider.mode]}</span>
                  <span aria-hidden="true">·</span>
                </>
              ) : null}
              <span className="min-w-0 truncate font-mono">
                {provider.model.length > 0 ? provider.model : "No model set"}
              </span>
            </>
          )}
        </div>
      </div>

      {/* pl-10 = the header's 0.75rem padding + the 1rem radio gutter + the
          0.75rem column gap, so the controls line up under the provider name. */}
      {expanded ? <CardContent className="flex flex-col gap-3 border-t border-border p-3 pl-10">
        {isBuiltinLocal ? (
          <div className="flex flex-col gap-1">
            <p className="text-xs font-medium text-muted-foreground">
              Active model
            </p>
            <p className="font-mono text-sm">
              {provider.model.length > 0
                ? provider.model
                : "No local model selected"}
            </p>
            <p className="text-xs text-muted-foreground">
              Download and switch models in the Local AI tab. Built-in AI runs on
              this machine and never uses an API key.
            </p>
          </div>
        ) : <div className="grid gap-3 sm:grid-cols-[minmax(0,2fr)_minmax(0,1fr)_6rem]">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor={modelId} className="text-xs">Model</Label>
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
                  <SelectTrigger id={modelId} className="h-9 font-mono">
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
                    className="h-9 font-mono"
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
                className="h-9 font-mono"
                onChange={(event) => setModelDraft(event.target.value)}
                onBlur={commitModel}
                onKeyDown={(event) => {
                  if (event.key === "Enter") commitModel();
                }}
              />
            )}
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor={modeId} className="text-xs">Mode</Label>
            <Select
              value={provider.mode}
              onValueChange={(value) => {
                if (isProviderMode(value)) onSetMode(value);
              }}
            >
              <SelectTrigger id={modeId} className="h-9">
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

          <div className="flex flex-col gap-1.5">
            <Label htmlFor={timeoutId} className="text-xs">Timeout (s)</Label>
            <Input
              id={timeoutId}
              type="number"
              min={1}
              inputMode="numeric"
              className="h-9 tabular-nums"
              value={timeoutDraft}
              onChange={(event) => setTimeoutDraft(event.target.value)}
              onBlur={commitTimeout}
              onKeyDown={(event) => {
                if (event.key === "Enter") commitTimeout();
              }}
            />
          </div>
        </div>}

        <div className="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={testing}
            onClick={() => {
              void runTest();
            }}
          >
            <Zap aria-hidden="true" />
            {testing ? "Testing…" : isBuiltinLocal ? "Test local AI" : "Test"}
          </Button>

          {!isBuiltinLocal ? (
            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={() => setKeyDialog(true)}
            >
              <KeyRound aria-hidden="true" />
              Store key
            </Button>
          ) : null}

          {testResult !== null ? (
            <>
              <Badge
                variant={testResult.ok ? "success" : "error"}
                className="tabular-nums"
              >
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
      </CardContent> : null}

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
