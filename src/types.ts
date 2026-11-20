export interface ModemStatus {
  connected: boolean;
  portName: string;
  operator: string;
  regStatus: string;
  signalDbm: number | null;
  signalBars: number;
  rat: string;
  simInserted: boolean;
  imei: string;
  firmware: string;
  iccid: string;
  imsi: string;
  usbnetMode: number | null;
  simSmsUsed: number;
  simSmsTotal: number;
  meSmsUsed: number;
  meSmsTotal: number;
  lastUpdated: string;
  error: string | null;
}

export interface NetworkStats {
  adapterName: string;
  adapterUp: boolean;
  sessionRxBytes: number;
  sessionTxBytes: number;
  totalRxBytes: number;
  totalTxBytes: number;
}

export type ThemeMode = "light" | "dark" | "system";

export interface AppSettings {
  takeoverSms: boolean;
  notifySms: boolean;
  preferredPort: string | null;
  theme: ThemeMode;
  startMinimized: boolean;
}

export interface DashboardPayload {
  status: ModemStatus;
  network: NetworkStats;
  settings: AppSettings;
  unreadCount: number;
}

export interface SmsMessage {
  id: string;
  sender: string;
  text: string;
  timestamp: string;
  direction: "in" | "out";
  otpCode: string | null;
  read: boolean;
}

export interface AdapterPriorityItem {
  ifIndex: number;
  name: string;
  description: string;
  status: string;
  metric: number;
  isModem: boolean;
  isDefaultRoute: boolean;
  tag: string;
}
