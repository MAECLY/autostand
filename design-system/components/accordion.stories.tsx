import type { Meta, StoryObj } from "@storybook/react";
import { useState } from "react";

import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "./accordion";
import { Label } from "./label";
import { Switch } from "./switch";

const SECTIONS = [
  {
    value: "providers",
    title: "Providers",
    body: "Five AI providers, each with a CLI-first and an API mode. The deterministic renderer is the fallback for all of them.",
  },
  {
    value: "sources",
    title: "Data sources",
    body: "Eight read-only sources. local-git is authoritative and always enabled.",
  },
  {
    value: "scrub",
    title: "Scrub",
    body: "Secrets redaction runs twice: before the prompt leaves the machine and again before the file is written.",
  },
];

function ControlledAccordion() {
  const [value, setValue] = useState("providers");

  return (
    <div className="flex w-96 flex-col gap-3">
      <Accordion
        type="single"
        collapsible
        value={value}
        onValueChange={setValue}
        className="rounded-lg border border-border"
      >
        {SECTIONS.map((section) => (
          <AccordionItem key={section.value} value={section.value}>
            <AccordionTrigger>{section.title}</AccordionTrigger>
            <AccordionContent className="text-muted-foreground">{section.body}</AccordionContent>
          </AccordionItem>
        ))}
      </Accordion>
      <p className="text-xs text-muted-foreground">
        Open section: <span className="font-mono">{value || "none"}</span>
      </p>
    </div>
  );
}

const meta = {
  title: "Components/Accordion",
  component: Accordion,
  parameters: {
    layout: "padded",
    docs: {
      description: {
        component:
          "Collapsible section list. `type=\"single\"` opens one section at a time (add `collapsible` to allow closing it); `type=\"multiple\"` allows several.",
      },
    },
  },
  tags: ["autodocs"],
  args: { type: "single", collapsible: true },
} satisfies Meta<typeof Accordion>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <Accordion type="single" collapsible className="w-96 rounded-lg border border-border">
      {SECTIONS.map((section) => (
        <AccordionItem key={section.value} value={section.value}>
          <AccordionTrigger>{section.title}</AccordionTrigger>
          <AccordionContent className="text-muted-foreground">{section.body}</AccordionContent>
        </AccordionItem>
      ))}
    </Accordion>
  ),
};

export const DefaultOpen: Story = {
  render: () => (
    <Accordion
      type="single"
      collapsible
      defaultValue="sources"
      className="w-96 rounded-lg border border-border"
    >
      {SECTIONS.map((section) => (
        <AccordionItem key={section.value} value={section.value}>
          <AccordionTrigger>{section.title}</AccordionTrigger>
          <AccordionContent className="text-muted-foreground">{section.body}</AccordionContent>
        </AccordionItem>
      ))}
    </Accordion>
  ),
};

export const Multiple: Story = {
  render: () => (
    <Accordion
      type="multiple"
      defaultValue={["providers", "scrub"]}
      className="w-96 rounded-lg border border-border"
    >
      {SECTIONS.map((section) => (
        <AccordionItem key={section.value} value={section.value}>
          <AccordionTrigger>{section.title}</AccordionTrigger>
          <AccordionContent className="text-muted-foreground">{section.body}</AccordionContent>
        </AccordionItem>
      ))}
    </Accordion>
  ),
};

export const DisabledItem: Story = {
  render: () => (
    <Accordion type="single" collapsible className="w-96 rounded-lg border border-border">
      <AccordionItem value="providers">
        <AccordionTrigger>Providers</AccordionTrigger>
        <AccordionContent className="text-muted-foreground">{SECTIONS[0].body}</AccordionContent>
      </AccordionItem>
      <AccordionItem value="locked">
        <AccordionTrigger disabled>Enterprise policy (locked)</AccordionTrigger>
        <AccordionContent>Managed by your administrator.</AccordionContent>
      </AccordionItem>
    </Accordion>
  ),
};

export const WithControls: Story = {
  render: () => (
    <Accordion type="single" collapsible className="w-96 rounded-lg border border-border">
      <AccordionItem value="scheduler">
        <AccordionTrigger>Scheduler</AccordionTrigger>
        <AccordionContent>
          <div className="flex flex-col gap-4">
            <div className="flex items-center justify-between gap-4">
              <Label htmlFor="accordion-enabled">Enabled</Label>
              <Switch id="accordion-enabled" defaultChecked />
            </div>
            <div className="flex items-center justify-between gap-4">
              <Label htmlFor="accordion-heal">Self-heal missed runs</Label>
              <Switch id="accordion-heal" />
            </div>
          </div>
        </AccordionContent>
      </AccordionItem>
      <AccordionItem value="scrub">
        <AccordionTrigger>Scrub</AccordionTrigger>
        <AccordionContent className="text-muted-foreground">{SECTIONS[2].body}</AccordionContent>
      </AccordionItem>
    </Accordion>
  ),
};

export const Controlled: Story = { render: () => <ControlledAccordion /> };
