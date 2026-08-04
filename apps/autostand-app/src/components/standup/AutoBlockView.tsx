/**
 * One host's AUTO block with a preview/edit toggle. History is read-only, so
 * editing is opt-in via `editable`.
 */

import { useState } from "react";
import { Eye, Pencil } from "lucide-react";

import { Badge } from "@autostand/ui/components/badge";
import { Button } from "@autostand/ui/components/button";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@autostand/ui/components/card";
import { Textarea } from "@autostand/ui/components/textarea";

import { StandupMarkdown } from "@/components/standup/StandupPreview";
import type { AutoBlock } from "@/lib/types";

export interface AutoBlockViewProps {
  block: AutoBlock;
  /** Default false — history and the dashboard preview are read-only. */
  editable?: boolean;
  onEdit?: (block: AutoBlock) => void;
}

export function AutoBlockView({
  block,
  editable = false,
  onEdit,
}: AutoBlockViewProps) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(block.body);

  // Re-seed the draft when the parent hands us a different block (date or host
  // switch) rather than syncing in an effect, which would render stale text.
  const [seededFrom, setSeededFrom] = useState(block);
  if (seededFrom !== block) {
    setSeededFrom(block);
    setDraft(block.body);
    setEditing(false);
  }

  function handleChange(body: string) {
    setDraft(body);
    onEdit?.({ ...block, body });
  }

  // `editable` can be revoked by the parent mid-edit (switching to history).
  const isEditing = editable && editing;

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between gap-2">
        <CardTitle className="flex items-center gap-2 text-sm font-medium text-subtle">
          Auto
          <Badge variant="outline" className="font-mono">
            {block.host}
          </Badge>
        </CardTitle>
        {editable && (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => setEditing((value) => !value)}
          >
            {isEditing ? (
              <>
                <Eye />
                Preview
              </>
            ) : (
              <>
                <Pencil />
                Edit
              </>
            )}
          </Button>
        )}
      </CardHeader>
      <CardContent>
        {isEditing ? (
          <Textarea
            aria-label={`AUTO block for ${block.host}`}
            className="min-h-48 font-mono text-sm"
            value={draft}
            onChange={(event) => handleChange(event.target.value)}
          />
        ) : (
          <StandupMarkdown>{draft}</StandupMarkdown>
        )}
      </CardContent>
    </Card>
  );
}
