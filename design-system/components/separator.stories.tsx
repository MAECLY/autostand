import type { Meta, StoryObj } from "@storybook/react";

import { Separator } from "./separator";

const meta = {
  title: "Components/Separator",
  component: Separator,
  parameters: {
    layout: "centered",
    docs: { description: { component: "1px rule for splitting card and settings sections." } },
  },
  tags: ["autodocs"],
  argTypes: {
    orientation: { control: "inline-radio", options: ["horizontal", "vertical"] },
  },
} satisfies Meta<typeof Separator>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Horizontal: Story = {
  args: { orientation: "horizontal" },
  render: (args) => (
    <div className="w-80">
      <p className="pb-3 text-sm text-foreground">Providers</p>
      <Separator {...args} />
      <p className="pt-3 text-sm text-muted-foreground">Claude, Ollama, OpenAI, Gemini, Grok</p>
    </div>
  ),
};

export const Vertical: Story = {
  args: { orientation: "vertical" },
  render: (args) => (
    <div className="flex h-8 items-center gap-4 text-sm">
      <span>Today</span>
      <Separator {...args} />
      <span>History</span>
      <Separator {...args} />
      <span>Settings</span>
    </div>
  ),
};

export const NonDecorative: Story = {
  parameters: {
    docs: {
      description: {
        story:
          "`decorative={false}` exposes the separator to assistive tech as a real `role=\"separator\"`.",
      },
    },
  },
  args: { decorative: false },
  render: (args) => (
    <div className="w-80">
      <p className="pb-3 text-sm">Section A</p>
      <Separator {...args} />
      <p className="pt-3 text-sm">Section B</p>
    </div>
  ),
};

export const Orientations: Story = {
  render: () => (
    <div className="flex w-80 flex-col gap-6">
      <Separator />
      <div className="flex h-8 items-center gap-4">
        <span className="text-sm">Left</span>
        <Separator orientation="vertical" />
        <span className="text-sm">Right</span>
      </div>
    </div>
  ),
};
