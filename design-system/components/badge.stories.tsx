import type { Meta, StoryObj } from "@storybook/react";
import { CheckCircle2, GitCommit, Ghost as GhostIcon } from "lucide-react";

import { Badge } from "./badge";

const meta = {
  title: "Components/Badge",
  component: Badge,
  parameters: {
    layout: "centered",
    docs: {
      description: {
        component:
          "Compact status tag. Used by audit badges, provider status pills and history filters.",
      },
    },
  },
  tags: ["autodocs"],
  argTypes: {
    variant: {
      control: "select",
      options: ["default", "secondary", "success", "warning", "error", "outline"],
    },
  },
  args: { children: "Badge" },
} satisfies Meta<typeof Badge>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = { args: { variant: "default" } };

export const Secondary: Story = { args: { variant: "secondary", children: "Neutral" } };

export const Success: Story = { args: { variant: "success", children: "Done" } };

export const Warning: Story = { args: { variant: "warning", children: "Stale" } };

export const Error: Story = { args: { variant: "error", children: "Phantom" } };

export const Outline: Story = { args: { variant: "outline", children: "Subtle" } };

export const WithIcon: Story = {
  args: {
    variant: "success",
    children: (
      <>
        <CheckCircle2 aria-hidden />
        Verified
      </>
    ),
  },
};

export const Variants: Story = {
  render: () => (
    <div className="flex flex-wrap items-center gap-2">
      <Badge variant="default">Default</Badge>
      <Badge variant="secondary">Secondary</Badge>
      <Badge variant="success">Success</Badge>
      <Badge variant="warning">Warning</Badge>
      <Badge variant="error">Error</Badge>
      <Badge variant="outline">Outline</Badge>
    </div>
  ),
};

export const AuditClassifications: Story = {
  parameters: {
    docs: {
      description: {
        story: "How the audit sidecar classifications read when rendered as badges.",
      },
    },
  },
  render: () => (
    <div className="flex flex-wrap items-center gap-2">
      <Badge variant="success">
        <GitCommit aria-hidden />
        commit
      </Badge>
      <Badge variant="secondary">github</Badge>
      <Badge variant="secondary">review</Badge>
      <Badge variant="outline">note</Badge>
      <Badge variant="error">
        <GhostIcon aria-hidden />
        phantom
      </Badge>
      <Badge variant="warning">unverified</Badge>
    </div>
  ),
};
