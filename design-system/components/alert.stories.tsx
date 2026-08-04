import type { Meta, StoryObj } from "@storybook/react";
import { AlertCircle, AlertTriangle, CheckCircle2, Info } from "lucide-react";

import { Alert, AlertDescription, AlertTitle } from "./alert";

const meta = {
  title: "Components/Alert",
  component: Alert,
  parameters: {
    layout: "padded",
    docs: {
      description: {
        component:
          "Inline status banner. An optional lucide icon may be passed as the first child; the body text indents around it automatically.",
      },
    },
  },
  tags: ["autodocs"],
  argTypes: {
    variant: { control: "select", options: ["default", "destructive", "success", "warning"] },
  },
} satisfies Meta<typeof Alert>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    variant: "default",
    children: (
      <>
        <Info aria-hidden />
        <AlertTitle>No standup for this date</AlertTitle>
        <AlertDescription>Pick another day or compile a new entry.</AlertDescription>
      </>
    ),
  },
};

export const Destructive: Story = {
  args: {
    variant: "destructive",
    children: (
      <>
        <AlertTriangle aria-hidden />
        <AlertTitle>Compile failed</AlertTitle>
        <AlertDescription>
          The provider returned a non-zero exit code. The deterministic fallback was written
          instead.
        </AlertDescription>
      </>
    ),
  },
};

export const Success: Story = {
  args: {
    variant: "success",
    children: (
      <>
        <CheckCircle2 aria-hidden />
        <AlertTitle>Standup written</AlertTitle>
        <AlertDescription>12 bullets across 3 repositories.</AlertDescription>
      </>
    ),
  },
};

export const Warning: Story = {
  args: {
    variant: "warning",
    children: (
      <>
        <AlertCircle aria-hidden />
        <AlertTitle>Partial gather</AlertTitle>
        <AlertDescription>2 of 8 data sources were unreachable.</AlertDescription>
      </>
    ),
  },
};

export const WithoutIcon: Story = {
  args: {
    children: (
      <>
        <AlertTitle>Scheduler disabled</AlertTitle>
        <AlertDescription>Enable it in Settings to compile automatically.</AlertDescription>
      </>
    ),
  },
};

export const TitleOnly: Story = {
  args: {
    variant: "success",
    children: (
      <>
        <CheckCircle2 aria-hidden />
        <AlertTitle>Saved to keychain</AlertTitle>
      </>
    ),
  },
};

export const DescriptionOnly: Story = {
  args: {
    children: (
      <>
        <Info aria-hidden />
        <AlertDescription>Audit sidecars are kept next to each rendered standup.</AlertDescription>
      </>
    ),
  },
};

export const Variants: Story = {
  render: () => (
    <div className="flex w-full max-w-2xl flex-col gap-3">
      <Alert>
        <Info aria-hidden />
        <AlertTitle>Default</AlertTitle>
        <AlertDescription>Neutral information.</AlertDescription>
      </Alert>
      <Alert variant="success">
        <CheckCircle2 aria-hidden />
        <AlertTitle>Success</AlertTitle>
        <AlertDescription>Everything completed.</AlertDescription>
      </Alert>
      <Alert variant="warning">
        <AlertCircle aria-hidden />
        <AlertTitle>Warning</AlertTitle>
        <AlertDescription>Something needs attention.</AlertDescription>
      </Alert>
      <Alert variant="destructive">
        <AlertTriangle aria-hidden />
        <AlertTitle>Destructive</AlertTitle>
        <AlertDescription>An operation failed.</AlertDescription>
      </Alert>
    </div>
  ),
};
