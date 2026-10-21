import { useState, useCallback, useEffect } from "react";
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
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const sendInput = useCallback((event: InputEvent) => {
    invoke("send_input", { event }).catch((err: unknown) => {
      console.error("Failed to send input:", err);
    });
  }, []);

  const getDeviceId = useCallback(async (): Promise<string | null> => {
    try {
      const id = await invoke<string | null>("get_device_id");
      return id;
    } catch (err) {
      console.error("get_device_id failed:", err);
      return null;
    }
  }, []);

  const getConnectionState = useCallback(async (): Promise<string> => {
    try {
      const state = await invoke<{ Disconnected?: null; Connecting?: null; Connected?: { remote_id: string }; Hosting?: { device_id: string } }>("get_connection_state");
      if (state.Disconnected !== undefined) return "disconnected";
      if (state.Connecting !== undefined) return "connecting";
      if (state.Connected) return "connected";
      if (state.Hosting) return "hosting";
      return "disconnected";
    } catch (err) {
      console.error("get_connection_state failed:", err);
      return "disconnected";
    }
  }, []);

  const connect = useCallback(async (remoteId: string, password: string): Promise<boolean> => {
    setLoading(true);
    setError(null);
    try {
      await invoke("connect", { remoteId, password });
      return true;
    } catch (err) {
      const msg = typeof err === "string" ? err : String(err);
      setError(msg);
      return false;
    } finally {
      setLoading(false);
    }
  }, []);

  const disconnect = useCallback(async (): Promise<boolean> => {
    setLoading(true);
    setError(null);
    try {
      await invoke("disconnect");
      return true;
    } catch (err) {
      const msg = typeof err === "string" ? err : String(err);
      setError(msg);
      return false;
    } finally {
      setLoading(false);
    }
  }, []);

  const setPassword = useCallback(async (password: string): Promise<void> => {
    setError(null);
    try {
      await invoke("set_password", { password });
    } catch (err) {
      const msg = typeof err === "string" ? err : String(err);
      setError(msg);
    }
  }, []);

  const generateOneTimePassword = useCallback(async (): Promise<string> => {
    setError(null);
    try {
      const otp = await invoke<string>("generate_one_time_password");
      return otp;
    } catch (err) {
      const msg = typeof err === "string" ? err : String(err);
      setError(msg);
      return "";
    }
  }, []);

  // Listen for incoming frame data from Rust backend and dispatch to VideoCanvas
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    const setup = async () => {
      try {
        unlisten = await listen<FramePayload>("cntdesk-frame", (event) => {
          const { data, width, height, is_keyframe } = event.payload;
          const frameData = new Uint8Array(data);
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

  return {
    sendInput,
    getDeviceId,
    getConnectionState,
    connect,
    disconnect,
    setPassword,
    generateOneTimePassword,
    loading,
    error,
  };
}