import type { Meta, StoryObj } from "@storybook/react";

import { ScrollArea } from "./scroll-area";
import { Separator } from "./separator";

const DATES = Array.from({ length: 40 }, (_, index) => {
  const day = String(31 - (index % 31)).padStart(2, "0");
  return `2026-07-${day}`;
});

const PARAGRAPH =
  "The deterministic renderer is always computed, even when a provider succeeds, so a failed or slow LLM call can never block the write. The audit sidecar records which bullets came from which source.";

const meta = {
  title: "Components/ScrollArea",
  component: ScrollArea,
  parameters: {
    layout: "padded",
    docs: {
      description: {
        component:
          "Radix scroll container with a themed scrollbar. It needs an explicit height (or a constrained parent) to scroll — the component only sets `overflow: hidden`.",
      },
    },
  },
  tags: ["autodocs"],
} satisfies Meta<typeof ScrollArea>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <ScrollArea className="h-64 w-80 rounded-md border border-border bg-surface p-4">
      <p className="text-sm text-foreground">
        {PARAGRAPH} {PARAGRAPH} {PARAGRAPH}
      </p>
    </ScrollArea>
  ),
};

export const List: Story = {
  render: () => (
    <ScrollArea className="h-64 w-56 rounded-md border border-border bg-surface">
      <div className="p-4">
        <p className="pb-3 text-sm font-medium">History</p>
        {DATES.map((date) => (
          <div key={date}>
            <p className="py-2 font-mono text-sm text-muted-foreground">{date}</p>
            <Separator />
          </div>
        ))}
      </div>
    </ScrollArea>
  ),
};

export const NoOverflow: Story = {
  parameters: {
    docs: { description: { story: "The scrollbar stays hidden when the content fits." } },
  },
  render: () => (
    <ScrollArea className="h-64 w-80 rounded-md border border-border bg-surface p-4">
      <p className="text-sm">Short content — nothing to scroll.</p>
    </ScrollArea>
  ),
};

export const Horizontal: Story = {
  parameters: {
    docs: {
      description: {
        story:
          "Wide content scrolls horizontally inside the viewport; only the vertical scrollbar is rendered.",
      },
    },
  },
  render: () => (
    <ScrollArea className="w-80 rounded-md border border-border bg-surface">
      <div className="flex w-max gap-3 p-4">
        {["Claude", "Ollama", "OpenAI", "Gemini", "Grok"].map((provider) => (
          <div
            key={provider}
            className="flex h-24 w-32 shrink-0 items-center justify-center rounded-md border border-border bg-elevated text-sm"
          >
            {provider}
          </div>
        ))}
      </div>
    </ScrollArea>
  ),
};

export const FullHeight: Story = {
  render: () => (
    <div className="flex h-72 w-80 flex-col overflow-hidden rounded-md border border-border bg-surface">
      <p className="border-b border-border p-3 text-sm font-medium">Audit bullets</p>
      <ScrollArea className="flex-1">
        <div className="p-3">
          {DATES.map((date) => (
            <p key={date} className="py-1.5 font-mono text-xs text-muted-foreground">
              {date} — 3 bullets
            </p>
          ))}
        </div>
      </ScrollArea>
    </div>
  ),
};
