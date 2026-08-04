import type { Meta, StoryObj } from "@storybook/react";
import { useState } from "react";
import { ChevronDown } from "lucide-react";

import { Button } from "./button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "./collapsible";

const BULLETS = [
  "Implemented the LLM adapter trait (autostand-adapters)",
  "Added the anti-backdating guard to the file writer",
  "Reviewed PR #128 — scheduler self-heal",
];

function ControlledCollapsible() {
  const [open, setOpen] = useState(false);

  return (
    <div className="flex w-96 flex-col gap-3">
      <Collapsible open={open} onOpenChange={setOpen} className="rounded-lg border border-border">
        <CollapsibleTrigger asChild>
          <button
            type="button"
            className="flex w-full items-center justify-between gap-2 rounded-lg px-4 py-3 text-left text-sm font-medium transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            AUTO block
            <ChevronDown
              aria-hidden
              className={`size-4 text-muted-foreground transition-transform duration-150 ${
                open ? "rotate-180" : ""
              }`}
            />
          </button>
        </CollapsibleTrigger>
        <CollapsibleContent className="border-t border-border px-4 py-3 text-sm text-muted-foreground">
          <ul className="flex flex-col gap-1">
            {BULLETS.map((bullet) => (
              <li key={bullet}>{bullet}</li>
            ))}
          </ul>
        </CollapsibleContent>
      </Collapsible>
      <p className="text-xs text-muted-foreground">
        Open: <span className="font-mono">{String(open)}</span>
      </p>
    </div>
  );
}

const meta = {
  title: "Components/Collapsible",
  component: Collapsible,
  parameters: {
    layout: "padded",
    docs: {
      description: {
        component:
          "Show/hide a region behind any trigger. The component ships unstyled apart from `overflow: hidden` on the content, so the trigger chrome is up to the caller.",
      },
    },
  },
  tags: ["autodocs"],
} satisfies Meta<typeof Collapsible>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <Collapsible className="w-96">
      <CollapsibleTrigger asChild>
        <Button variant="outline" className="w-full justify-between">
          Show audit details
          <ChevronDown aria-hidden />
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent className="pt-2 text-sm text-muted-foreground">
        <ul className="flex flex-col gap-1 rounded-md border border-border bg-surface p-3">
          {BULLETS.map((bullet) => (
            <li key={bullet}>{bullet}</li>
          ))}
        </ul>
      </CollapsibleContent>
    </Collapsible>
  ),
};

export const DefaultOpen: Story = {
  render: () => (
    <Collapsible defaultOpen className="w-96">
      <CollapsibleTrigger asChild>
        <Button variant="outline" className="w-full justify-between">
          Hide audit details
          <ChevronDown aria-hidden />
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent className="pt-2 text-sm text-muted-foreground">
        <ul className="flex flex-col gap-1 rounded-md border border-border bg-surface p-3">
          {BULLETS.map((bullet) => (
            <li key={bullet}>{bullet}</li>
          ))}
        </ul>
      </CollapsibleContent>
    </Collapsible>
  ),
};

export const Disabled: Story = {
  render: () => (
    <Collapsible disabled className="w-96">
      <CollapsibleTrigger asChild>
        <Button variant="outline" className="w-full justify-between" disabled>
          No AUTO block for this date
          <ChevronDown aria-hidden />
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent className="pt-2 text-sm text-muted-foreground">
        Unreachable while disabled.
      </CollapsibleContent>
    </Collapsible>
  ),
};

export const WithPeek: Story = {
  parameters: {
    docs: { description: { story: "Keep the first row visible and collapse only the remainder." } },
  },
  render: () => (
    <Collapsible className="flex w-96 flex-col gap-2 rounded-lg border border-border bg-surface p-4">
      <div className="flex items-center justify-between gap-2">
        <p className="text-sm font-medium">3 bullets</p>
        <CollapsibleTrigger asChild>
          <Button variant="ghost" size="sm">
            Toggle
          </Button>
        </CollapsibleTrigger>
      </div>
      <p className="text-sm text-muted-foreground">{BULLETS[0]}</p>
      <CollapsibleContent>
        <ul className="flex flex-col gap-2 pt-2 text-sm text-muted-foreground">
          {BULLETS.slice(1).map((bullet) => (
            <li key={bullet}>{bullet}</li>
          ))}
        </ul>
      </CollapsibleContent>
    </Collapsible>
  ),
};

export const Controlled: Story = { render: () => <ControlledCollapsible /> };
