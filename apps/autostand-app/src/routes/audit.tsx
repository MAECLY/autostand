import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/audit")({
  component: AuditPage,
});

function AuditPage() {
  return <div className="p-6">Audit</div>;
}
