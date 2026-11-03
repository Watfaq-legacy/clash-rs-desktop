import { invoke } from "@tauri-apps/api/core";

export type Phase = "disconnected" | "connecting" | "connected" | "disconnecting";
export type RoutingMode = "rule" | "global" | "direct";
export type ConnectMode = "system-proxy" | "tun";
export type NeedsInstall = "missing" | "outdated";
export type RuleKind = "domain-suffix" | "domain-keyword" | "ip-cidr" | "process-name";
export type RuleAction = "proxy" | "direct" | "reject";
export type Language = "system" | "zh" | "en";

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
  merged: boolean;
}

export interface CustomRule {
  kind: RuleKind;
  value: string;
  action: RuleAction;
}

export interface View {
  phase: Phase;
  step: string | null;
  error: string | null;
  needsInstall: NeedsInstall | null;
  connectedSince: number | null;
  port: number | null;
  profiles: Profile[];
  routingMode: RoutingMode;
  connectMode: ConnectMode;
  connectedMode: ConnectMode | null;
  autoConnect: boolean;
  autoUpdateSubscriptions: boolean;
  customRules: CustomRule[];
  bypass: string[];
  leaks: string[];
  healthIssue: string | null;
  tunIpv6: boolean;
  language: Language;
  english: boolean;
  coreVersion: string | null;
  appVersion: string;
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

export interface CoreUpdateInfo {
  current: string | null;
  latest: string;
  available: boolean;
}

export type FindingLevel = "ok" | "info" | "warn" | "error";
export type Fix =
  | "reconnect"
  | "reapply-system-proxy"
  | "restore-system-proxy"
  | "flush-dns"
  | "enable-tun"
  | "enable-tun-ipv6";

export interface Finding {
  id: string;
  level: FindingLevel;
  title: string;
  detail: string;
  fix: Fix | null;
}

export interface DeepLinkImport {
  url: string;
  name: string | null;
}

export const api = {
  getView: () => invoke<View>("get_view"),
  toggle: () => invoke<View>("toggle_connection"),
  connect: () => invoke<View>("connect"),
  dismissError: () => invoke<View>("dismiss_error"),
  setConnectMode: (mode: ConnectMode) => invoke<View>("set_connect_mode", { mode }),
  installTunSupport: () => invoke<View>("install_tun_support"),
  addSubscription: (url: string, name?: string | null) =>
    invoke<View>("add_subscription", { url, name: name ?? null }),
  refreshSubscription: (id: string) => invoke<View>("refresh_subscription", { id }),
  selectProfile: (id: string) => invoke<View>("select_profile", { id }),
  removeProfile: (id: string) => invoke<View>("remove_profile", { id }),
  setAutoUpdateSubscriptions: (enabled: boolean) =>
    invoke<View>("set_auto_update_subscriptions", { enabled }),
  setLanguage: (language: Language) => invoke<View>("set_language", { language }),
  setRoutingMode: (mode: RoutingMode) => invoke<View>("set_routing_mode", { mode }),
  setAutoConnect: (enabled: boolean) => invoke<View>("set_auto_connect", { enabled }),
  addCustomRule: (kind: RuleKind, value: string, action: RuleAction) =>
    invoke<View>("add_custom_rule", { kind, value, action }),
  removeCustomRule: (index: number) => invoke<View>("remove_custom_rule", { index }),
  checkCoreUpdate: () => invoke<CoreUpdateInfo>("check_core_update"),
  updateCore: () => invoke<View>("update_core"),
  getNodes: () => invoke<Nodes>("get_nodes"),
  selectNode: (group: string, name: string) => invoke<Nodes>("select_node", { group, name }),
  testDelays: (group: string) => invoke<Nodes>("test_delays", { group }),
  coreLogs: () => invoke<string[]>("core_logs"),
  readOverride: () => invoke<string>("read_override"),
  writeOverride: (text: string) => invoke<View>("write_override", { text }),
  takeDeepLink: () => invoke<DeepLinkImport | null>("take_deep_link"),
  importFile: (path: string) => invoke<View>("import_file", { path }),
  setMerged: (id: string, merged: boolean) => invoke<View>("set_merged", { id, merged }),
  setBypass: (entries: string[]) => invoke<View>("set_bypass", { entries }),
  ignoreLeak: (appName: string) => invoke<View>("ignore_leak", { appName }),
  openLogsDir: () => invoke<void>("open_logs_dir"),
  runDiagnostics: () => invoke<Finding[]>("run_diagnostics"),
  applyFix: (fix: Fix) => invoke<View>("apply_fix", { fix }),
  setTunIpv6: (enabled: boolean) => invoke<View>("set_tun_ipv6", { enabled }),
  quit: () => invoke<void>("quit_app"),
};

/** Tauri rejects with the command's error string. */
export function errorText(error: unknown): string {
  return typeof error === "string" ? error : String((error as Error)?.message ?? error);
}
