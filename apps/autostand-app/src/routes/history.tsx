import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/history")({
  component: HistoryPage,
});

function HistoryPage() {
  return <div className="p-6">History</div>;
}
