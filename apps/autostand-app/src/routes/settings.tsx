/**
 * Settings — providers, data sources, paths, scheduler.
 * Spec: `docs/tauri/04-frontend-stack.md` § Key pages, `docs/specs/configuration.md`.
 *
 * Every write to `AppConfig` goes through `useSetConfig` (which owns the
 * "Settings saved" toast), so no tab persists config on its own. The two
 * exceptions are backend-owned writes with their own commands and toasts:
 * `toggle_data_source` and `set_scheduler_schedule`.
 */

import { useState, type ReactNode } from "react";
import { createFileRoute } from "@tanstack/react-router";
import { AlertTriangle, Save } from "lucide-react";

import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@autostand/ui/components/alert";
import { Button } from "@autostand/ui/components/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@autostand/ui/components/card";
import { Separator } from "@autostand/ui/components/separator";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@autostand/ui/components/tabs";

import { DataSourceToggle } from "@/components/settings/DataSourceToggle";
import { PathInput } from "@/components/settings/PathInput";
import { ProviderCard } from "@/components/settings/ProviderCard";
import { RepoTable } from "@/components/settings/RepoTable";
import { SchedulerForm } from "@/components/settings/SchedulerForm";
import { useConfig, useSetConfig } from "@/hooks/use-config";
import { useDataSources } from "@/hooks/use-data-sources";
import {
  useLlmProviders,
  useStoreApiKey,
  useTestProvider,
} from "@/hooks/use-providers";
import { toAppError } from "@/lib/error";
import type {
  AppConfig,
  LlmProviderConfig,
  ProviderConfig,
} from "@/lib/types";

export const Route = createFileRoute("/settings")({
  component: SettingsPage,
});

/** `docs/specs/configuration.md` § defaults. */
const FALLBACK_TIMEOUT_SECS = 180;

/** Ollama waits on local inference, which is slow on a cold model load. */
const TIMEOUT_OVERRIDES: Record<string, number> = { ollama: 300 };

function defaultTimeoutSecs(providerId: string): number {
  return TIMEOUT_OVERRIDES[providerId] ?? FALLBACK_TIMEOUT_SECS;
}

// ── Shared tab chrome ─────────────────────────────────────────────────────

interface LoadErrorProps {
  title: string;
  error: unknown;
}

function LoadError({ title, error }: LoadErrorProps) {
  const appError = toAppError(error);
  return (
    <Alert variant="destructive">
      <AlertTriangle />
      <AlertTitle>{title}</AlertTitle>
      <AlertDescription>
        <p className="font-mono text-xs">{appError.code}</p>
        <p>{appError.message}</p>
      </AlertDescription>
    </Alert>
  );
}

interface TabSkeletonProps {
  rows: number;
}

function TabSkeleton({ rows }: TabSkeletonProps) {
  return (
    <div className="flex flex-col gap-3" aria-busy="true" aria-label="Loading">
      {Array.from({ length: rows }, (_, index) => (
        <div key={index} className="h-20 animate-pulse rounded-lg bg-muted" />
      ))}
    </div>
  );
}

interface SectionProps {
  title: string;
  description: string;
  children: ReactNode;
}

function Section({ title, description, children }: SectionProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
        <CardDescription>{description}</CardDescription>
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  );
}

// ── Providers ─────────────────────────────────────────────────────────────

/** Shape a provider takes in config when the user first touches its card. */
function newProviderConfig(id: string): ProviderConfig {
  return {
    id,
    enabled: true,
    mode: "CliFirst",
    model: "",
    cli_path: null,
    api_key_ref: null,
    api_base_url: null,
    timeout_secs: defaultTimeoutSecs(id),
  };
}

function upsertProvider(
  config: AppConfig,
  id: string,
  changes: Partial<ProviderConfig>,
): AppConfig {
  const stored = config.llm.providers.find((provider) => provider.id === id);
  const next: ProviderConfig = {
    ...(stored ?? newProviderConfig(id)),
    ...changes,
    id,
  };
  const providers = stored
    ? config.llm.providers.map((provider) =>
        provider.id === id ? next : provider,
      )
    : [...config.llm.providers, next];

  return { ...config, llm: { ...config.llm, providers } };
}

function ProvidersTab() {
  const { data: config } = useConfig();
  const providers = useLlmProviders();
  const setConfig = useSetConfig();
  const testProvider = useTestProvider();
  const storeApiKey = useStoreApiKey();

  function patchProvider(id: string, changes: Partial<ProviderConfig>) {
    if (config === undefined) return;
    setConfig.mutate(upsertProvider(config, id, changes));
  }

  if (providers.isPending) return <TabSkeleton rows={3} />;
  if (providers.isError) {
    return (
      <LoadError title="Could not list providers" error={providers.error} />
    );
  }

  return (
    <div className="flex flex-col gap-4">
      {providers.data.map((provider) => {
        const stored = config?.llm.providers.find(
          (candidate) => candidate.id === provider.id,
        );
        // `list_llm_providers` reports live CLI/keychain status but not the
        // user's saved choices, so config wins on the fields it owns.
        const merged: LlmProviderConfig =
          stored === undefined
            ? provider
            : {
                ...provider,
                enabled: stored.enabled,
                mode: stored.mode,
                model: stored.model,
              };

        return (
          <ProviderCard
            key={provider.id}
            provider={merged}
            timeoutSecs={stored?.timeout_secs ?? defaultTimeoutSecs(provider.id)}
            isPreferred={config?.llm.preferred_provider === provider.id}
            onSetPreferred={() => {
              if (config === undefined) return;
              setConfig.mutate({
                ...config,
                llm: { ...config.llm, preferred_provider: provider.id },
              });
            }}
            onSetMode={(mode) => patchProvider(provider.id, { mode })}
            onSetModel={(model) => patchProvider(provider.id, { model })}
            onSetTimeout={(seconds) =>
              patchProvider(provider.id, { timeout_secs: seconds })
            }
            onTest={(mode) =>
              testProvider.mutateAsync({ provider: provider.id, mode })
            }
            onSaveKey={(key) =>
              storeApiKey.mutateAsync({ provider: provider.id, key })
            }
          />
        );
      })}
    </div>
  );
}

