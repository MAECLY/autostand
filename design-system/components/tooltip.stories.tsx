import type { Meta, StoryObj } from "@storybook/react";
import { HelpCircle, RefreshCw, Trash2 } from "lucide-react";

import { Badge } from "./badge";
import { Button } from "./button";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "./tooltip";

const meta = {
  title: "Components/Tooltip",
  component: Tooltip,
  parameters: {
    layout: "centered",
    docs: {
      description: {
        component:
          "Hover/focus hint. Every tooltip must sit inside a `TooltipProvider` (the app mounts one at the root; stories mount their own).",
      },
    },
  },
  tags: ["autodocs"],
} satisfies Meta<typeof Tooltip>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button variant="outline">Hover me</Button>
        </TooltipTrigger>
        <TooltipContent>Compiles today&apos;s standup</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  ),
};

export const OnIconButton: Story = {
  render: () => (
    <TooltipProvider>
      <div className="flex items-center gap-2">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="ghost" size="icon" aria-label="Recompile">
              <RefreshCw aria-hidden />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Recompile</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="ghost" size="icon" aria-label="Discard draft">
              <Trash2 aria-hidden />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Discard draft</TooltipContent>
        </Tooltip>
      </div>
    </TooltipProvider>
  ),
};

export const Sides: Story = {
  render: () => (
    <TooltipProvider>
      <div className="grid grid-cols-2 gap-3">
        {(["top", "right", "bottom", "left"] as const).map((side) => (
          <Tooltip key={side}>
            <TooltipTrigger asChild>
              <Button variant="outline" size="sm" className="capitalize">
                {side}
              </Button>
            </TooltipTrigger>
            <TooltipContent side={side}>Opens on the {side}</TooltipContent>
          </Tooltip>
        ))}
      </div>
    </TooltipProvider>
  ),
};

export const LongContent: Story = {
  render: () => (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <Badge variant="error">phantom</Badge>
        </TooltipTrigger>
        <TooltipContent>
          A bullet with no supporting evidence in any data source. It is kept in the audit sidecar
          but excluded from the rendered standup.
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  ),
};

export const NoDelay: Story = {
  parameters: {
    docs: { description: { story: "`delayDuration` is set on the provider; the default is 200ms." } },
  },
  render: () => (
    <TooltipProvider delayDuration={0}>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button variant="outline">Instant</Button>
        </TooltipTrigger>
        <TooltipContent>No hover delay</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  ),
};

export const DefaultOpen: Story = {
  parameters: {
    docs: { description: { story: "Pinned open so the surface and shadow can be inspected." } },
  },
  render: () => (
    <TooltipProvider>
      <div className="pt-16">
        <Tooltip defaultOpen>
          <TooltipTrigger asChild>
            <Button variant="ghost" size="icon" aria-label="Help">
              <HelpCircle aria-hidden />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="top">Audit classifications explained</TooltipContent>
        </Tooltip>
      </div>
    </TooltipProvider>
  ),
};
