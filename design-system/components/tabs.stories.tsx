import type { Meta, StoryObj } from "@storybook/react";
import { useState } from "react";
import { Clock, Database, FolderTree, Shield, Sparkles } from "lucide-react";

import { Tabs, TabsContent, TabsList, TabsTrigger } from "./tabs";

const PANELS = [
  { value: "providers", label: "Providers", body: "Claude, Ollama, OpenAI, Gemini, Grok." },
  { value: "sources", label: "Data Sources", body: "Eight read-only sources, local-git first." },
  { value: "paths", label: "Paths", body: "Where standups are read from and written to." },
  { value: "scheduler", label: "Scheduler", body: "Cron expression, triggers and self-heal." },
  { value: "scrub", label: "Scrub", body: "Redaction patterns applied pre-LLM and pre-write." },
];

function ControlledTabs() {
  const [tab, setTab] = useState("sources");

  return (
    <div className="flex w-[36rem] flex-col gap-3">
      <Tabs value={tab} onValueChange={setTab}>
        <TabsList>
          {PANELS.map((panel) => (
            <TabsTrigger key={panel.value} value={panel.value}>
              {panel.label}
            </TabsTrigger>
          ))}
        </TabsList>
        {PANELS.map((panel) => (
          <TabsContent key={panel.value} value={panel.value} className="p-4 text-sm">
            {panel.body}
          </TabsContent>
        ))}
      </Tabs>
      <p className="text-xs text-muted-foreground">
        Active tab: <span className="font-mono">{tab}</span>
      </p>
    </div>
  );
}

const meta = {
  title: "Components/Tabs",
  component: Tabs,
  parameters: {
    layout: "padded",
    docs: {
      description: {
        component:
          "Tabbed sections. The list flips to a left rail when `orientation=\"vertical\"` is set on the root.",
      },
    },
  },
  tags: ["autodocs"],
  argTypes: {
    orientation: { control: "inline-radio", options: ["horizontal", "vertical"] },
  },
} satisfies Meta<typeof Tabs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <Tabs defaultValue="providers" className="w-[36rem]">
      <TabsList>
        {PANELS.map((panel) => (
          <TabsTrigger key={panel.value} value={panel.value}>
            {panel.label}
          </TabsTrigger>
        ))}
      </TabsList>
      {PANELS.map((panel) => (
        <TabsContent key={panel.value} value={panel.value} className="p-4 text-sm">
          {panel.body}
        </TabsContent>
      ))}
    </Tabs>
  ),
};

export const WithIcons: Story = {
  render: () => (
    <Tabs defaultValue="providers" className="w-[36rem]">
      <TabsList>
        <TabsTrigger value="providers">
          <Sparkles aria-hidden />
          Providers
        </TabsTrigger>
        <TabsTrigger value="sources">
          <Database aria-hidden />
          Data Sources
        </TabsTrigger>
        <TabsTrigger value="paths">
          <FolderTree aria-hidden />
          Paths
        </TabsTrigger>
        <TabsTrigger value="scheduler">
          <Clock aria-hidden />
          Scheduler
        </TabsTrigger>
        <TabsTrigger value="scrub">
          <Shield aria-hidden />
          Scrub
        </TabsTrigger>
      </TabsList>
      {PANELS.map((panel) => (
        <TabsContent key={panel.value} value={panel.value} className="p-4 text-sm">
          {panel.body}
        </TabsContent>
      ))}
    </Tabs>
  ),
};

export const Vertical: Story = {
  render: () => (
    <Tabs defaultValue="providers" orientation="vertical" className="w-[36rem]">
      <TabsList className="min-w-40">
        {PANELS.map((panel) => (
          <TabsTrigger key={panel.value} value={panel.value} className="justify-start">
            {panel.label}
          </TabsTrigger>
        ))}
      </TabsList>
      {PANELS.map((panel) => (
        <TabsContent key={panel.value} value={panel.value} className="p-4 text-sm">
          {panel.body}
        </TabsContent>
      ))}
    </Tabs>
  ),
};

export const DisabledTab: Story = {
  render: () => (
    <Tabs defaultValue="providers" className="w-[36rem]">
      <TabsList>
        <TabsTrigger value="providers">Providers</TabsTrigger>
        <TabsTrigger value="sources">Data Sources</TabsTrigger>
        <TabsTrigger value="enterprise" disabled>
          Enterprise
        </TabsTrigger>
      </TabsList>
      <TabsContent value="providers" className="p-4 text-sm">
        {PANELS[0].body}
      </TabsContent>
      <TabsContent value="sources" className="p-4 text-sm">
        {PANELS[1].body}
      </TabsContent>
      <TabsContent value="enterprise" className="p-4 text-sm">
        Unreachable while disabled.
      </TabsContent>
    </Tabs>
  ),
};

export const TwoTabs: Story = {
  render: () => (
    <Tabs defaultValue="auto" className="w-96">
      <TabsList>
        <TabsTrigger value="auto">AUTO</TabsTrigger>
        <TabsTrigger value="manual">MANUAL</TabsTrigger>
      </TabsList>
      <TabsContent value="auto" className="p-4 text-sm text-muted-foreground">
        Regenerated on every compile.
      </TabsContent>
      <TabsContent value="manual" className="p-4 text-sm text-muted-foreground">
        Yours — never overwritten.
      </TabsContent>
    </Tabs>
  ),
};

export const Controlled: Story = { render: () => <ControlledTabs /> };
