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

import type { StandupFileContent } from "@/lib/types";
import { cn, hostColor } from "@/lib/utils";

/**
 * Block bodies may contain HTML comments (the MANUAL placeholder line, block
 * markers left by a union merge). We never enable `rehype-raw`, so strip them
 * instead of leaving react-markdown to silently drop them mid-paragraph.
 */
const HTML_COMMENT_LINE = /^\s*<!--.*-->\s*$/;

function stripHtmlComments(body: string): string {
  return body
    .split("\n")
    .filter((line) => !HTML_COMMENT_LINE.test(line))
    .join("\n")
    .trim();
}

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
    <pre className="overflow-x-auto rounded-md bg-inset p-3 font-mono text-sm">
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
    <div className="space-y-4">
      <header className="space-y-1">
        <h2 className="text-lg font-semibold text-foreground">
          {content.title}
        </h2>
        <StandupMarkdown className="text-muted-foreground">
          {content.subtitle}
        </StandupMarkdown>
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
              <Badge
                variant={isLocal ? "default" : "outline"}
                className={cn("font-mono", !isLocal && hostColor(block.host))}
              >
                {block.host}
              </Badge>
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
            <Badge variant="warning">never overwritten</Badge>
          </CardHeader>
          <CardContent>
            <StandupMarkdown>{manual}</StandupMarkdown>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
