import type { Meta, StoryObj } from "@storybook/react";
import { useState } from "react";
import type { CheckedState } from "@radix-ui/react-checkbox";

import { Checkbox } from "./checkbox";
import { Label } from "./label";

const SOURCES = ["local-git", "github", "claude-code", "opencode"] as const;

function ControlledCheckbox() {
  const [checked, setChecked] = useState<CheckedState>(false);

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-2">
        <Checkbox id="controlled" checked={checked} onCheckedChange={setChecked} />
        <Label htmlFor="controlled">Redact secrets before render</Label>
      </div>
      <p className="text-xs text-muted-foreground">
        State: <span className="font-mono">{String(checked)}</span>
      </p>
    </div>
  );
}

function CheckboxGroup() {
  const [selected, setSelected] = useState<string[]>(["local-git"]);

  return (
    <fieldset className="flex flex-col gap-3">
      <legend className="pb-2 text-sm font-medium">Data sources</legend>
      {SOURCES.map((source) => (
        <div key={source} className="flex items-center gap-2">
          <Checkbox
            id={source}
            checked={selected.includes(source)}
            onCheckedChange={(next) =>
              setSelected((prev) =>
                next === true ? [...prev, source] : prev.filter((item) => item !== source),
              )
            }
          />
          <Label htmlFor={source} className="font-mono">
            {source}
          </Label>
        </div>
      ))}
    </fieldset>
  );
}

const meta = {
  title: "Components/Checkbox",
  component: Checkbox,
  parameters: {
    layout: "centered",
    docs: {
      description: {
        component:
          "Tri-state checkbox (Radix). `checked` accepts `true`, `false` or `\"indeterminate\"`.",
      },
    },
  },
  tags: ["autodocs"],
} satisfies Meta<typeof Checkbox>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = { args: { "aria-label": "Default checkbox" } };

export const Checked: Story = { args: { defaultChecked: true, "aria-label": "Checked" } };

export const Indeterminate: Story = {
  args: { checked: "indeterminate", "aria-label": "Indeterminate" },
};

export const Disabled: Story = { args: { disabled: true, "aria-label": "Disabled" } };

export const DisabledChecked: Story = {
  args: { disabled: true, checked: true, "aria-label": "Disabled checked" },
};

export const WithLabel: Story = {
  render: () => (
    <div className="flex items-center gap-2">
      <Checkbox id="with-label" defaultChecked />
      <Label htmlFor="with-label">Include GitHub reviews</Label>
    </div>
  ),
};

export const WithDescription: Story = {
  render: () => (
    <div className="flex items-start gap-2">
      <Checkbox id="with-description" defaultChecked className="mt-0.5" />
      <div className="flex flex-col gap-1">
        <Label htmlFor="with-description">Self-heal missed runs</Label>
        <p className="text-xs text-muted-foreground">
          Compiles any business day that was skipped while the machine was asleep.
        </p>
      </div>
    </div>
  ),
};

export const Controlled: Story = { render: () => <ControlledCheckbox /> };

export const Group: Story = { render: () => <CheckboxGroup /> };

export const States: Story = {
  render: () => (
    <div className="flex items-center gap-6">
      <div className="flex flex-col items-center gap-2">
        <Checkbox aria-label="Unchecked" />
        <span className="text-xs text-muted-foreground">Unchecked</span>
      </div>
      <div className="flex flex-col items-center gap-2">
        <Checkbox checked aria-label="Checked" />
        <span className="text-xs text-muted-foreground">Checked</span>
      </div>
      <div className="flex flex-col items-center gap-2">
        <Checkbox checked="indeterminate" aria-label="Indeterminate" />
        <span className="text-xs text-muted-foreground">Indeterminate</span>
      </div>
      <div className="flex flex-col items-center gap-2">
        <Checkbox disabled aria-label="Disabled" />
        <span className="text-xs text-muted-foreground">Disabled</span>
      </div>
      <div className="flex flex-col items-center gap-2">
        <Checkbox disabled checked aria-label="Disabled checked" />
        <span className="text-xs text-muted-foreground">Disabled checked</span>
      </div>
    </div>
  ),
};
