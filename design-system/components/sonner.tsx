import type * as React from "react";
import { Toaster as SonnerToaster, type ToasterProps } from "sonner";

import { cn } from "../lib/utils";

/* Sonner ships its own unlayered stylesheet, which outranks Tailwind's utility layer.
   Its CSS custom properties are the only reliable override point, so the design tokens
   are injected inline (inline styles beat sonner's own `[data-sonner-toaster]` rules).
   The values inherit from `:root` / `.dark`, so toasts flip with the app theme. */
const toasterTokens = {
  "--normal-bg": "var(--bg-elevated)",
  "--normal-text": "var(--fg-base)",
  "--normal-border": "var(--border-default)",
  "--success-bg": "var(--status-success-bg)",
  "--success-text": "var(--status-success)",
  "--success-border": "var(--status-success)",
  "--error-bg": "var(--status-error-bg)",
  "--error-text": "var(--status-error)",
  "--error-border": "var(--status-error)",
  "--warning-bg": "var(--status-warning-bg)",
  "--warning-text": "var(--status-warning)",
  "--warning-border": "var(--status-warning)",
  "--info-bg": "var(--status-info-bg)",
  "--info-text": "var(--status-info)",
  "--info-border": "var(--status-info)",
  "--border-radius": "var(--radius-lg)",
} as React.CSSProperties;

export type { ToasterProps };

function Toaster({ className, style, toastOptions, richColors = true, ...props }: ToasterProps) {
  return (
    <SonnerToaster
      className={cn("toaster group font-sans", className)}
      richColors={richColors}
      style={{ ...toasterTokens, ...style }}
      toastOptions={{
        ...toastOptions,
        classNames: {
          title: "font-medium",
          description: "text-sm",
          ...toastOptions?.classNames,
        },
      }}
      {...props}
    />
  );
}

export { Toaster };
