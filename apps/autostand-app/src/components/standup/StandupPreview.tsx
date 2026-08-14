/**
 * Read-only render of one standup file: one Card per AUTO block plus the
 * single global MANUAL region.
 *
 * File grammar: `docs/specs/standup-file-format.md`.
 */

import { Inbox } from "lucide-react";
import Markdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";

import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@autostand/ui/components/alert";
import { Badge } from "@autostand/ui/components/badge";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@autostand/ui/components/card";

import { CopyButton } from "@/components/common/CopyButton";
import type { StandupFileContent } from "@/lib/types";
import { cn, hostColor, serializeStandup, stripHtmlComments } from "@/lib/utils";

/**
 * Overrides so markdown output picks up the tokens. Props are taken one by one
 * rather than spread: the rest object carries react-markdown's `node`, which
 * React would then forward to the DOM.
 */
const markdownComponents: Components = {
  a: ({ href, title, children }) => (
    <a
      href={href}
      title={title}
      target="_blank"
      rel="noreferrer noopener"
      className="text-primary underline underline-offset-2"
    >
      {children}
    </a>
  ),
  // Fenced blocks arrive as `code.language-*` inside a `pre`; only inline code
  // gets its own chip so the two do not stack backgrounds.
  code: ({ className, children }) => (
    <code
      className={cn(
        "font-mono text-sm",
        !className?.includes("language-") && "rounded-sm bg-inset px-1 py-0.5",
      )}
    >
      {children}
    </code>
  ),
  pre: ({ children }) => (
    <pre className="max-h-[40rem] overflow-auto rounded-md bg-inset p-3 font-mono text-sm">
      {children}
    </pre>
  ),
  ul: ({ children }) => (
    <ul className="list-disc space-y-1 pl-5">{children}</ul>
  ),
  li: ({ children }) => <li className="leading-relaxed">{children}</li>,
  p: ({ children }) => <p className="leading-relaxed">{children}</p>,
};

export interface StandupMarkdownProps {
  children: string;
  className?: string;
}

/** Markdown renderer wired to the design tokens. No raw HTML, ever. */
export function StandupMarkdown({ children, className }: StandupMarkdownProps) {
  return (
    <div className={cn("space-y-3 text-sm text-foreground", className)}>
      <Markdown remarkPlugins={[remarkGfm]} components={markdownComponents}>
        {children}
      </Markdown>
    </div>
  );
}

export interface StandupPreviewProps {
  content: StandupFileContent;
  /** Slug of this machine — its AUTO block is highlighted. */
  hostSlug?: string;
}

export function StandupPreview({ content, hostSlug }: StandupPreviewProps) {
  const manual = stripHtmlComments(content.manual_region);
  const hasBlocks = content.auto_blocks.length > 0 || manual.length > 0;

  if (!hasBlocks) {
    return (
      <Alert>
        <Inbox />
        <AlertTitle>Nothing filed for {content.date}</AlertTitle>
        <AlertDescription>
          Compile a standup or add a manual item to populate this file.
        </AlertDescription>
      </Alert>
    );
  }

  return (
    <div className="min-w-0 space-y-4">
      <header className="flex items-start justify-between gap-3 space-y-1">
        <div className="min-w-0 space-y-1">
          <h2 className="text-lg font-semibold text-foreground">
            {content.title}
          </h2>
          <StandupMarkdown className="text-muted-foreground">
            {content.subtitle}
          </StandupMarkdown>
        </div>
        <CopyButton
          text={serializeStandup(content)}
          label="Copy standup"
          tooltip="Copy full standup as markdown"
          size="sm"
          className="mt-0.5 shrink-0"
        />
      </header>

      {content.auto_blocks.map((block) => {
        const isLocal = block.host === hostSlug;
        return (
          <Card
            key={block.host}
            className={cn(isLocal && "ring-1 ring-ring")}
          >
            <CardHeader className="flex flex-row items-center justify-between gap-2">
              <CardTitle className="text-sm font-medium text-subtle">
                Auto
              </CardTitle>
              <div className="flex items-center gap-1">
                <CopyButton
                  text={stripHtmlComments(block.body)}
                  label={`Copy AUTO block for ${block.host}`}
                  tooltip="Copy this block"
                  size="sm"
                />
                <Badge
                  variant={isLocal ? "default" : "outline"}
                  className={cn("font-mono", !isLocal && hostColor(block.host))}
                >
                  {block.host}
                </Badge>
              </div>
            </CardHeader>
            <CardContent>
              <StandupMarkdown>{stripHtmlComments(block.body)}</StandupMarkdown>
            </CardContent>
          </Card>
        );
      })}

      {manual.length > 0 && (
        <Card>
          <CardHeader className="flex flex-row items-center justify-between gap-2 rounded-t-lg bg-warning-bg">
            <CardTitle className="text-sm font-medium text-warning">
              Manual
            </CardTitle>
            <div className="flex items-center gap-1">
              <CopyButton
                text={manual}
                label="Copy manual region"
                tooltip="Copy manual items"
                size="sm"
              />
              <Badge variant="warning">never overwritten</Badge>
            </div>
          </CardHeader>
          <CardContent>
            <StandupMarkdown>{manual}</StandupMarkdown>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
