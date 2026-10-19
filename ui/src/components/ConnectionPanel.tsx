import { useState, useEffect } from "react";
import { useTauriIpc } from "../hooks/useTauriIpc";

export function ConnectionPanel() {
  const ipc = useTauriIpc();
  const [deviceId, setDeviceId] = useState<string | null>(null);
  const [remoteId, setRemoteId] = useState("");
  const [password, setPassword] = useState("");
  const [permanentPassword, setPermanentPassword] = useState("");
  const [oneTimePassword, setOneTimePassword] = useState<string | null>(null);
  const [connectionState, setConnectionState] = useState<string>("disconnected");

  useEffect(() => {
    ipc.getDeviceId().then(setDeviceId);
    ipc.getConnectionState().then(setConnectionState);
  }, []);

  const handleConnect = async () => {
    if (!remoteId || !password) return;
    const ok = await ipc.connect(remoteId, password);
    if (ok) setConnectionState("connected");
  };

  const handleDisconnect = async () => {
    const ok = await ipc.disconnect();
    if (ok) setConnectionState("disconnected");
  };

  const handleSetPassword = async () => {
    if (!permanentPassword) return;
    await ipc.setPassword(permanentPassword);
    setPermanentPassword("");
  };

  const handleGenerateOtp = async () => {
    const otp = await ipc.generateOneTimePassword();
    setOneTimePassword(otp);
  };

  return (
    <div style={{
      width: 280,
      padding: 16,
      background: "#1a1a2e",
      color: "#eee",
      display: "flex",
      flexDirection: "column",
      gap: 12,
      borderRight: "1px solid #333",
      fontFamily: "system-ui, sans-serif",
      fontSize: 14,
    }}>
      <h2 style={{ margin: 0, fontSize: 18 }}>CNT Desk</h2>

      <div>
        <label style={{ display: "block", marginBottom: 4, opacity: 0.7 }}>Your ID</label>
        <div style={{
          padding: "8px 12px",
          background: "#0f0f23",
          borderRadius: 4,
          fontFamily: "monospace",
          letterSpacing: 1,
        }}>
          {deviceId ?? "—"}
        </div>
      </div>

      <hr style={{ border: "none", borderTop: "1px solid #333" }} />

      <div>
        <label style={{ display: "block", marginBottom: 4, opacity: 0.7 }}>Remote ID</label>
        <input
          value={remoteId}
          onChange={(e) => setRemoteId(e.target.value)}
          placeholder="XXX-XXX-XXX"
          disabled={connectionState === "connected"}
          style={{
            width: "100%",
            padding: "8px 12px",
            background: "#0f0f23",
            border: "1px solid #333",
            borderRadius: 4,
            color: "#eee",
            boxSizing: "border-box",
            fontFamily: "monospace",
          }}
        />
      </div>

      <div>
        <label style={{ display: "block", marginBottom: 4, opacity: 0.7 }}>Password</label>
        <input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          disabled={connectionState === "connected"}
          style={{
            width: "100%",
            padding: "8px 12px",
            background: "#0f0f23",
            border: "1px solid #333",
            borderRadius: 4,
            color: "#eee",
            boxSizing: "border-box",
          }}
        />
      </div>

      {connectionState === "connected" ? (
        <button onClick={handleDisconnect} style={{
          padding: "10px",
          background: "#c0392b",
          color: "#fff",
          border: "none",
          borderRadius: 4,
          cursor: "pointer",
          fontWeight: 600,
        }}>
          Disconnect
        </button>
      ) : (
        <button onClick={handleConnect} disabled={ipc.loading || !remoteId || !password} style={{
          padding: "10px",
          background: "#27ae60",
          color: "#fff",
          border: "none",
          borderRadius: 4,
          cursor: ipc.loading ? "wait" : "pointer",
          fontWeight: 600,
          opacity: (!remoteId || !password) ? 0.5 : 1,
        }}>
          {ipc.loading ? "Connecting…" : "Connect"}
        </button>
      )}

      <div style={{
        padding: "6px 10px",
        background: connectionState === "connected" ? "#27ae6033" : "#33333366",
        borderRadius: 4,
        textAlign: "center",
        fontSize: 12,
        textTransform: "uppercase",
        letterSpacing: 1,
      }}>
        {connectionState}
      </div>

      <hr style={{ border: "none", borderTop: "1px solid #333" }} />

      <div>
        <label style={{ display: "block", marginBottom: 4, opacity: 0.7 }}>Set Permanent Password</label>
        <input
          type="password"
          value={permanentPassword}
          onChange={(e) => setPermanentPassword(e.target.value)}
          placeholder="Min 8 characters"
          style={{
            width: "100%",
            padding: "8px 12px",
            background: "#0f0f23",
            border: "1px solid #333",
            borderRadius: 4,
            color: "#eee",
            boxSizing: "border-box",
            marginBottom: 8,
          }}
        />
        <button onClick={handleSetPassword} disabled={!permanentPassword} style={{
          width: "100%",
          padding: "8px",
          background: "#2980b9",
          color: "#fff",
          border: "none",
          borderRadius: 4,
          cursor: "pointer",
          fontSize: 12,
        }}>
          Set Password
        </button>
      </div>

      <div>
        <button onClick={handleGenerateOtp} style={{
          width: "100%",
          padding: "8px",
          background: "#8e44ad",
          color: "#fff",
          border: "none",
          borderRadius: 4,
          cursor: "pointer",
          fontSize: 12,
        }}>
          Generate One-Time Password
        </button>
        {oneTimePassword && (
          <div style={{
            marginTop: 8,
            padding: "8px 12px",
            background: "#0f0f23",
            borderRadius: 4,
            fontFamily: "monospace",
            textAlign: "center",
            letterSpacing: 2,
            fontSize: 16,
          }}>
            {oneTimePassword}
          </div>
        )}
      </div>

      {ipc.error && (
        <div style={{
          padding: "8px 12px",
          background: "#c0392b33",
          border: "1px solid #c0392b",
          borderRadius: 4,
          fontSize: 12,
          wordBreak: "break-word",
        }}>
          {ipc.error}
        </div>
      )}
    </div>
  );
}