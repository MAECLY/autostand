import type { Meta, StoryObj } from "@storybook/react";
import { useState } from "react";

import { Label } from "./label";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "./select";

const MODELS = [
  "claude-opus-4",
  "claude-sonnet-4",
  "gpt-5",
  "gemini-2.5-pro",
  "grok-4",
  "llama3.2",
];

function ControlledSelect() {
  const [value, setValue] = useState("claude-sonnet-4");

  return (
    <div className="flex w-64 flex-col gap-3">
      <Select value={value} onValueChange={setValue}>
        <SelectTrigger aria-label="Model">
          <SelectValue placeholder="Pick a model" />
        </SelectTrigger>
        <SelectContent>
          {MODELS.map((model) => (
            <SelectItem key={model} value={model}>
              {model}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <p className="text-xs text-muted-foreground">
        Selected: <span className="font-mono">{value}</span>
      </p>
    </div>
  );
}

const meta = {
  title: "Components/Select",
  component: Select,
  parameters: {
    layout: "centered",
    docs: {
      description: {
        component:
          "Single-choice dropdown (Radix). The content is portalled and sized to the trigger via `--radix-select-trigger-width`.",
      },
    },
  },
  tags: ["autodocs"],
} satisfies Meta<typeof Select>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <Select>
      <SelectTrigger className="w-64" aria-label="Provider">
        <SelectValue placeholder="Select a provider" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="claude">Claude</SelectItem>
        <SelectItem value="ollama">Ollama</SelectItem>
        <SelectItem value="openai">OpenAI / Codex</SelectItem>
        <SelectItem value="gemini">Gemini</SelectItem>
        <SelectItem value="grok">Grok</SelectItem>
      </SelectContent>
    </Select>
  ),
};

export const WithDefaultValue: Story = {
  render: () => (
    <Select defaultValue="cli">
      <SelectTrigger className="w-64" aria-label="Provider mode">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="cli">CLI first, API fallback</SelectItem>
        <SelectItem value="api">API only</SelectItem>
        <SelectItem value="off">Deterministic only</SelectItem>
      </SelectContent>
    </Select>
  ),
};

export const WithGroups: Story = {
  render: () => (
    <Select>
      <SelectTrigger className="w-64" aria-label="Model">
        <SelectValue placeholder="Select a model" />
      </SelectTrigger>
      <SelectContent>
        <SelectGroup>
          <SelectLabel>Hosted</SelectLabel>
          <SelectItem value="claude-opus-4">claude-opus-4</SelectItem>
          <SelectItem value="gpt-5">gpt-5</SelectItem>
          <SelectItem value="gemini-2.5-pro">gemini-2.5-pro</SelectItem>
        </SelectGroup>
        <SelectSeparator />
        <SelectGroup>
          <SelectLabel>Local</SelectLabel>
          <SelectItem value="llama3.2">llama3.2</SelectItem>
          <SelectItem value="qwen2.5-coder">qwen2.5-coder</SelectItem>
        </SelectGroup>
      </SelectContent>
    </Select>
  ),
};

export const WithLabel: Story = {
  render: () => (
    <div className="flex w-64 flex-col gap-2">
      <Label htmlFor="date-filter">Date range</Label>
      <Select defaultValue="7d">
        <SelectTrigger id="date-filter">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="7d">Last 7 days</SelectItem>
          <SelectItem value="30d">Last 30 days</SelectItem>
          <SelectItem value="all">All time</SelectItem>
        </SelectContent>
      </Select>
    </div>
  ),
};

export const DisabledItem: Story = {
  render: () => (
    <Select>
      <SelectTrigger className="w-64" aria-label="Provider">
        <SelectValue placeholder="Select a provider" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="claude">Claude</SelectItem>
        <SelectItem value="ollama" disabled>
          Ollama (not installed)
        </SelectItem>
        <SelectItem value="grok">Grok</SelectItem>
      </SelectContent>
    </Select>
  ),
};

export const Disabled: Story = {
  render: () => (
    <Select disabled defaultValue="claude">
      <SelectTrigger className="w-64" aria-label="Provider">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="claude">Claude</SelectItem>
      </SelectContent>
    </Select>
  ),
};

export const LongList: Story = {
  parameters: {
    docs: {
      description: {
        story: "The content caps at the available viewport height and grows scroll buttons.",
      },
    },
  },
  render: () => (
    <Select>
      <SelectTrigger className="w-64" aria-label="Repository">
        <SelectValue placeholder="Select a repository" />
      </SelectTrigger>
      <SelectContent>
        {Array.from({ length: 40 }, (_, index) => (
          <SelectItem key={index} value={`repo-${index}`}>
            repo-{String(index).padStart(2, "0")}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  ),
};

export const Controlled: Story = { render: () => <ControlledSelect /> };
