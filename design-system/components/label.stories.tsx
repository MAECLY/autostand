import type { Meta, StoryObj } from "@storybook/react";

import { Checkbox } from "./checkbox";
import { Input } from "./input";
import { Label } from "./label";
import { Switch } from "./switch";

const meta = {
  title: "Components/Label",
  component: Label,
  parameters: {
    layout: "centered",
    docs: {
      description: {
        component:
          "Form label built on the Radix Label primitive. It dims automatically when it follows a disabled `peer` control.",
      },
    },
  },
  tags: ["autodocs"],
  args: { children: "Dailies path" },
} satisfies Meta<typeof Label>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const WithInput: Story = {
  render: () => (
    <div className="flex w-72 flex-col gap-2">
      <Label htmlFor="label-input">Dailies path</Label>
      <Input id="label-input" defaultValue="~/Sync/Github_Dailies" />
    </div>
  ),
};

export const WithCheckbox: Story = {
  render: () => (
    <div className="flex items-center gap-2">
      <Checkbox id="label-checkbox" defaultChecked />
      <Label htmlFor="label-checkbox">Include GitHub reviews</Label>
    </div>
  ),
};

export const WithSwitch: Story = {
  render: () => (
    <div className="flex items-center gap-3">
      <Switch id="label-switch" defaultChecked />
      <Label htmlFor="label-switch">Enable scheduler</Label>
    </div>
  ),
};

export const Required: Story = {
  render: () => (
    <div className="flex w-72 flex-col gap-2">
      <Label htmlFor="label-required">
        API key
        <span className="ml-0.5 text-destructive" aria-hidden>
          *
        </span>
      </Label>
      <Input id="label-required" type="password" required />
    </div>
  ),
};

export const DisabledPeer: Story = {
  parameters: {
    docs: {
      description: {
        story:
          "The control must carry the `peer` class (all base inputs do) for the label to dim when disabled.",
      },
    },
  },
  render: () => (
    <div className="flex items-center gap-2">
      <Checkbox id="label-disabled" disabled />
      <Label htmlFor="label-disabled">Unavailable source</Label>
    </div>
  ),
};
