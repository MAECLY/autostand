import type { Meta, StoryObj } from "@storybook/react";

import { Badge } from "./badge";
import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableFooter,
  TableHead,
  TableHeader,
  TableRow,
} from "./table";

interface AuditRow {
  bullet: string;
  source: string;
  classification: "commit" | "github" | "review" | "note" | "phantom";
  refs: number;
}

const ROWS: AuditRow[] = [
  { bullet: "Implemented the LLM adapter trait", source: "local-git", classification: "commit", refs: 4 },
  { bullet: "Reviewed PR #128", source: "github", classification: "review", refs: 1 },
  { bullet: "Opened issue #131", source: "github", classification: "github", refs: 1 },
  { bullet: "Discussed cron edge cases", source: "claude-code", classification: "note", refs: 0 },
  { bullet: "Shipped the landing page", source: "—", classification: "phantom", refs: 0 },
];

const BADGE_VARIANT: Record<AuditRow["classification"], "success" | "secondary" | "outline" | "error"> = {
  commit: "success",
  github: "secondary",
  review: "secondary",
  note: "outline",
  phantom: "error",
};

const meta = {
  title: "Components/Table",
  component: Table,
  parameters: {
    layout: "padded",
    docs: {
      description: {
        component:
          "Data table. `Table` wraps the `<table>` in a horizontal scroll container so wide content cannot break the layout — style that wrapper via `wrapperClassName`.",
      },
    },
  },
  tags: ["autodocs"],
} satisfies Meta<typeof Table>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Bullet</TableHead>
          <TableHead>Source</TableHead>
          <TableHead>Classification</TableHead>
          <TableHead className="text-right">Refs</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {ROWS.map((row) => (
          <TableRow key={row.bullet}>
            <TableCell>{row.bullet}</TableCell>
            <TableCell className="font-mono text-xs">{row.source}</TableCell>
            <TableCell>
              <Badge variant={BADGE_VARIANT[row.classification]}>{row.classification}</Badge>
            </TableCell>
            <TableCell className="text-right font-mono">{row.refs}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  ),
};

export const WithCaption: Story = {
  render: () => (
    <Table>
      <TableCaption>Audit sidecar for 2026-08-03.</TableCaption>
      <TableHeader>
        <TableRow>
          <TableHead>Bullet</TableHead>
          <TableHead>Source</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {ROWS.slice(0, 3).map((row) => (
          <TableRow key={row.bullet}>
            <TableCell>{row.bullet}</TableCell>
            <TableCell className="font-mono text-xs">{row.source}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  ),
};

export const WithFooter: Story = {
  render: () => (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Source</TableHead>
          <TableHead className="text-right">Bullets</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        <TableRow>
          <TableCell className="font-mono text-xs">local-git</TableCell>
          <TableCell className="text-right font-mono">7</TableCell>
        </TableRow>
        <TableRow>
          <TableCell className="font-mono text-xs">github</TableCell>
          <TableCell className="text-right font-mono">3</TableCell>
        </TableRow>
      </TableBody>
      <TableFooter>
        <TableRow>
          <TableCell>Total</TableCell>
          <TableCell className="text-right font-mono">10</TableCell>
        </TableRow>
      </TableFooter>
    </Table>
  ),
};

export const SelectedRow: Story = {
  parameters: {
    docs: { description: { story: "`data-state=\"selected\"` keeps the hover surface pinned." } },
  },
  render: () => (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Date</TableHead>
          <TableHead>Provider</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        <TableRow>
          <TableCell className="font-mono">2026-08-01</TableCell>
          <TableCell>Claude</TableCell>
        </TableRow>
        <TableRow data-state="selected">
          <TableCell className="font-mono">2026-07-31</TableCell>
          <TableCell>Ollama</TableCell>
        </TableRow>
        <TableRow>
          <TableCell className="font-mono">2026-07-30</TableCell>
          <TableCell>Deterministic</TableCell>
        </TableRow>
      </TableBody>
    </Table>
  ),
};

export const Empty: Story = {
  render: () => (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Bullet</TableHead>
          <TableHead>Source</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        <TableRow>
          <TableCell colSpan={2} className="py-10 text-center text-muted-foreground">
            No audit entries for this date.
          </TableCell>
        </TableRow>
      </TableBody>
    </Table>
  ),
};

export const Wide: Story = {
  parameters: {
    docs: { description: { story: "Overflowing columns scroll inside the wrapper, not the page." } },
  },
  render: () => (
    <Table wrapperClassName="max-w-md rounded-lg border border-border">
      <TableHeader>
        <TableRow>
          <TableHead className="whitespace-nowrap">Bullet</TableHead>
          <TableHead className="whitespace-nowrap">Source</TableHead>
          <TableHead className="whitespace-nowrap">Classification</TableHead>
          <TableHead className="whitespace-nowrap">Repository</TableHead>
          <TableHead className="whitespace-nowrap">Commit</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {ROWS.map((row) => (
          <TableRow key={row.bullet}>
            <TableCell className="whitespace-nowrap">{row.bullet}</TableCell>
            <TableCell className="whitespace-nowrap font-mono text-xs">{row.source}</TableCell>
            <TableCell className="whitespace-nowrap">{row.classification}</TableCell>
            <TableCell className="whitespace-nowrap font-mono text-xs">autostand</TableCell>
            <TableCell className="whitespace-nowrap font-mono text-xs">83f1d66</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  ),
};
