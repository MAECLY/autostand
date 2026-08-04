import type { Meta, StoryObj } from "@storybook/react";

import { Button } from "./button";
import { Spinner } from "./spinner";

const meta = {
  title: "Components/Spinner",
  component: Spinner,
  parameters: {
    layout: "centered",
    docs: {
      description: {
        component:
          "Indeterminate loading glyph. The icon is decorative; `label` is what screen readers announce.",
      },
    },
  },
  tags: ["autodocs"],
  argTypes: {
    size: { control: "inline-radio", options: ["sm", "default", "lg"] },
  },
} satisfies Meta<typeof Spinner>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = { args: { size: "default" } };

export const Small: Story = { args: { size: "sm" } };

export const Large: Story = { args: { size: "lg" } };

export const CustomLabel: Story = { args: { size: "lg", label: "Gathering evidence" } };

export const OnPrimary: Story = {
  args: { size: "default", className: "text-primary" },
};

export const Sizes: Story = {
  render: () => (
    <div className="flex items-center gap-6">
      <Spinner size="sm" label="Small" />
      <Spinner size="default" label="Default" />
      <Spinner size="lg" label="Large" />
    </div>
  ),
};

export const InButton: Story = {
  render: () => (
    <div className="flex items-center gap-2">
      <Button disabled>
        <Spinner label="Compiling" />
        Compiling…
      </Button>
      <Button variant="outline" size="sm" disabled>
        <Spinner size="sm" label="Saving" />
        Saving
      </Button>
      <Button variant="ghost" size="icon" disabled aria-label="Loading">
        <Spinner label="Loading" />
      </Button>
    </div>
  ),
};

export const InlineWithText: Story = {
  render: () => (
    <p className="flex items-center gap-2 text-sm text-muted-foreground">
      <Spinner size="sm" label="Reading sources" />
      Reading 8 data sources…
    </p>
  ),
};
