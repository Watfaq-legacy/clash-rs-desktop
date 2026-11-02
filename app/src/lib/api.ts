import { invoke } from "@tauri-apps/api/core";

export type Phase = "disconnected" | "connecting" | "connected" | "disconnecting";
export type RoutingMode = "rule" | "global" | "direct";
export type ConnectMode = "system-proxy" | "tun";

export interface SubscriptionInfo {
  upload: number;
  download: number;
  total: number;
  expire: number | null;
}

export interface Profile {
  id: string;
  name: string;
  remote: boolean;
  active: boolean;
  subscription: SubscriptionInfo | null;
  updatedAt: number | null;
}

export interface View {
  phase: Phase;
  step: string | null;
  error: string | null;
  connectedSince: number | null;
  port: number | null;
  profiles: Profile[];
  routingMode: RoutingMode;
  connectMode: ConnectMode;
  autoConnect: boolean;
}

export interface NodeItem {
  name: string;
  kind: string;
  delay: number | null;
  auto: boolean;
  resolved: string | null;
}

export interface Nodes {
  group: string | null;
  selected: string | null;
  resolved: string | null;
  resolvedDelay: number | null;
  items: NodeItem[];
}

export interface Traffic {
  up: number;
  down: number;
}

export const api = {
  getView: () => invoke<View>("get_view"),
  toggle: () => invoke<View>("toggle_connection"),
  dismissError: () => invoke<View>("dismiss_error"),
  addSubscription: (url: string) => invoke<View>("add_subscription", { url }),
  refreshSubscription: (id: string) => invoke<View>("refresh_subscription", { id }),
  selectProfile: (id: string) => invoke<View>("select_profile", { id }),
  removeProfile: (id: string) => invoke<View>("remove_profile", { id }),
  setRoutingMode: (mode: RoutingMode) => invoke<View>("set_routing_mode", { mode }),
  setAutoConnect: (enabled: boolean) => invoke<View>("set_auto_connect", { enabled }),
  getNodes: () => invoke<Nodes>("get_nodes"),
  selectNode: (group: string, name: string) => invoke<Nodes>("select_node", { group, name }),
  testDelays: (group: string) => invoke<Nodes>("test_delays", { group }),
  coreLogs: () => invoke<string[]>("core_logs"),
  readOverride: () => invoke<string>("read_override"),
  writeOverride: (text: string) => invoke<View>("write_override", { text }),
  quit: () => invoke<void>("quit_app"),
};

/** Tauri rejects with the command's error string. */
export function errorText(error: unknown): string {
  return typeof error === "string" ? error : String((error as Error)?.message ?? error);
}
