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
import { useQueryClient } from "@tanstack/react-query";
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
import { Label } from "@autostand/ui/components/label";
import { Separator } from "@autostand/ui/components/separator";
import { Switch } from "@autostand/ui/components/switch";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@autostand/ui/components/tabs";

import { SystemAccessSummary } from "@/components/access/SystemAccessDialog";
import { AdvancedTab } from "@/components/settings/AdvancedTab";
import { CommitScanCard } from "@/components/settings/CommitScanCard";
import { DataSourceToggle } from "@/components/settings/DataSourceToggle";
import { FilingDateCard } from "@/components/settings/FilingDateCard";
import { FormatTab } from "@/components/settings/FormatTab";
import { LocalModelsTab } from "@/components/settings/LocalModelsTab";
import { NotificationsTab } from "@/components/settings/NotificationsTab";
import { PathInput } from "@/components/settings/PathInput";
import { ProviderCard } from "@/components/settings/ProviderCard";
import { ProviderUsage } from "@/components/settings/ProviderUsage";
import { RepoTable } from "@/components/settings/RepoTable";
import { SchedulerForm } from "@/components/settings/SchedulerForm";
import { StandupReadinessAlert } from "@/components/settings/StandupReadinessAlert";
import { SyncTab } from "@/components/settings/SyncTab";
import { useConfig, useSetConfig } from "@/hooks/use-config";
import { useDataSources } from "@/hooks/use-data-sources";
import { standupReadinessKey } from "@/hooks/use-readiness";
import {
  useLlmProviders,
  useStoreApiKey,
  useTestProvider,
} from "@/hooks/use-providers";
import { toAppError } from "@/lib/error";
import {
  SETTINGS_TABS,
  isSettingsTab,
  useUiStore,
  type SettingsTab,
} from "@/lib/store";
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
  const [expandedProvider, setExpandedProvider] = useState<string | null>(null);
  const { data: config } = useConfig();
  const providers = useLlmProviders();
  const setConfig = useSetConfig();
  const testProvider = useTestProvider();
  const storeApiKey = useStoreApiKey();

  function patchProvider(id: string, changes: Partial<ProviderConfig>) {
    if (config === undefined) return;
    setConfig.mutate(upsertProvider(config, id, changes));
  }

  function setProviderOrder(order: string[]) {
    if (config === undefined) return;
    setConfig.mutate({
      ...config,
      llm: {
        ...config.llm,
        provider_order: order,
        preferred_provider: order[0] ?? config.llm.preferred_provider,
      },
    });
  }

  if (providers.isPending) return <TabSkeleton rows={3} />;
  if (providers.isError) {
    return (
      <LoadError title="Could not list providers" error={providers.error} />
    );
  }

  const providerById = new Map(
    providers.data.map((provider) => [provider.id, provider]),
  );
  const configuredOrder = config?.llm.provider_order ?? [];
  const orderedIds = [
    ...configuredOrder.filter((id) => providerById.has(id)),
    ...providers.data
      .map((provider) => provider.id)
      .filter((id) => !configuredOrder.includes(id)),
  ];

  return (
    <div className="grid items-start gap-4 xl:grid-cols-[minmax(0,1fr)_20rem]">
      <div className="flex min-w-0 flex-col gap-3">
        <div className="flex items-start justify-between gap-5 rounded-xl border border-border bg-surface p-4">
          <div>
            <p className="text-sm font-semibold">Provider priority</p>
            <p className="max-w-2xl text-sm text-muted-foreground">
              Autostand starts at the top and continues with the next enabled provider when usage, authentication or service availability blocks a render.
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-3">
            <Label htmlFor="provider-fallback-enabled" className="text-xs font-normal">
              Automatic fallback
            </Label>
            <Switch
              id="provider-fallback-enabled"
              checked={config?.llm.fallback_enabled ?? true}
              disabled={config === undefined}
              onCheckedChange={(fallback_enabled) => {
                if (config === undefined) return;
                setConfig.mutate({ ...config, llm: { ...config.llm, fallback_enabled } });
              }}
            />
          </div>
        </div>

        <div className="flex flex-col gap-2" role="radiogroup" aria-label="Preferred AI provider">
        {orderedIds.map((providerId, index) => {
        const provider = providerById.get(providerId);
        if (provider === undefined) return null;
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
            canMoveUp={index > 0}
            canMoveDown={index < orderedIds.length - 1}
            onSetPreferred={() => {
              if (config === undefined) return;
              const order = [
                provider.id,
                ...orderedIds.filter((id) => id !== provider.id),
              ];
              setConfig.mutate({
                ...config,
                llm: {
                  ...config.llm,
                  preferred_provider: provider.id,
                  provider_order: order,
                },
              });
            }}
            onSetEnabled={(enabled) => patchProvider(provider.id, { enabled })}
            onMoveUp={() => {
              if (index === 0) return;
              const order = [...orderedIds];
              [order[index - 1], order[index]] = [
                order[index],
                order[index - 1],
              ];
              setProviderOrder(order);
            }}
            onMoveDown={() => {
              if (index >= orderedIds.length - 1) return;
              const order = [...orderedIds];
              [order[index], order[index + 1]] = [
                order[index + 1],
                order[index],
              ];
              setProviderOrder(order);
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
            expanded={expandedProvider === provider.id}
            onToggleDetails={() =>
              setExpandedProvider((current) => current === provider.id ? null : provider.id)
            }
          />
        );
        })}
        </div>
      </div>

      <aside className="sticky top-4 min-w-0 rounded-xl border border-border bg-surface p-4">
        <ProviderUsage compact />
      </aside>
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
  const queryClient = useQueryClient();
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
      {
        onSuccess: () => {
          // Drop the draft so the inputs follow the reloaded config again.
          setDraft(null);
          // `github_dir` is one of the three inputs the readiness probe reads.
          void queryClient.invalidateQueries({ queryKey: standupReadinessKey });
        },
      },
    );
  }

  return (
    <div className="flex flex-col gap-4">
      {/* First on the tab: the filename question comes before the directory
          question, because it is the one that decides whether the user can find
          the standup they just compiled. */}
      <FilingDateCard />

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

      <CommitScanCard />

      {/* Beside the paths, because the question it answers — "can autostand
          actually read this?" — is about these two directories. */}
      <Section
        title="Folder access"
        description="What the operating system currently lets autostand read. Everything here is read-only."
      >
        <SystemAccessSummary />
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

/**
 * Labels for the tab ids owned by the store. A `Record` keyed by
 * {@link SettingsTab} makes a forgotten label a type error, so the strip and
 * the remembered tab cannot drift apart.
 */
const TAB_LABELS: Record<SettingsTab, string> = {
  providers: "Providers",
  "data-sources": "Data Sources",
  format: "Standup Format",
  paths: "Paths",
  sync: "Sync",
  scheduler: "Scheduler",
  notifications: "Notifications",
  "local-models": "Local AI",
  advanced: "Advanced",
};

function SettingsPage() {
  // The route unmounts on every navigation away, so the open tab lives in the
  // UI store instead of Radix's internal state.
  const settingsTab = useUiStore((state) => state.settingsTab);
  const setSettingsTab = useUiStore((state) => state.setSettingsTab);

  return (
    <div className="flex min-h-0 flex-col gap-6 p-6">
      <header className="min-w-0">
        <h2 className="text-lg font-semibold text-foreground">Settings</h2>
        <p className="text-sm text-muted-foreground">
          Providers, data sources, paths, cloud sync and the compile schedule.
        </p>
      </header>

      {/* Above the tab strip on purpose: a cold-start install is missing the
          inputs Local Git needs, and that has to be visible from whichever tab
          the store reopened, not only from Paths. */}
      <StandupReadinessAlert onFix={() => setSettingsTab("paths")} />

      <Tabs
        className="min-w-0"
        value={settingsTab}
        onValueChange={(value) => {
          if (isSettingsTab(value)) setSettingsTab(value);
        }}
      >
        {/* Nine triggers need ~854px laid out in a row. Without a scroller they
            set the floor for the whole pane, and the window's overflow-hidden
            then clips whatever does not fit — at 640px that is the right half of
            every tab's content, unreachable rather than merely tight. */}
        <div className="min-w-0 overflow-x-auto">
          <TabsList>
            {SETTINGS_TABS.map((tab) => (
              <TabsTrigger key={tab} value={tab}>
                {TAB_LABELS[tab]}
              </TabsTrigger>
            ))}
          </TabsList>
        </div>

        <TabsContent value="providers">
          <ProvidersTab />
        </TabsContent>

        <TabsContent value="data-sources">
          <DataSourcesTab />
        </TabsContent>

        <TabsContent value="format">
          <FormatTab />
        </TabsContent>

        <TabsContent value="paths">
          <PathsTab />
        </TabsContent>

        <TabsContent value="sync">
          <SyncTab />
        </TabsContent>

        <TabsContent value="scheduler">
          <Section
            title="Scheduler"
            description="Choose when Autostand runs in plain language. Advanced cron remains available when needed."
          >
            <SchedulerForm />
          </Section>
        </TabsContent>

        <TabsContent value="notifications">
          <Section
            title="Notifications"
            description="Choose which provider, usage and scheduler events may reach the operating system."
          >
            <NotificationsTab />
          </Section>
        </TabsContent>

        <TabsContent value="local-models">
          <Section
            title="Built-in local AI"
            description="Download and select a private GGUF model for offline provider fallback."
          >
            <LocalModelsTab />
          </Section>
        </TabsContent>

        <TabsContent value="advanced">
          <AdvancedTab />
        </TabsContent>
      </Tabs>
    </div>
  );
}
