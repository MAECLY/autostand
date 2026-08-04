/**
 * Labelled directory field for the Settings → Paths tab.
 *
 * Validation is backend-owned: `validate_paths` re-checks every configured path
 * and this component picks the entry whose `label` matches its `field`, so the
 * badge can never disagree with what the pipeline will actually read.
 */

import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen } from "lucide-react";

import { Badge } from "@design-system/components/badge";
import { Button } from "@design-system/components/button";
import { Input } from "@design-system/components/input";
import { Label } from "@design-system/components/label";

import { useValidatePaths } from "@/hooks/use-paths";
import type { PathValidation } from "@/lib/types";

export interface PathInputProps {
  /** Human-readable field name, e.g. `GitHub directory`. */
  label: string;
  /** Config key the backend reports in `PathValidation.label`, e.g. `github_dir`. */
  field: string;
  value: string;
  onChange: (path: string) => void;
  placeholder?: string;
}

export function PathInput({
  label,
  field,
  value,
  onChange,
  placeholder,
}: PathInputProps) {
  const [validation, setValidation] = useState<PathValidation | null>(null);
  const validatePaths = useValidatePaths();

  const inputId = `path-${field}`;
  const ok = validation !== null && validation.exists && validation.readable;

  async function revalidate() {
    try {
      const results = await validatePaths.mutateAsync();
      setValidation(results.find((entry) => entry.label === field) ?? null);
    } catch {
      // A failed probe is not a verdict on the path — leave the badge unset.
      setValidation(null);
    }
  }

  async function browse() {
    const picked = await open({
      directory: true,
      multiple: false,
      defaultPath: value.length > 0 ? value : undefined,
      title: label,
    });
    if (picked === null) return;
    onChange(picked);
    await revalidate();
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <Label htmlFor={inputId}>{label}</Label>
        {validatePaths.isPending ? (
          <Badge variant="secondary">Checking…</Badge>
        ) : validation !== null ? (
          <Badge variant={ok ? "success" : "error"}>
            {ok ? "Found" : "Missing"}
          </Badge>
        ) : null}
      </div>

      <div className="flex items-center gap-2">
        <Input
          id={inputId}
          value={value}
          placeholder={placeholder}
          spellCheck={false}
          autoComplete="off"
          onChange={(event) => onChange(event.target.value)}
          onBlur={() => {
            void revalidate();
          }}
        />
        <Button
          type="button"
          variant="outline"
          onClick={() => {
            void browse();
          }}
        >
          <FolderOpen className="size-4" aria-hidden="true" />
          Browse…
        </Button>
      </div>

      {validation !== null ? (
        <div className="flex flex-col gap-1">
          <p className="font-mono text-xs text-muted-foreground">
            {validation.path}
          </p>
          {!ok && validation.message !== null ? (
            <p className="text-xs text-destructive">{validation.message}</p>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
