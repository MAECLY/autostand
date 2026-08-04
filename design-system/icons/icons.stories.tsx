import type { Meta, StoryObj } from "@storybook/react";
import { GitBranch, GitPullRequest, Terminal } from "lucide-react";

import {
  AuditCommitIcon,
  AuditGithubIcon,
  AuditPhantomIcon,
  HostIcon,
  PipelineIcon,
  StandupFileIcon,
  type IconProps,
} from "./index";

type IconComponent = (props: IconProps) => ReturnType<typeof StandupFileIcon>;

const iconSet: ReadonlyArray<{ name: string; file: string; Icon: IconComponent }> = [
  { name: "StandupFileIcon", file: "standup-file.svg", Icon: StandupFileIcon },
  { name: "PipelineIcon", file: "pipeline.svg", Icon: PipelineIcon },
  { name: "HostIcon", file: "host.svg", Icon: HostIcon },
  { name: "AuditPhantomIcon", file: "audit-phantom.svg", Icon: AuditPhantomIcon },
  { name: "AuditCommitIcon", file: "audit-commit.svg", Icon: AuditCommitIcon },
  { name: "AuditGithubIcon", file: "audit-github.svg", Icon: AuditGithubIcon },
];

const meta = {
  title: "Brand/Icons",
  component: StandupFileIcon,
  parameters: {
    layout: "padded",
    docs: {
      description: {
        component:
          "Custom icons for concepts lucide does not cover. Same 24 grid, 2px stroke and rounded joins as lucide, drawn with `currentColor` so they inherit text colour. The `.svg` files in `design-system/icons/` are the source of truth.",
      },
    },
  },
  tags: ["autodocs"],
  argTypes: {
    size: { control: { type: "number", min: 12, max: 96, step: 4 } },
  },
} satisfies Meta<typeof StandupFileIcon>;

export default meta;
type Story = StoryObj<typeof meta>;

export const StandupFile: Story = { args: { size: 32 } };

export const Pipeline: Story = { args: { size: 32 }, render: (args) => <PipelineIcon {...args} /> };

export const Host: Story = { args: { size: 32 }, render: (args) => <HostIcon {...args} /> };

export const AuditPhantom: Story = {
  args: { size: 32 },
  render: (args) => <AuditPhantomIcon {...args} />,
};

export const AuditCommit: Story = {
  args: { size: 32 },
  render: (args) => <AuditCommitIcon {...args} />,
};

export const AuditGithub: Story = {
  args: { size: 32 },
  render: (args) => <AuditGithubIcon {...args} />,
};

export const AllIcons: Story = {
  parameters: {
    docs: {
      description: {
        story:
          "Every icon at the three sizes it actually ships at: 16px inline with text, 24px in buttons and table cells, 32px in empty states and landing cards.",
      },
    },
  },
  render: () => (
    <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
      {iconSet.map(({ name, file, Icon }) => (
        <div
          key={name}
          className="flex flex-col gap-3 rounded-lg border border-border bg-card p-4"
        >
          <div className="flex items-end gap-4 text-foreground">
            <Icon size={16} />
            <Icon size={24} />
            <Icon size={32} />
          </div>
          <div className="flex flex-col gap-0.5">
            <span className="text-sm font-medium text-foreground">{name}</span>
            <span className="font-mono text-xs text-muted-foreground">{file}</span>
          </div>
        </div>
      ))}
    </div>
  ),
};

export const InheritsTextColor: Story = {
  parameters: {
    docs: {
      description: {
        story:
          "Nothing is hardcoded: every stroke (and the one filled GitHub path) uses `currentColor`, so an icon takes the colour of whatever it sits in. The audit row uses the audit classification tokens.",
      },
    },
  },
  render: () => (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-center gap-6">
        {["text-foreground", "text-muted-foreground", "text-primary", "text-accent"].map(
          (tone) => (
            <div key={tone} className={`flex items-center gap-2 ${tone}`}>
              <StandupFileIcon size={20} />
              <PipelineIcon size={20} />
              <HostIcon size={20} />
              <span className="font-mono text-xs">{tone}</span>
            </div>
          ),
        )}
      </div>

      <div className="flex flex-wrap items-center gap-6">
        <span className="flex items-center gap-2 text-audit-commit">
          <AuditCommitIcon size={20} />
          <span className="text-sm">commit</span>
        </span>
        <span className="flex items-center gap-2 text-audit-github">
          <AuditGithubIcon size={20} />
          <span className="text-sm">github</span>
        </span>
        <span className="flex items-center gap-2 text-audit-phantom">
          <AuditPhantomIcon size={20} />
          <span className="text-sm">phantom</span>
        </span>
      </div>

      <p className="flex items-center gap-1.5 text-sm text-muted-foreground">
        Inline at 16px:
        <StandupFileIcon size={16} />
        <AuditCommitIcon size={16} />
        <AuditPhantomIcon size={16} />
        icons match the cap height of the text they sit in.
      </p>
    </div>
  ),
};

export const OnDark: Story = {
  parameters: {
    docs: {
      description: {
        story:
          "The same icons inside a `.dark` container. The tokens flip; the icons need no dark variant because they never name a colour.",
      },
    },
  },
  render: () => (
    <div className="dark flex flex-col gap-6 rounded-lg border border-border bg-background p-6">
      <div className="flex flex-wrap items-center gap-6 text-foreground">
        {iconSet.map(({ name, Icon }) => (
          <Icon key={name} size={32} />
        ))}
      </div>
      <div className="flex flex-wrap items-center gap-6">
        <span className="flex items-center gap-2 text-audit-commit">
          <AuditCommitIcon size={20} />
          <span className="text-sm">commit</span>
        </span>
        <span className="flex items-center gap-2 text-audit-github">
          <AuditGithubIcon size={20} />
          <span className="text-sm">github</span>
        </span>
        <span className="flex items-center gap-2 text-audit-phantom">
          <AuditPhantomIcon size={20} />
          <span className="text-sm">phantom</span>
        </span>
      </div>
      <p className="text-sm text-muted-foreground">
        Same six icons, no <code className="font-mono text-xs">dark:</code> variant anywhere.
      </p>
    </div>
  ),
};

export const NextToLucide: Story = {
  parameters: {
    docs: {
      description: {
        story:
          "The stroke test. Custom icons are interleaved with lucide icons at 24px — if one looks heavier or rounder than its neighbours, the drawing is wrong.",
      },
    },
  },
  render: () => (
    <div className="flex flex-wrap items-center gap-5 text-foreground">
      <GitBranch aria-hidden="true" width={24} height={24} />
      <PipelineIcon size={24} />
      <Terminal aria-hidden="true" width={24} height={24} />
      <HostIcon size={24} />
      <GitPullRequest aria-hidden="true" width={24} height={24} />
      <AuditCommitIcon size={24} />
      <StandupFileIcon size={24} />
      <AuditPhantomIcon size={24} />
      <AuditGithubIcon size={24} />
    </div>
  ),
};

export const Labelled: Story = {
  parameters: {
    docs: {
      description: {
        story:
          "Icons default to `aria-hidden`, which is right when adjacent text already names them. When an icon is the only content, pass `aria-hidden={false}` with a `role` and an accessible name.",
      },
    },
  },
  render: () => (
    <div className="flex items-center gap-6 text-foreground">
      <AuditPhantomIcon size={24} aria-hidden={false} role="img" aria-label="Phantom bullet" />
      <span className="text-sm text-muted-foreground">
        Exposed to assistive tech as &ldquo;Phantom bullet&rdquo;.
      </span>
    </div>
  ),
};
