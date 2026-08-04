/**
 * The questions a developer actually asks before running an unsigned binary that
 * reads their git history: what it talks to, what it costs, what it touches.
 *
 * Every answer is checked against the docs it cites. Nothing here promises a
 * feature that is not in the repo today.
 *
 * Needs `client:visible` — the Accordion is Radix, and a panel that cannot open
 * is worse than no panel at all.
 */
import type { ReactNode } from "react";

import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@design-system/components/accordion";
import { cn } from "@design-system/lib/utils";

const REPO_URL = "https://github.com/MAECLY/autostand";

const linkClass =
  "font-medium text-primary underline underline-offset-2 transition-colors hover:text-primary-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background rounded-sm";

/** Mono for things you would type or find on disk (docs/design-system/02-brand.md). */
function Code({ children }: { children: ReactNode }) {
  return <code className="font-mono text-sm text-foreground">{children}</code>;
}

interface FaqEntry {
  /** Stable accordion value — also the anchor a support reply can point at. */
  readonly id: string;
  readonly question: string;
  /** Two or three sentences. No more. */
  readonly answer: ReactNode;
}

const ENTRIES: readonly FaqEntry[] = [
  {
    id: "providers",
    question: "Which AI providers can write my standup?",
    answer: (
      <>
        Claude, Ollama, OpenAI/Codex, Gemini and Grok. Each one runs the local CLI first and
        falls back to that vendor's HTTP API, and you pick the default in Settings.
      </>
    ),
  },
  {
    id: "cost",
    question: "Do I need a subscription or an API key?",
    answer: (
      <>
        One of the two, not both. If you are already signed in to a provider's CLI, autostand
        reuses that session and never handles a key; otherwise it uses an API key you store in
        your OS keychain. Ollama runs on your own machine, so it needs neither.
      </>
    ),
  },
  {
    id: "sources",
    question: "What does it read?",
    answer: (
      <>
        Your local git history, your GitHub activity through the <Code>gh</Code> CLI, and your
        Claude Code, Remember, opencode, Codex, Gemini CLI and Grok CLI sessions. Every source
        is read-only. The only files autostand writes are your standup and its own local state.
      </>
    ),
  },
  {
    id: "network",
    question: "Does anything get sent to a server?",
    answer: (
      <>
        There is no autostand server, no account and no telemetry. The only outbound traffic is
        the request to the AI provider you configured — none at all if you render with Ollama or
        the deterministic renderer — plus <Code>git push</Code> and the <Code>gh</Code> CLI
        talking to GitHub with your own credentials. Gathering, scrubbing, writing and the audit
        trail all happen on your machine.
      </>
    ),
  },
  {
    id: "no-provider",
    question: "What happens when no AI provider is available?",
    answer: (
      <>
        A deterministic renderer is built into the pipeline, so a standup is written straight
        from your facts with no model involved. If the provider is missing, times out, or returns
        something the validator rejects, that render is what lands in the file. The audit
        sidecar records that it fell back.
      </>
    ),
  },
  {
    id: "two-machines",
    question: "I work on two machines. Do they fight over the file?",
    answer: (
      <>
        No. Each machine owns its own AUTO block, keyed by a stable host slug, so two machines
        write to different regions of the same dated file. The dailies repo marks those files{" "}
        <Code>merge=union</Code>, so git concatenates both sides instead of raising a conflict.
      </>
    ),
  },
  {
    id: "manual-notes",
    question: "Will it edit the notes I wrote myself?",
    answer: (
      <>
        Never. Anything between the MANUAL markers is yours, and a compile only replaces its own
        AUTO block. Bullets from an earlier run that the new render missed are put back —
        accumulate adds, it never deletes.
      </>
    ),
  },
  {
    id: "install",
    question: "How do I get it today?",
    answer: (
      <>
        Build it from source: clone{" "}
        <a className={linkClass} href={REPO_URL}>
          the repo
        </a>
        , then <Code>pnpm install</Code> and <Code>pnpm tauri dev</Code>. No installer is
        published yet, so that is the only way to run it. You will need Rust, Node 20+ and
        pnpm 9+.
      </>
    ),
  },
];

export interface FaqProps {
  /** Anchor target, so the page nav can link straight to this section. */
  id?: string;
  /** Extra classes for the outer `<section>` (page rhythm is the page's call). */
  className?: string;
}

export function Faq({ id = "faq", className }: FaqProps) {
  const headingId = `${id}-title`;

  return (
    <section id={id} aria-labelledby={headingId} className={cn("py-20 sm:py-28", className)}>
      <div className="mx-auto w-full max-w-3xl px-6">
        <p className="text-sm font-medium uppercase tracking-widest text-muted-foreground">
          Questions
        </p>
        <h2 id={headingId} className="mt-6 text-3xl font-bold tracking-tight sm:text-4xl">
          Before you run it
        </h2>

        {/* Radix drops closed panels from the DOM, so the first answer opens by
            default: something to read before hydration, and one answer a crawler
            can actually see. */}
        <Accordion
          type="single"
          collapsible
          defaultValue={ENTRIES[0]?.id}
          className="mt-10 overflow-hidden rounded-lg border border-border shadow-sm"
        >
          {ENTRIES.map((entry) => (
            <AccordionItem key={entry.id} value={entry.id}>
              <AccordionTrigger className="px-4 py-4 text-base sm:text-lg">
                {entry.question}
              </AccordionTrigger>
              <AccordionContent>
                <p className="max-w-prose text-base leading-relaxed text-muted-foreground">
                  {entry.answer}
                </p>
              </AccordionContent>
            </AccordionItem>
          ))}
        </Accordion>

        <p className="mt-8 text-sm text-muted-foreground">
          Something else?{" "}
          <a className={linkClass} href={`${REPO_URL}/issues`}>
            Open an issue
          </a>
          .
        </p>
      </div>
    </section>
  );
}
