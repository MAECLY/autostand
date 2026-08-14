/**
 * Copy-to-clipboard button. Shows a copy icon; flips to a check for 2s after a
 * successful copy. Sits in any standup-rendering surface.
 */

import { Check, Copy } from "lucide-react";

import { Button, type ButtonProps } from "@autostand/ui/components/button";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@autostand/ui/components/tooltip";

import { useClipboard } from "@/hooks/use-clipboard";
import { cn } from "@/lib/utils";

export interface CopyButtonProps {
  /** Text placed on the clipboard. */
  text: string;
  /** Accessible label — required because the button is icon-only by default. */
  label: string;
  /** Tooltip shown on hover. Defaults to `label`. */
  tooltip?: string;
  variant?: ButtonProps["variant"];
  size?: ButtonProps["size"];
  className?: string;
}

export function CopyButton({
  text,
  label,
  tooltip,
  variant = "ghost",
  size = "icon",
  className,
}: CopyButtonProps) {
  const { copy, copied } = useClipboard();

  return (
    <TooltipProvider delayDuration={300}>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            type="button"
            variant={variant}
            size={size}
            aria-label={label}
            onClick={() => {
              void copy(text);
            }}
            className={cn(className)}
          >
            {copied ? (
              <Check className="text-success" aria-hidden />
            ) : (
              <Copy aria-hidden />
            )}
          </Button>
        </TooltipTrigger>
        <TooltipContent>{copied ? "Copied" : (tooltip ?? label)}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}