// ── Data sources ──────────────────────────────────────────────────────────

function DataSourcesTab() {
  const sources = useDataSources();

  if (sources.isPending) return <TabSkeleton rows={4} />;
  if (sources.isError) {
    return (
      <LoadError title="Could not list data sources" error={sources.error} />
    );
  }

  return (
    <Section
      title="Data sources"
      description="Read-only collectors the gather step queries. local-git is authoritative and always on."
    >
      <div className="flex flex-col">
        {sources.data.map((source) => (
          <div
            key={source.id}
            className="border-b border-border last:border-b-0"
          >
            <DataSourceToggle source={source} />
          </div>
        ))}
      </div>
    </Section>
  );
}

// ── Paths ─────────────────────────────────────────────────────────────────

interface PathDraft {
  github_dir: string;
  dailies_dir: string;
}

function PathsTab() {
  const config = useConfig();
  const setConfig = useSetConfig();
  // `null` means "no local edit yet", so the fields track the loaded config
  // until the user types — no effect needed to resync after a save.
  const [draft, setDraft] = useState<PathDraft | null>(null);

  if (config.isPending) return <TabSkeleton rows={2} />;
  if (config.isError) {
    return <LoadError title="Could not load settings" error={config.error} />;
  }

  // Bound before the callbacks close over it: narrowing from the guards above
  // does not survive into a deferred closure.
  const current = config.data;
  const saved: PathDraft = {
    github_dir: current.github_dir,
    dailies_dir: current.dailies_dir,
  };
  const value = draft ?? saved;
  const dirty =
    value.github_dir !== saved.github_dir ||
    value.dailies_dir !== saved.dailies_dir;

  function patchDraft(changes: Partial<PathDraft>) {
    setDraft((previous) => ({ ...(previous ?? saved), ...changes }));
  }

  function save() {
    if (!dirty) return;
    setConfig.mutate(
      { ...current, ...value },
      // Drop the draft so the inputs follow the reloaded config again.
      { onSuccess: () => setDraft(null) },
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <Section
        title="Paths"
        description="Where autostand reads repositories from and writes standup files to."
      >
        <div className="flex flex-col gap-6">
          <PathInput
            label="GitHub directory"
            field="github_dir"
            value={value.github_dir}
            placeholder="~/Documents/Github"
            onChange={(github_dir) => patchDraft({ github_dir })}
          />
          <PathInput
            label="Dailies directory"
            field="dailies_dir"
            value={value.dailies_dir}
            placeholder="~/Sync/Github_Dailies/dailies"
            onChange={(dailies_dir) => patchDraft({ dailies_dir })}
          />

          <Separator />

          <div className="flex items-center justify-end gap-3">
            {dirty && (
              <span className="text-sm text-muted-foreground">
                Unsaved changes
              </span>
            )}
            <Button
              type="button"
              disabled={!dirty || setConfig.isPending}
              onClick={save}
            >
              <Save aria-hidden="true" />
              {setConfig.isPending ? "Saving…" : "Save paths"}
            </Button>
          </div>
        </div>
      </Section>

      <Card>
        <CardContent className="pt-6">
          <RepoTable />
        </CardContent>
      </Card>
    </div>
  );
}

// ── Page ──────────────────────────────────────────────────────────────────

function SettingsPage() {
  return (
    <div className="flex flex-col gap-6 p-6">
      <header className="min-w-0">
        <h2 className="text-lg font-semibold text-foreground">Settings</h2>
        <p className="text-sm text-muted-foreground">
          Providers, data sources, paths and the compile schedule.
        </p>
      </header>

      <Tabs defaultValue="providers">
        <TabsList>
          <TabsTrigger value="providers">Providers</TabsTrigger>
          <TabsTrigger value="data-sources">Data Sources</TabsTrigger>
          <TabsTrigger value="paths">Paths</TabsTrigger>
          <TabsTrigger value="scheduler">Scheduler</TabsTrigger>
        </TabsList>

        <TabsContent value="providers">
          <ProvidersTab />
        </TabsContent>

        <TabsContent value="data-sources">
          <DataSourcesTab />
        </TabsContent>

        <TabsContent value="paths">
          <PathsTab />
        </TabsContent>

        <TabsContent value="scheduler">
          <Section
            title="Scheduler"
            description="Cron expression, self-heal, and what the installed scheduler is doing."
          >
            <SchedulerForm />
          </Section>
        </TabsContent>
      </Tabs>
    </div>
  );
}
