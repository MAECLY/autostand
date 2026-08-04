import type { Meta, StoryObj } from "@storybook/react";
import { useState } from "react";
import { Copy, Download, MoreHorizontal, RefreshCw, Trash2 } from "lucide-react";

import { Button } from "./button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "./dropdown-menu";

const SOURCES = ["local-git", "github", "claude-code", "opencode"] as const;

function SourceFilterMenu() {
  const [enabled, setEnabled] = useState<string[]>(["local-git", "github"]);

  return (
    <div className="flex flex-col items-center gap-3">
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="outline">Filter sources</Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-56">
          <DropdownMenuLabel>Data sources</DropdownMenuLabel>
          <DropdownMenuSeparator />
          {SOURCES.map((source) => (
            <DropdownMenuCheckboxItem
              key={source}
              checked={enabled.includes(source)}
              disabled={source === "local-git"}
              onCheckedChange={(next) =>
                setEnabled((prev) =>
                  next ? [...prev, source] : prev.filter((item) => item !== source),
                )
              }
            >
              {source}
            </DropdownMenuCheckboxItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
      <p className="text-xs text-muted-foreground">
        Enabled: <span className="font-mono">{enabled.join(", ") || "none"}</span>
      </p>
    </div>
  );
}

function ProviderRadioMenu() {
  const [provider, setProvider] = useState("claude");

  return (
    <div className="flex flex-col items-center gap-3">
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="outline">Provider</Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-56">
          <DropdownMenuLabel>Compile with</DropdownMenuLabel>
          <DropdownMenuSeparator />
          <DropdownMenuRadioGroup value={provider} onValueChange={setProvider}>
            <DropdownMenuRadioItem value="claude">Claude</DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="ollama">Ollama</DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="openai">OpenAI / Codex</DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="gemini">Gemini</DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="grok">Grok</DropdownMenuRadioItem>
          </DropdownMenuRadioGroup>
        </DropdownMenuContent>
      </DropdownMenu>
      <p className="text-xs text-muted-foreground">
        Selected: <span className="font-mono">{provider}</span>
      </p>
    </div>
  );
}

const meta = {
  title: "Components/DropdownMenu",
  component: DropdownMenu,
  parameters: {
    layout: "centered",
    docs: {
      description: {
        component:
          "Action menu (Radix). Items highlight with a primary wash because the muted surface matches the popover background.",
      },
    },
  },
  tags: ["autodocs"],
} satisfies Meta<typeof DropdownMenu>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon" aria-label="More actions">
          <MoreHorizontal aria-hidden />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-56">
        <DropdownMenuLabel>2026-08-03</DropdownMenuLabel>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuItem>
            <RefreshCw aria-hidden />
            Recompile
            <DropdownMenuShortcut>⌘R</DropdownMenuShortcut>
          </DropdownMenuItem>
          <DropdownMenuItem>
            <Copy aria-hidden />
            Copy markdown
            <DropdownMenuShortcut>⌘C</DropdownMenuShortcut>
          </DropdownMenuItem>
          <DropdownMenuItem>
            <Download aria-hidden />
            Export audit
          </DropdownMenuItem>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuItem variant="destructive">
          <Trash2 aria-hidden />
          Discard draft
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  ),
};

export const WithInsetItems: Story = {
  parameters: {
    docs: { description: { story: "`inset` aligns plain items with checkbox and radio siblings." } },
  },
  render: () => (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline">View</Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-56">
        <DropdownMenuLabel inset>Layout</DropdownMenuLabel>
        <DropdownMenuCheckboxItem checked>Show audit column</DropdownMenuCheckboxItem>
        <DropdownMenuItem inset>Reset layout</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  ),
};

export const WithSubmenu: Story = {
  render: () => (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline">Compile</Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-56">
        <DropdownMenuItem>
          <RefreshCw aria-hidden />
          Compile now
        </DropdownMenuItem>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>Compile with…</DropdownMenuSubTrigger>
          <DropdownMenuSubContent className="w-48">
            <DropdownMenuItem>Claude</DropdownMenuItem>
            <DropdownMenuItem>Ollama</DropdownMenuItem>
            <DropdownMenuItem>OpenAI / Codex</DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem>Deterministic only</DropdownMenuItem>
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuSeparator />
        <DropdownMenuItem disabled>Compile yesterday (backdating blocked)</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  ),
};

export const DisabledItems: Story = {
  render: () => (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline">Actions</Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-56">
        <DropdownMenuItem>Open standup file</DropdownMenuItem>
        <DropdownMenuItem disabled>Push to remote (no upstream)</DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem variant="destructive" disabled>
          <Trash2 aria-hidden />
          Delete (accumulate-never-delete)
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  ),
};

export const CheckboxItems: Story = { render: () => <SourceFilterMenu /> };

export const RadioItems: Story = { render: () => <ProviderRadioMenu /> };
