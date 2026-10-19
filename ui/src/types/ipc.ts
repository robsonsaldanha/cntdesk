export interface InputEvent {
  event_type: "MouseMove" | "MouseDown" | "MouseUp" | "KeyDown" | "KeyUp" | "Scroll";
  timestamp_us: number;
  data: number[];
}

export type ConnectionState = "disconnected" | "connecting" | "connected" | "hosting";

export interface IpcCommands {
  get_device_id: () => Promise<string | null>;
  set_password: (password: string) => Promise<void>;
  generate_one_time_password: () => Promise<string>;
  connect: (remote_id: string, password: string) => Promise<void>;
  disconnect: () => Promise<void>;
  send_input: (event: InputEvent) => Promise<void>;
  get_connection_state: () => Promise<ConnectionState>;
}