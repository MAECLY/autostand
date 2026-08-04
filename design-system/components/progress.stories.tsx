import type { Meta, StoryObj } from "@storybook/react";

import { Progress } from "./progress";

const meta = {
  title: "Components/Progress",
  component: Progress,
  parameters: {
    layout: "centered",
    docs: {
      description: {
        component:
          "Determinate progress bar. `value` is clamped to `[0, max]` so a stale or overshooting value can never push the indicator outside the track.",
      },
    },
  },
  tags: ["autodocs"],
  argTypes: {
    value: { control: { type: "range", min: 0, max: 100, step: 1 } },
    max: { control: { type: "number", min: 1 } },
  },
  args: { value: 60, max: 100, className: "w-80" },
} satisfies Meta<typeof Progress>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Empty: Story = { args: { value: 0 } };

export const Complete: Story = { args: { value: 100 } };

export const Indeterminate: Story = {
  parameters: {
    docs: {
      description: {
        story: "`value={null}` puts Radix in its indeterminate state; the bar renders empty.",
      },
    },
  },
  args: { value: null },
};

export const Clamped: Story = {
  parameters: {
    docs: { description: { story: "A value above `max` is clamped instead of overflowing." } },
  },
  args: { value: 180 },
};

export const CustomMax: Story = { args: { value: 3, max: 8 } };

export const CustomIndicator: Story = {
  args: { value: 45, indicatorClassName: "bg-success" },
};

export const Thick: Story = { args: { value: 70, className: "h-4 w-80" } };

export const Steps: Story = {
  render: () => (
    <div className="flex w-80 flex-col gap-4">
      {[
        ["Gather", 100],
        ["Redact", 100],
        ["Render", 55],
        ["Write", 0],
      ].map(([label, value]) => (
        <div key={String(label)} className="flex flex-col gap-1.5">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{label}</span>
            <span className="font-mono">{value}%</span>
          </div>
          <Progress value={Number(value)} aria-label={`${label} progress`} />
        </div>
      ))}
    </div>
  ),
};
