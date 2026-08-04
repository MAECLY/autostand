import type { Meta, StoryObj } from "@storybook/react";

import { Input } from "./input";
import { Label } from "./label";

const meta = {
  title: "Components/Input",
  component: Input,
  parameters: {
    layout: "centered",
    docs: { description: { component: "Single-line text field used across settings and Quick Add." } },
  },
  tags: ["autodocs"],
  argTypes: {
    type: {
      control: "select",
      options: ["text", "password", "email", "number", "search", "file"],
    },
  },
  args: { placeholder: "Type here…", className: "w-72" },
} satisfies Meta<typeof Input>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const WithValue: Story = { args: { defaultValue: "~/Sync/Github_Dailies" } };

export const Disabled: Story = { args: { disabled: true, defaultValue: "macbook-pro-m3" } };

export const ReadOnly: Story = { args: { readOnly: true, defaultValue: "macbook-pro-m3" } };

export const Password: Story = { args: { type: "password", defaultValue: "sk-not-a-real-key" } };

export const Number: Story = { args: { type: "number", defaultValue: 30, placeholder: "Timeout (s)" } };

export const Search: Story = { args: { type: "search", placeholder: "Search history…" } };

export const File: Story = { args: { type: "file", placeholder: undefined } };

export const Invalid: Story = {
  args: {
    "aria-invalid": true,
    defaultValue: "not-a-cron",
    className: "w-72 border-destructive focus-visible:ring-destructive",
  },
};

export const WithLabel: Story = {
  render: () => (
    <div className="flex w-72 flex-col gap-2">
      <Label htmlFor="dailies-path">Dailies path</Label>
      <Input id="dailies-path" defaultValue="~/Sync/Github_Dailies" />
    </div>
  ),
};

export const WithHelperText: Story = {
  render: () => (
    <div className="flex w-72 flex-col gap-2">
      <Label htmlFor="host-slug">Host slug override</Label>
      <Input id="host-slug" placeholder="macbook-pro-m3" aria-describedby="host-slug-help" />
      <p id="host-slug-help" className="text-xs text-muted-foreground">
        Leave empty to keep the persisted slug. Changing it splits history.
      </p>
    </div>
  ),
};

export const States: Story = {
  render: () => (
    <div className="flex w-72 flex-col gap-3">
      <Input placeholder="Placeholder" />
      <Input defaultValue="Filled" />
      <Input defaultValue="Read only" readOnly />
      <Input defaultValue="Disabled" disabled />
      <Input
        defaultValue="Invalid"
        aria-invalid
        className="border-destructive focus-visible:ring-destructive"
      />
    </div>
  ),
};
