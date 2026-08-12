/**
 * Standup format configuration — presets, verbosity, toggles.
 *
 * Presets mold the LLM prompt's `## OUTPUT` section. When `render_mode = Det`
 * the preset is ignored (the deterministic renderer has a fixed format).
 */

import { useState } from "react";

import { Badge } from "@autostand/ui/components/badge";
import { Button } from "@autostand/ui/components/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@autostand/ui/components/card";
import { Label } from "@autostand/ui/components/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@autostand/ui/components/select";
import { Separator } from "@autostand/ui/components/separator";
import { Switch } from "@autostand/ui/components/switch";

import { useConfig, useSetConfig } from "@/hooks/use-config";
import type {
  AppConfig,
  StandupFormatConfig,
  StandupPreset,
  Verbosity,
} from "@/lib/types";

const PRESETS: Array<{
  id: StandupPreset;
  label: string;
  description: string;
}> = [
  {
    id: "classic-scrum",
    label: "Classic Scrum 3Q",
    description: "Yesterday / Today / Blockers — Scrum Guide 2020 default.",
  },
  {
    id: "four-question",
    label: "Scrum 4Q",
    description: "Adds a help-needed section.",
  },
  {
    id: "mad-sad-glad",
    label: "Mad / Sad / Glad",
    description: "Emotional check-in (Atlassian Team Playbook).",
  },
  {
    id: "start-stop-continue",
    label: "Start / Stop / Continue",
    description: "Process-feedback loop.",
  },
  {
    id: "keep-drop-create",
    label: "Keep / Drop / Create",
    description: "Creative retro framing.",
  },
  {
    id: "five-question",
    label: "5Q + Confidence",
    description: "Adds confidence + team health ratings.",
  },
  {
    id: "spotify-4q",
    label: "Spotify 4Q",
    description: "Did / Doing / Blocking / Need (Kniberg).",
  },
  {
    id: "async-status",
    label: "Async Status",
    description: "Done / Doing / Blockers / FYI — distributed teams.",
  },
  {
    id: "walking-timebox",
    label: "Walking / 15-min",
    description: "Terse, timeboxed classic Scrum.",
  },
  {
    id: "walk-the-board",
    label: "Walk the Board",
    description: "Kanban / board-driven (right-to-left).",
  },
  {
    id: "ytbr",
    label: "Y / T / B / R",
    description: "Yesterday / Today / Blockers / Risks — SRE.",
  },
  {
    id: "decisions-commitments",
    label: "Decisions & Commitments",
    description: "Fulfilled / Committing / Decisions / Blockers.",
  },
  {
    id: "okr-tied",
    label: "OKR-tied",
    description: "KR / delta / Today / Blockers / Confidence.",
  },
];

const VERBOSITIES: Array<{ id: Verbosity; label: string }> = [
  { id: "terse", label: "Terse" },
  { id: "standard", label: "Standard" },
  { id: "detailed", label: "Detailed" },
];

const DEFAULT_FORMAT: StandupFormatConfig = {
  preset: "classic-scrum",
  verbosity: "standard",
  include_pr_review: true,
  include_confidence: false,
  include_risks: false,
  conventional: false,
};

function patchFormat(
  config: AppConfig,
  changes: Partial<StandupFormatConfig>,
): AppConfig {
  const current = config.format ?? DEFAULT_FORMAT;
  return { ...config, format: { ...current, ...changes } };
}

