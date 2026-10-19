import { useCallback } from "react";
import { VideoCanvas } from "./components/VideoCanvas";
import { ConnectionPanel } from "./components/ConnectionPanel";
import { useTauriIpc } from "./hooks/useTauriIpc";
import type { InputEvent } from "./types/ipc";

export function App() {
  const ipc = useTauriIpc();

  const handleInput = useCallback(
    (event: InputEvent) => {
      ipc.sendInput(event);
    },
    [ipc],
  );

  return (
    <div style={{ display: "flex", width: "100vw", height: "100vh", overflow: "hidden" }}>
      <ConnectionPanel />
      <div style={{ flex: 1, position: "relative", background: "#111" }}>
        <VideoCanvas onInput={handleInput} />
      </div>
    </div>
  );
}