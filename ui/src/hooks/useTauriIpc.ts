import { useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { InputEvent, ConnectionState } from "../types/ipc";

export function useTauriIpc() {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const getDeviceId = useCallback(async (): Promise<string | null> => {
    try {
      setLoading(true);
      setError(null);
      return await invoke<string | null>("get_device_id");
    } catch (e) {
      setError(String(e));
      return null;
    } finally {
      setLoading(false);
    }
  }, []);

  const setPassword = useCallback(async (password: string): Promise<boolean> => {
    try {
      setLoading(true);
      setError(null);
      await invoke("set_password", { password });
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    } finally {
      setLoading(false);
    }
  }, []);

  const generateOneTimePassword = useCallback(async (): Promise<string | null> => {
    try {
      setLoading(true);
      setError(null);
      return await invoke<string>("generate_one_time_password");
    } catch (e) {
      setError(String(e));
      return null;
    } finally {
      setLoading(false);
    }
  }, []);

  const connect = useCallback(async (remoteId: string, password: string): Promise<boolean> => {
    try {
      setLoading(true);
      setError(null);
      await invoke("connect", { remoteId, password });
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    } finally {
      setLoading(false);
    }
  }, []);

  const disconnect = useCallback(async (): Promise<boolean> => {
    try {
      setLoading(true);
      setError(null);
      await invoke("disconnect");
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    } finally {
      setLoading(false);
    }
  }, []);

  const sendInput = useCallback(async (event: InputEvent): Promise<void> => {
    try {
      await invoke("send_input", { event });
    } catch (e) {
      console.error("send_input failed:", e);
    }
  }, []);

  const getConnectionState = useCallback(async (): Promise<ConnectionState> => {
    try {
      return await invoke<ConnectionState>("get_connection_state");
    } catch {
      return "disconnected";
    }
  }, []);

  return {
    loading,
    error,
    getDeviceId,
    setPassword,
    generateOneTimePassword,
    connect,
    disconnect,
    sendInput,
    getConnectionState,
  };
}