export function FormatTab() {
  const config = useConfig();
  const setConfig = useSetConfig();
  const [savedPreset, setSavedPreset] = useState<StandupPreset | null>(null);

  if (config.isPending) return <div className="h-20 animate-pulse rounded-lg bg-muted" />;
  if (config.isError) {
    return (
      <Card>
        <CardContent className="pt-6 text-sm text-muted-foreground">
          Could not load settings.
        </CardContent>
      </Card>
    );
  }

  const current = config.data;
  const format = current.format ?? DEFAULT_FORMAT;
  const isDet = current.render_mode === "Det";

  function apply(changes: Partial<StandupFormatConfig>) {
    setConfig.mutate(patchFormat(current, changes));
  }

  const previewPreset = savedPreset ?? format.preset;
  const previewEntry = PRESETS.find((p) => p.id === previewPreset);

  return (
    <div className="flex flex-col gap-4">
      {isDet && (
        <Card className="border-warning-border bg-warning-bg">
          <CardContent className="pt-6 text-sm text-warning-fg">
            Render mode is <strong>Det</strong> — presets only affect the LLM
            render path and are ignored by the deterministic renderer.
          </CardContent>
        </Card>
      )}

      <Card>
        <CardHeader>
          <CardTitle>Standup format</CardTitle>
          <CardDescription>
            Presets mold the LLM prompt's output structure. The deterministic
            renderer always uses a fixed format.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-6">
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {PRESETS.map((preset) => {
              const active = format.preset === preset.id;
              return (
                <button
                  key={preset.id}
                  type="button"
                  onClick={() => {
                    setSavedPreset(preset.id);
                    apply({ preset: preset.id });
                  }}
                  className={`flex flex-col items-start gap-1 rounded-lg border p-3 text-left transition-colors ${
                    active
                      ? "border-primary border-2 bg-primary/5"
                      : "border-border hover:border-primary/50"
                  }`}
                >
                  <div className="flex w-full items-center justify-between">
                    <span className="text-sm font-medium text-foreground">
                      {preset.label}
                    </span>
                    {active && <Badge variant="default">Active</Badge>}
                  </div>
                  <span className="text-xs text-muted-foreground">
                    {preset.description}
                  </span>
                </button>
              );
            })}
          </div>

          <Separator />

          <div className="grid gap-4 sm:grid-cols-2">
            <div className="flex flex-col gap-2">
              <Label htmlFor="verbosity">Verbosity</Label>
              <Select
                value={format.verbosity}
                onValueChange={(v) => apply({ verbosity: v as Verbosity })}
              >
                <SelectTrigger id="verbosity">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {VERBOSITIES.map((v) => (
                    <SelectItem key={v.id} value={v.id}>
                      {v.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>

          <Separator />

          <div className="flex flex-col gap-4">
            <div className="flex items-center justify-between">
              <div className="flex flex-col gap-1">
                <Label htmlFor="include-pr-review">Include PR Review section</Label>
                <span className="text-xs text-muted-foreground">
                  Trailing `**PR Review**` block with one bullet per PR reviewed.
                </span>
              </div>
              <Switch
                id="include-pr-review"
                checked={format.include_pr_review}
                onCheckedChange={(v) => apply({ include_pr_review: v })}
              />
            </div>

            <div className="flex items-center justify-between">
              <div className="flex flex-col gap-1">
                <Label htmlFor="include-confidence">Include confidence rating</Label>
                <span className="text-xs text-muted-foreground">
                  Adds a 1–5 confidence / team-health line.
                </span>
              </div>
              <Switch
                id="include-confidence"
                checked={format.include_confidence}
                onCheckedChange={(v) => apply({ include_confidence: v })}
              />
            </div>

            <div className="flex items-center justify-between">
              <div className="flex flex-col gap-1">
                <Label htmlFor="include-risks">Include risks section</Label>
                <span className="text-xs text-muted-foreground">
                  Forward-looking risks, separate from active blockers.
                </span>
              </div>
              <Switch
                id="include-risks"
                checked={format.include_risks}
                onCheckedChange={(v) => apply({ include_risks: v })}
              />
            </div>

            <div className="flex items-center justify-between">
              <div className="flex flex-col gap-1">
                <Label htmlFor="conventional">Conventional-commit scopes</Label>
                <span className="text-xs text-muted-foreground">
                  Ask the LLM for `feat:` / `fix:` / `refactor:` prefixed bullets.
                </span>
              </div>
              <Switch
                id="conventional"
                checked={format.conventional}
                onCheckedChange={(v) => apply({ conventional: v })}
              />
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Preview</CardTitle>
          <CardDescription>
            What the selected preset asks the LLM to produce.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="rounded-md border border-border bg-inset p-4">
            <div className="mb-2 flex items-center gap-2">
              <Badge variant="secondary">{previewEntry?.label ?? "—"}</Badge>
              <Badge variant="outline">{format.verbosity}</Badge>
            </div>
            <p className="text-sm text-muted-foreground">
              {previewEntry?.description ?? "Select a preset to see its description."}
            </p>
          </div>
          <div className="mt-4 flex justify-end">
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                setSavedPreset(format.preset);
                setConfig.mutate({ ...current, format: { ...DEFAULT_FORMAT } });
              }}
            >
              Reset to defaults
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}