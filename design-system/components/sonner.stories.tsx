import type { Meta, StoryObj } from "@storybook/react";
import { toast } from "sonner";

import { Button } from "./button";
import { Toaster } from "./sonner";

const meta = {
  title: "Components/Sonner",
  component: Toaster,
  parameters: {
    layout: "padded",
    docs: {
      description: {
        component:
          "Toast host. Mount one `Toaster` near the app root, then call `toast()` from anywhere. Design tokens are injected as inline CSS variables because sonner ships an unlayered stylesheet that outranks Tailwind.",
      },
    },
  },
  tags: ["autodocs"],
  argTypes: {
    position: {
      control: "select",
      options: [
        "top-left",
        "top-center",
        "top-right",
        "bottom-left",
        "bottom-center",
        "bottom-right",
      ],
    },
    richColors: { control: "boolean" },
    expand: { control: "boolean" },
    closeButton: { control: "boolean" },
  },
  args: { position: "bottom-right", richColors: true },
} satisfies Meta<typeof Toaster>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: (args) => (
    <div className="flex flex-wrap items-center gap-2">
      <Toaster {...args} />
      <Button onClick={() => toast("Standup compiled")}>Show toast</Button>
    </div>
  ),
};

export const Variants: Story = {
  render: (args) => (
    <div className="flex flex-wrap items-center gap-2">
      <Toaster {...args} />
      <Button variant="outline" onClick={() => toast("Neutral message")}>
        Default
      </Button>
      <Button variant="outline" onClick={() => toast.success("Compile done")}>
        Success
      </Button>
      <Button variant="outline" onClick={() => toast.error("Provider timed out")}>
        Error
      </Button>
      <Button variant="outline" onClick={() => toast.warning("2 of 8 sources unreachable")}>
        Warning
      </Button>
      <Button variant="outline" onClick={() => toast.info("Deterministic fallback used")}>
        Info
      </Button>
    </div>
  ),
};

export const WithDescription: Story = {
  render: (args) => (
    <div className="flex flex-wrap items-center gap-2">
      <Toaster {...args} />
      <Button
        onClick={() =>
          toast.success("Saved to keychain", {
            description: "The API key never touches config JSON or the logs.",
          })
        }
      >
        Show
      </Button>
    </div>
  ),
};

export const WithAction: Story = {
  render: (args) => (
    <div className="flex flex-wrap items-center gap-2">
      <Toaster {...args} />
      <Button
        onClick={() =>
          toast("Scheduler installed", {
            description: "Runs weekdays at 08:30.",
            action: { label: "Undo", onClick: () => toast("Reverted") },
          })
        }
      >
        Show
      </Button>
    </div>
  ),
};

export const Loading: Story = {
  render: (args) => (
    <div className="flex flex-wrap items-center gap-2">
      <Toaster {...args} />
      <Button
        onClick={() =>
          toast.promise(new Promise((resolve) => setTimeout(resolve, 1800)), {
            loading: "Gathering evidence…",
            success: "Standup written",
            error: "Compile failed",
          })
        }
      >
        Run pipeline
      </Button>
    </div>
  ),
};

export const WithCloseButton: Story = {
  args: { closeButton: true },
  render: (args) => (
    <div className="flex flex-wrap items-center gap-2">
      <Toaster {...args} />
      <Button onClick={() => toast("Dismiss me with the corner button")}>Show</Button>
    </div>
  ),
};

export const Expanded: Story = {
  args: { expand: true },
  render: (args) => (
    <div className="flex flex-wrap items-center gap-2">
      <Toaster {...args} />
      <Button
        onClick={() => {
          toast.success("Gather done");
          toast.info("Redaction applied");
          toast.success("Standup written");
        }}
      >
        Stack three toasts
      </Button>
    </div>
  ),
};

export const Positions: Story = {
  args: { position: "top-center" },
  render: (args) => (
    <div className="flex flex-wrap items-center gap-2">
      <Toaster {...args} />
      <Button onClick={() => toast(`Position: ${args.position}`)}>Show toast</Button>
    </div>
  ),
};
