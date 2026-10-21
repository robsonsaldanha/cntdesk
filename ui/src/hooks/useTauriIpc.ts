import { useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { InputEvent } from "../types/ipc";

interface FramePayload {
  data: number[];
  width: number;
  height: number;
  is_keyframe: boolean;
}

export function useTauriIpc() {
  const sendInput = useCallback((event: InputEvent) => {
    invoke("send_input", { event }).catch((err: unknown) => {
      console.error("Failed to send input:", err);
    });
  }, []);

  // Listen for incoming frame data from Rust backend and dispatch to VideoCanvas
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    const setup = async () => {
      try {
        unlisten = await listen<FramePayload>("cntdesk-frame", (event) => {
          const { data, width, height, is_keyframe } = event.payload;
          const frameData = new Uint8Array(data);

          // Dispatch custom event for VideoCanvas to consume
          window.dispatchEvent(
            new CustomEvent("cntdesk-frame", {
              detail: {
                data: frameData,
                width,
                height,
                isKeyframe: is_keyframe,
              },
            }),
          );
        });
      } catch (err) {
        console.warn("Failed to listen for frame events:", err);
      }
    };

    setup();

    return () => {
      unlisten?.();
    };
  }, []);

  return { sendInput };
}