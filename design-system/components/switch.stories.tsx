import type { Meta, StoryObj } from "@storybook/react";
import { useState } from "react";

import { Label } from "./label";
import { Switch } from "./switch";

const SOURCES = ["local-git", "github", "claude-code", "remember-plugin"] as const;

function ControlledSwitch() {
  const [enabled, setEnabled] = useState(false);

  return (
    <div className="flex items-center gap-3">
      <Switch id="controlled-switch" checked={enabled} onCheckedChange={setEnabled} />
      <Label htmlFor="controlled-switch">{enabled ? "Scheduler on" : "Scheduler off"}</Label>
    </div>
  );
}

function SwitchList() {
  const [enabled, setEnabled] = useState<string[]>(["local-git"]);

  return (
    <div className="flex w-80 flex-col gap-4">
      {SOURCES.map((source) => (
        <div key={source} className="flex items-center justify-between gap-4">
          <Label htmlFor={`toggle-${source}`} className="font-mono">
            {source}
          </Label>
          <Switch
            id={`toggle-${source}`}
            checked={enabled.includes(source)}
            disabled={source === "local-git"}
            onCheckedChange={(next) =>
              setEnabled((prev) =>
                next ? [...prev, source] : prev.filter((item) => item !== source),
              )
            }
          />
        </div>
      ))}
      <p className="text-xs text-muted-foreground">
        <span className="font-mono">local-git</span> is authoritative and cannot be turned off.
      </p>
    </div>
  );
}

const meta = {
  title: "Components/Switch",
  component: Switch,
  parameters: {
    layout: "centered",
    docs: {
      description: {
        component: "Binary toggle for settings that apply immediately (no Save button).",
      },
    },
  },
  tags: ["autodocs"],
} satisfies Meta<typeof Switch>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = { args: { "aria-label": "Default switch" } };

export const Checked: Story = { args: { defaultChecked: true, "aria-label": "Checked switch" } };

export const Disabled: Story = { args: { disabled: true, "aria-label": "Disabled switch" } };

export const DisabledChecked: Story = {
  args: { disabled: true, checked: true, "aria-label": "Disabled checked switch" },
};

export const WithLabel: Story = {
  render: () => (
    <div className="flex items-center gap-3">
      <Switch id="with-label-switch" defaultChecked />
      <Label htmlFor="with-label-switch">Enable scheduler</Label>
    </div>
  ),
};

export const WithDescription: Story = {
  render: () => (
    <div className="flex w-80 items-start justify-between gap-4">
      <div className="flex flex-col gap-1">
        <Label htmlFor="self-heal">Self-heal</Label>
        <p className="text-xs text-muted-foreground">
          Backfill business days missed while the machine was asleep.
        </p>
      </div>
      <Switch id="self-heal" defaultChecked className="mt-1" />
    </div>
  ),
};

export const Controlled: Story = { render: () => <ControlledSwitch /> };

export const SettingsList: Story = { render: () => <SwitchList /> };

export const States: Story = {
  render: () => (
    <div className="flex items-center gap-6">
      <Switch aria-label="Off" />
      <Switch defaultChecked aria-label="On" />
      <Switch disabled aria-label="Disabled off" />
      <Switch disabled checked aria-label="Disabled on" />
    </div>
  ),
};
