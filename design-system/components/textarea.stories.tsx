import type { Meta, StoryObj } from "@storybook/react";

import { Label } from "./label";
import { Textarea } from "./textarea";

const SAMPLE = `- Wired the LLM adapter trait to the pipeline
- Added anti-backdating guard to the file writer
- Reviewed PR #128 (scheduler self-heal)`;

const meta = {
  title: "Components/Textarea",
  component: Textarea,
  parameters: {
    layout: "centered",
    docs: {
      description: {
        component: "Multi-line text field. Used for the MANUAL block editor and Quick Add notes.",
      },
    },
  },
  tags: ["autodocs"],
  args: { placeholder: "What did you work on?", className: "w-96" },
} satisfies Meta<typeof Textarea>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const WithValue: Story = { args: { defaultValue: SAMPLE, rows: 5 } };

export const Disabled: Story = { args: { defaultValue: SAMPLE, disabled: true, rows: 5 } };

export const ReadOnly: Story = { args: { defaultValue: SAMPLE, readOnly: true, rows: 5 } };

export const Tall: Story = { args: { rows: 12, defaultValue: SAMPLE } };

export const Monospace: Story = {
  args: { defaultValue: SAMPLE, rows: 6, className: "w-96 font-mono" },
};

export const WithLabel: Story = {
  render: () => (
    <div className="flex w-96 flex-col gap-2">
      <Label htmlFor="manual-block">MANUAL block</Label>
      <Textarea id="manual-block" defaultValue={SAMPLE} rows={6} />
      <p className="text-xs text-muted-foreground">
        Never overwritten by a compile — the AUTO block is merged around it.
      </p>
    </div>
  ),
};

export const States: Story = {
  render: () => (
    <div className="flex w-96 flex-col gap-3">
      <Textarea placeholder="Placeholder" />
      <Textarea defaultValue="Filled" />
      <Textarea defaultValue="Read only" readOnly />
      <Textarea defaultValue="Disabled" disabled />
      <Textarea
        defaultValue="Invalid"
        aria-invalid
        className="border-destructive focus-visible:ring-destructive"
      />
    </div>
  ),
};
