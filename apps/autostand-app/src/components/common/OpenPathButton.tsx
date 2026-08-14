/**
 * Reveal a configured directory in the OS file manager. Sits next to any
 * surface that shows a folder path the user may want to inspect by hand.
 *
 * The button never guesses: existence is the backend's verdict, so callers that
 * already hold a `PathValidation` pass `disabled` and everyone else gets the
 * error toast the command raises.
 */

import { ExternalLink } from "lucide-react";

import { Button, type ButtonProps } from "@autostand/ui/components/button";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@autostand/ui/components/tooltip";

import { useOpenInFileManager } from "@/hooks/use-paths";
import { cn } from "@/lib/utils";

export interface OpenPathButtonProps {
  /** Absolute directory handed to the OS shell. */
  path: string;
  /** Accessible label — required because the button is icon-only by default. */
  label: string;
  /** Tooltip shown on hover. Defaults to `label`. */
  tooltip?: string;
  /** Force-disable, e.g. when `validate_paths` reported the path as missing. */
  disabled?: boolean;
  variant?: ButtonProps["variant"];
  size?: ButtonProps["size"];
  className?: string;
}

export function OpenPathButton({
  path,
  label,
  tooltip,
  disabled = false,
  variant = "ghost",
  size = "icon",
  className,
}: OpenPathButtonProps) {
  const openPath = useOpenInFileManager();
  const blank = path.trim().length === 0;

  return (
    <TooltipProvider delayDuration={300}>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            type="button"
            variant={variant}
            size={size}
            aria-label={label}
            disabled={disabled || blank || openPath.isPending}
            onClick={() => openPath.mutate(path)}
            className={cn(className)}
          >
            <ExternalLink aria-hidden />
          </Button>
        </TooltipTrigger>
        <TooltipContent>{tooltip ?? label}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
