import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

function App() {
  const [hostSlug, setHostSlug] = useState<string>("…");

  useEffect(() => {
    invoke<string>("get_host_slug")
      .then(setHostSlug)
      .catch((e) => setHostSlug(`error: ${e}`));
  }, []);

  return (
    <main className="min-h-screen flex flex-col items-center justify-center gap-4 p-8">
      <h1 className="text-3xl font-bold text-primary">autostand</h1>
      <p className="text-muted-foreground">Cross-platform daily standup automation</p>
      <div className="rounded-lg border border-border bg-card p-4">
        <p className="text-sm text-muted-foreground">Host slug</p>
        <p className="font-mono">{hostSlug}</p>
      </div>
    </main>
  );
}

export default App;