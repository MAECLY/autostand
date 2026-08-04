import { useCallback, useEffect, useState } from "react";
import { Moon, Sun } from "lucide-react";

import { Button } from "@design-system/components/button";

/** Must match the key the pre-paint script in src/layouts/Base.astro reads. */
const THEME_STORAGE_KEY = "autostand-theme";

export interface ThemeToggleProps {
  /** Extra classes for the trigger, e.g. spacing inside the navbar row. */
  className?: string;
}

/**
 * Light/dark switch for the landing page. Interactive, so it is the one island
 * in the page chrome — everything else around it is static HTML.
 */
export function ThemeToggle({ className }: ThemeToggleProps) {
  // Base.astro resolves the theme before first paint, so the <html> class — not a
  // constant here — is the source of truth. The static HTML is built without
  // knowing the visitor's theme, hence the read-on-mount below.
  const [isDark, setIsDark] = useState(false);

  useEffect(() => {
    setIsDark(document.documentElement.classList.contains("dark"));
  }, []);

  const toggleTheme = useCallback(() => {
    const nextIsDark = !document.documentElement.classList.contains("dark");
    document.documentElement.classList.toggle("dark", nextIsDark);
    setIsDark(nextIsDark);

    try {
      localStorage.setItem(THEME_STORAGE_KEY, nextIsDark ? "dark" : "light");
    } catch {
      // Persistence is best-effort: storage blocked by the browser (private mode,
      // third-party cookie rules) must not break the toggle for this visit.
    }
  }, []);

  return (
    <Button
      type="button"
      variant="ghost"
      size="icon"
      className={className}
      onClick={toggleTheme}
      aria-label={isDark ? "Switch to light theme" : "Switch to dark theme"}
      aria-pressed={isDark}
    >
      {/* Icon visibility rides on the `dark:` variant rather than on React state,
          so the correct icon is already painted in the frame before hydration. */}
      <Sun className="hidden dark:block" aria-hidden="true" />
      <Moon className="block dark:hidden" aria-hidden="true" />
    </Button>
  );
}
