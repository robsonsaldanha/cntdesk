import { useState, useEffect, useRef } from "react";
import { useTauriIpc } from "../hooks/useTauriIpc";

type ConnectionState = "disconnected" | "waiting" | "pairing" | "connected" | "error";

interface LogEntry {
  timestamp: string;
  level: "info" | "warn" | "error";
  message: string;
}

export function ConnectionPanel() {
  const ipc = useTauriIpc();
  const [deviceId, setDeviceId] = useState<string | null>(null);
  const [remoteId, setRemoteId] = useState("");
  const [password, setPassword] = useState("");
  const [permanentPassword, setPermanentPassword] = useState("");
  const [oneTimePassword, setOneTimePassword] = useState<string | null>(null);
  const [connectionState, setConnectionState] = useState<ConnectionState>("disconnected");
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const logsEndRef = useRef<HTMLDivElement>(null);

  // Auto-scroll logs to bottom
  useEffect(() => {
    logsEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  // Add log helper
  const addLog = (level: LogEntry["level"], message: string) => {
    const timestamp = new Date().toLocaleTimeString("pt-BR", { hour12: false });
    setLogs(prev => [...prev, { timestamp, level, message }]);
  };

  // Auto-register as host on mount
  useEffect(() => {
    const init = async () => {
      addLog("info", "Iniciando CNT Desk...");

      // Get device ID
      const id = await ipc.getDeviceId();
      if (id) {
        setDeviceId(id);
        addLog("info", `ID do dispositivo: ${id}`);
      }

      // Get initial connection state
      const state = await ipc.getConnectionState();
      setConnectionState(state as ConnectionState);

      // Auto-register as host (connect to server and wait for incoming connections)
      addLog("info", "Conectando ao servidor CNT Desk...");
      try {
        // The connect command with empty remoteId triggers host registration
        const ok = await ipc.connect("", "");
        if (ok) {
          setConnectionState("waiting");
          addLog("info", "Registrado como host. Aguardando conexões...");
        } else {
          setConnectionState("error");
          addLog("error", "Falha ao registrar como host");
        }
      } catch (err) {
        setConnectionState("error");
        addLog("error", `Erro ao conectar: ${err}`);
      }
    };

    init();
  }, []);

  const handleConnect = async () => {
    if (!remoteId || !password) {
      addLog("warn", "Preencha Remote ID e Password");
      return;
    }

    setConnectionState("pairing");
    addLog("info", `Pareando com ${remoteId}...`);

    const ok = await ipc.connect(remoteId, password);
    if (ok) {
      setConnectionState("connected");
      addLog("info", `Conectado a ${remoteId}`);
    } else {
      setConnectionState("error");
      addLog("error", `Falha ao conectar com ${remoteId}`);
    }
  };

  const handleDisconnect = async () => {
    addLog("info", "Desconectando...");
    const ok = await ipc.disconnect();
    if (ok) {
      setConnectionState("waiting");
      addLog("info", "Desconectado. Voltando ao modo Aguardando...");
      // Re-register as host
      await ipc.connect("", "");
    }
  };

  const handleSetPassword = async () => {
    if (!permanentPassword) {
      addLog("warn", "Digite uma senha permanente");
      return;
    }
    await ipc.setPassword(permanentPassword);
    setPermanentPassword("");
    addLog("info", "Senha permanente definida");
  };

  const handleGenerateOtp = async () => {
    const otp = await ipc.generateOneTimePassword();
    setOneTimePassword(otp);
    addLog("info", `Senha temporária gerada: ${otp}`);
  };

  const getStateLabel = (state: ConnectionState): string => {
    switch (state) {
      case "disconnected": return "DESCONECTADO";
      case "waiting": return "AGUARDANDO";
      case "pairing": return "PAREANDO";
      case "connected": return "CONECTADO";
      case "error": return "ERRO";
      default: return state.toUpperCase();
    }
  };

  const getStateColor = (state: ConnectionState): string => {
    switch (state) {
      case "disconnected": return "#666";
      case "waiting": return "#f39c12";
      case "pairing": return "#3498db";
      case "connected": return "#27ae60";
      case "error": return "#c0392b";
      default: return "#666";
    }
  };

  return (
    <div style={{
      width: 320,
      padding: 20,
      background: "linear-gradient(180deg, #1a1f1a 0%, #0f1410 100%)",
      color: "#e8e8e8",
      display: "flex",
      flexDirection: "column",
      gap: 16,
      borderRight: "2px solid #2d3a2d",
      fontFamily: "'Segoe UI', system-ui, sans-serif",
      fontSize: 14,
      overflow: "hidden",
    }}>
      {/* Header with Logo */}
      <div style={{ display: "flex", alignItems: "center", gap: 12, marginBottom: 8 }}>
        <img src="/logo.png" alt="CNT Desk" style={{ width: 40, height: 40, objectFit: "contain" }} />
        <h2 style={{ margin: 0, fontSize: 20, fontWeight: 700, color: "#a8c686" }}>CNT Desk</h2>
      </div>

      {/* Your ID */}
      <div>
        <label style={{ display: "block", marginBottom: 6, opacity: 0.7, fontSize: 12, textTransform: "uppercase", letterSpacing: 1 }}>Seu ID</label>
        <div style={{
          padding: "10px 14px",
          background: "#0a0f0a",
          borderRadius: 6,
          fontFamily: "'Courier New', monospace",
          letterSpacing: 2,
          fontSize: 16,
          border: "1px solid #2d3a2d",
          color: "#a8c686",
        }}>
          {deviceId ?? "—"}
        </div>
      </div>

      {/* Status Badge */}
      <div style={{
        padding: "10px 14px",
        background: `${getStateColor(connectionState)}22`,
        borderRadius: 6,
        textAlign: "center",
        fontSize: 13,
        fontWeight: 600,
        textTransform: "uppercase",
        letterSpacing: 1.5,
        border: `1px solid ${getStateColor(connectionState)}44`,
        color: getStateColor(connectionState),
      }}>
        {getStateLabel(connectionState)}
      </div>

      <hr style={{ border: "none", borderTop: "1px solid #2d3a2d", margin: "4px 0" }} />

      {/* Remote ID */}
      <div>
        <label style={{ display: "block", marginBottom: 6, opacity: 0.7, fontSize: 12, textTransform: "uppercase", letterSpacing: 1 }}>ID Remoto</label>
        <input
          value={remoteId}
          onChange={(e) => setRemoteId(e.target.value)}
          placeholder="XXX-XXX-XXX"
          disabled={connectionState === "connected" || connectionState === "pairing"}
          style={{
            width: "100%",
            padding: "10px 14px",
            background: "#0a0f0a",
            border: "1px solid #2d3a2d",
            borderRadius: 6,
            color: "#e8e8e8",
            boxSizing: "border-box",
            fontFamily: "'Courier New', monospace",
            fontSize: 14,
          }}
        />
      </div>

      {/* Password */}
      <div>
        <label style={{ display: "block", marginBottom: 6, opacity: 0.7, fontSize: 12, textTransform: "uppercase", letterSpacing: 1 }}>Senha</label>
        <input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          disabled={connectionState === "connected" || connectionState === "pairing"}
          style={{
            width: "100%",
            padding: "10px 14px",
            background: "#0a0f0a",
            border: "1px solid #2d3a2d",
            borderRadius: 6,
            color: "#e8e8e8",
            boxSizing: "border-box",
            fontSize: 14,
          }}
        />
      </div>

      {/* Connect/Disconnect Button */}
      {connectionState === "connected" ? (
        <button onClick={handleDisconnect} style={{
          padding: "12px",
          background: "linear-gradient(135deg, #c0392b 0%, #a93226 100%)",
          color: "#fff",
          border: "none",
          borderRadius: 6,
          cursor: "pointer",
          fontWeight: 600,
          fontSize: 14,
          textTransform: "uppercase",
          letterSpacing: 1,
        }}>
          Desconectar
        </button>
      ) : (
        <button
          onClick={handleConnect}
          disabled={ipc.loading || !remoteId || !password || connectionState === "pairing"}
          style={{
            padding: "12px",
            background: connectionState === "pairing"
              ? "linear-gradient(135deg, #3498db 0%, #2980b9 100%)"
              : "linear-gradient(135deg, #27ae60 0%, #229954 100%)",
            color: "#fff",
            border: "none",
            borderRadius: 6,
            cursor: (ipc.loading || !remoteId || !password || connectionState === "pairing") ? "not-allowed" : "pointer",
            fontWeight: 600,
            fontSize: 14,
            textTransform: "uppercase",
            letterSpacing: 1,
            opacity: (!remoteId || !password || connectionState === "pairing") ? 0.6 : 1,
          }}
        >
          {connectionState === "pairing" ? "Pareando..." : ipc.loading ? "Conectando..." : "Conectar"}
        </button>
      )}

      <hr style={{ border: "none", borderTop: "1px solid #2d3a2d", margin: "4px 0" }} />

      {/* Set Permanent Password */}
      <div>
        <label style={{ display: "block", marginBottom: 6, opacity: 0.7, fontSize: 12, textTransform: "uppercase", letterSpacing: 1 }}>Senha Permanente</label>
        <input
          type="password"
          value={permanentPassword}
          onChange={(e) => setPermanentPassword(e.target.value)}
          placeholder="Mínimo 8 caracteres"
          style={{
            width: "100%",
            padding: "10px 14px",
            background: "#0a0f0a",
            border: "1px solid #2d3a2d",
            borderRadius: 6,
            color: "#e8e8e8",
            boxSizing: "border-box",
            marginBottom: 8,
            fontSize: 14,
          }}
        />
        <button onClick={handleSetPassword} disabled={!permanentPassword} style={{
          width: "100%",
          padding: "10px",
          background: "linear-gradient(135deg, #2980b9 0%, #2471a3 100%)",
          color: "#fff",
          border: "none",
          borderRadius: 6,
          cursor: !permanentPassword ? "not-allowed" : "pointer",
          fontSize: 12,
          fontWeight: 600,
          textTransform: "uppercase",
          letterSpacing: 1,
          opacity: !permanentPassword ? 0.5 : 1,
        }}>
          Definir Senha
        </button>
      </div>

      {/* Generate OTP */}
      <div>
        <button onClick={handleGenerateOtp} style={{
          width: "100%",
          padding: "10px",
          background: "linear-gradient(135deg, #8e44ad 0%, #7d3c98 100%)",
          color: "#fff",
          border: "none",
          borderRadius: 6,
          cursor: "pointer",
          fontSize: 12,
          fontWeight: 600,
          textTransform: "uppercase",
          letterSpacing: 1,
        }}>
          Gerar Senha Temporária
        </button>
        {oneTimePassword && (
          <div style={{
            marginTop: 8,
            padding: "10px 14px",
            background: "#0a0f0a",
            borderRadius: 6,
            fontFamily: "'Courier New', monospace",
            textAlign: "center",
            letterSpacing: 3,
            fontSize: 18,
            border: "1px solid #8e44ad44",
            color: "#bb8fce",
          }}>
            {oneTimePassword}
          </div>
        )}
      </div>

      {/* Error Display */}
      {ipc.error && (
        <div style={{
          padding: "10px 14px",
          background: "#c0392b22",
          border: "1px solid #c0392b",
          borderRadius: 6,
          fontSize: 12,
          wordBreak: "break-word",
          color: "#e74c3c",
        }}>
          {ipc.error}
        </div>
      )}

      {/* Logs Terminal */}
      <div style={{
        flex: 1,
        minHeight: 200,
        background: "#050805",
        borderRadius: 6,
        border: "1px solid #2d3a2d",
        padding: 12,
        overflowY: "auto",
        fontFamily: "'Courier New', monospace",
        fontSize: 11,
        lineHeight: 1.6,
      }}>
        {logs.map((log, i) => (
          <div key={i} style={{ marginBottom: 4 }}>
            <span style={{ color: "#666" }}>[{log.timestamp}]</span>{" "}
            <span style={{
              color: log.level === "error" ? "#e74c3c" : log.level === "warn" ? "#f39c12" : "#a8c686",
              fontWeight: log.level === "error" ? 600 : 400,
            }}>
              {log.level.toUpperCase()}
            </span>{" "}
            <span style={{ color: "#ccc" }}>{log.message}</span>
          </div>
        ))}
        <div ref={logsEndRef} />
      </div>
    </div>
  );
}