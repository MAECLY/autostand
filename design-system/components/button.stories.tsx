import type { Meta, StoryObj } from "@storybook/react";
import { Plus, RefreshCw, Trash2 } from "lucide-react";

import { Button } from "./button";
import { Spinner } from "./spinner";

const meta = {
  title: "Components/Button",
  component: Button,
  parameters: {
    layout: "centered",
    docs: {
      description: {
        component:
          "Primary action control. Six variants (`default`, `destructive`, `outline`, `secondary`, `ghost`, `link`) and four sizes (`sm`, `default`, `lg`, `icon`).",
      },
    },
  },
  tags: ["autodocs"],
  argTypes: {
    variant: {
      control: "select",
      options: ["default", "destructive", "outline", "secondary", "ghost", "link"],
    },
    size: { control: "select", options: ["sm", "default", "lg", "icon"] },
    asChild: { control: false },
  },
  args: { children: "Button", disabled: false },
} satisfies Meta<typeof Button>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = { args: { variant: "default" } };

export const Destructive: Story = { args: { variant: "destructive", children: "Delete" } };

export const Outline: Story = { args: { variant: "outline", children: "Cancel" } };

export const Secondary: Story = { args: { variant: "secondary", children: "Secondary" } };

export const Ghost: Story = { args: { variant: "ghost", children: "Ghost" } };

export const Link: Story = { args: { variant: "link", children: "View audit sidecar" } };

export const Small: Story = { args: { size: "sm", children: "Small" } };

export const Large: Story = { args: { size: "lg", children: "Large" } };

export const Icon: Story = {
  args: { size: "icon", children: <Plus aria-hidden /> },
};

export const WithIcon: Story = {
  args: {
    children: (
      <>
        <RefreshCw aria-hidden />
        Compile now
      </>
    ),
  },
};

export const Loading: Story = {
  args: {
    disabled: true,
    children: (
      <>
        <Spinner label="Compiling" />
        Compiling…
      </>
    ),
  },
};

export const Disabled: Story = { args: { disabled: true, children: "Disabled" } };

export const AsChild: Story = {
  parameters: {
    docs: { description: { story: "`asChild` renders the child element with the button styles." } },
  },
  render: () => (
    <Button asChild variant="link">
      <a href="#storybook-root">Anchor styled as a button</a>
    </Button>
  ),
};

export const Variants: Story = {
  render: () => (
    <div className="flex flex-wrap items-center gap-2">
      <Button variant="default">Default</Button>
      <Button variant="destructive">Destructive</Button>
      <Button variant="outline">Outline</Button>
      <Button variant="secondary">Secondary</Button>
      <Button variant="ghost">Ghost</Button>
      <Button variant="link">Link</Button>
    </div>
  ),
};

export const Sizes: Story = {
  render: () => (
    <div className="flex flex-wrap items-center gap-2">
      <Button size="sm">Small</Button>
      <Button size="default">Default</Button>
      <Button size="lg">Large</Button>
      <Button size="icon" aria-label="Add item">
        <Plus aria-hidden />
      </Button>
    </div>
  ),
};

export const States: Story = {
  render: () => (
    <div className="flex flex-wrap items-center gap-2">
      <Button>Normal</Button>
      <Button disabled>Disabled</Button>
      <Button disabled>
        <Spinner label="Loading" />
        Loading
      </Button>
      <Button variant="destructive" disabled>
        <Trash2 aria-hidden />
        Disabled destructive
      </Button>
    </div>
  ),
};
