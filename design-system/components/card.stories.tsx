import type { Meta, StoryObj } from "@storybook/react";

import { Badge } from "./badge";
import { Button } from "./button";
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "./card";
import { Separator } from "./separator";

const meta = {
  title: "Components/Card",
  component: Card,
  parameters: {
    layout: "centered",
    docs: {
      description: {
        component:
          "Surface container. Compose `CardHeader` / `CardTitle` / `CardDescription` / `CardContent` / `CardFooter` as needed — every part is optional.",
      },
    },
  },
  tags: ["autodocs"],
} satisfies Meta<typeof Card>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <Card className="w-96">
      <CardHeader>
        <CardTitle>Claude</CardTitle>
        <CardDescription>CLI first, API fallback.</CardDescription>
      </CardHeader>
      <CardContent>
        <p className="text-sm text-muted-foreground">
          Runs <span className="font-mono">claude -p</span> against the gathered evidence and falls
          back to the deterministic renderer on failure.
        </p>
      </CardContent>
    </Card>
  ),
};

export const WithFooter: Story = {
  render: () => (
    <Card className="w-96">
      <CardHeader>
        <CardTitle>Scheduler</CardTitle>
        <CardDescription>Runs every weekday at 08:30.</CardDescription>
      </CardHeader>
      <CardContent>
        <p className="text-sm text-muted-foreground">
          Missed runs are self-healed on the next launch.
        </p>
      </CardContent>
      <CardFooter>
        <Button variant="outline">Disable</Button>
        <Button>Run now</Button>
      </CardFooter>
    </Card>
  ),
};

export const HeaderOnly: Story = {
  render: () => (
    <Card className="w-96">
      <CardHeader>
        <CardTitle>Host slug</CardTitle>
        <CardDescription className="font-mono">macbook-pro-m3</CardDescription>
      </CardHeader>
    </Card>
  ),
};

export const ContentOnly: Story = {
  render: () => (
    <Card className="w-96">
      <CardContent className="pt-6">
        <p className="text-sm">A bare card is just a surface with a border and a shadow.</p>
      </CardContent>
    </Card>
  ),
};

export const WithHeaderAction: Story = {
  render: () => (
    <Card className="w-96">
      <CardHeader className="flex-row items-start justify-between gap-4">
        <div className="flex flex-col gap-1.5">
          <CardTitle>local-git</CardTitle>
          <CardDescription>Authoritative source, always on.</CardDescription>
        </div>
        <Badge variant="success">OK</Badge>
      </CardHeader>
      <CardContent>
        <p className="text-sm text-muted-foreground">3 repositories scanned, 12 commits found.</p>
      </CardContent>
    </Card>
  ),
};

export const WithSeparator: Story = {
  render: () => (
    <Card className="w-96">
      <CardHeader>
        <CardTitle>Data sources</CardTitle>
        <CardDescription>All read-only.</CardDescription>
      </CardHeader>
      <Separator />
      <CardContent className="pt-6 text-sm text-muted-foreground">
        Sources are merged with a union driver, never overwritten.
      </CardContent>
    </Card>
  ),
};

export const Grid: Story = {
  parameters: { layout: "padded" },
  render: () => (
    <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
      {["Claude", "Ollama", "OpenAI", "Gemini", "Grok"].map((provider) => (
        <Card key={provider}>
          <CardHeader>
            <CardTitle>{provider}</CardTitle>
            <CardDescription>CLI + API</CardDescription>
          </CardHeader>
          <CardFooter>
            <Button size="sm" variant="outline">
              Configure
            </Button>
          </CardFooter>
        </Card>
      ))}
    </div>
  ),
};
