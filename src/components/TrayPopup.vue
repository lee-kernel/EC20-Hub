<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { ExternalLink, RefreshCw } from "lucide-vue-next";
import { applyTheme } from "../theme";
import type { DashboardPayload } from "../types";

const dashboard = ref<DashboardPayload>({
  status: {
    connected: false,
    portName: "",
    operator: "",
    regStatus: "",
    signalDbm: null,
    signalBars: 0,
    rat: "LTE",
    simInserted: false,
    imei: "",
    firmware: "",
    iccid: "",
    imsi: "",
    simSmsUsed: 0,
    simSmsTotal: 0,
    meSmsUsed: 0,
    meSmsTotal: 0,
    lastUpdated: "",
    error: null,
  },
  network: {
    adapterName: "检测中...",
    adapterUp: false,
    sessionRxBytes: 0,
    sessionTxBytes: 0,
    totalRxBytes: 0,
    totalTxBytes: 0,
  },
  settings: {
    takeoverSms: true,
    notifySms: true,
    preferredPort: null,
    theme: "light",
    startMinimized: false,
  },
  unreadCount: 0,
});

const refreshing = ref(false);
let unlistenStatus: UnlistenFn | null = null;

async function refreshNow() {
  if (refreshing.value) return;
  refreshing.value = true;
  try {
    dashboard.value = await invoke<DashboardPayload>("refresh_now");
  } finally {
    refreshing.value = false;
  }
}

async function openMain() {
  await invoke("open_main_window");
}

async function openSms() {
  await invoke("open_sms_window");
}

onMounted(async () => {
  dashboard.value = await invoke<DashboardPayload>("get_dashboard_state");
  applyTheme(dashboard.value.settings.theme);
  unlistenStatus = await listen<DashboardPayload>("status-updated", (event) => {
    dashboard.value = event.payload;
    applyTheme(dashboard.value.settings.theme);
  });
});

onUnmounted(() => {
  unlistenStatus?.();
});
</script>

<template>
  <div class="tray-card">
    <div class="top-row">
      <div class="left-info">
        <div class="operator-name">
          {{ dashboard.status.connected ? dashboard.status.operator : "模块未连接" }}
        </div>
        <div class="sub-status">
          <template v-if="dashboard.status.connected">
            {{ dashboard.status.regStatus || "已连接" }}
            <span v-if="dashboard.status.lastUpdated">
              · 更新于 {{ dashboard.status.lastUpdated }}
            </span>
          </template>
          <template v-else>
            {{ dashboard.status.error || "正在扫描移远 AT 串口..." }}
          </template>
        </div>
      </div>

      <div class="right-status">
        <div class="signal-bars" :title="`信号强度: ${dashboard.status.signalDbm ?? '--'} dBm`">
          <span
            v-for="bar in 4"
            :key="bar"
            class="bar"
            :class="{ active: dashboard.status.connected && bar <= dashboard.status.signalBars }"
            :style="{ height: `${5 + bar * 3}px` }"
          />
          <span class="dbm-text">
            {{
              dashboard.status.signalDbm !== null
                ? `${dashboard.status.signalDbm} dBm`
                : "-- dBm"
            }}
          </span>
        </div>
        <span class="pill pill-blue">{{ dashboard.status.rat || "LTE" }}</span>
      </div>
    </div>

    <div class="bottom-row">
      <span class="sim-indicator" :class="{ offline: !dashboard.status.simInserted }">
        <span class="dot" :class="dashboard.status.simInserted ? 'dot-green' : 'dot-gray'" />
        {{ dashboard.status.simInserted ? "SIM 已插入" : "无 SIM 卡" }}
      </span>

      <span
        v-if="dashboard.unreadCount > 0"
        class="pill pill-red clickable"
        title="点击查看新短信"
        @click="openSms"
      >
        未读短信 {{ dashboard.unreadCount }}
      </span>

      <div class="actions">
        <button class="btn btn-sm" :disabled="refreshing" title="立即刷新" @click="refreshNow">
          <RefreshCw :size="12" :class="{ spin: refreshing }" />
        </button>
        <button class="btn btn-sm btn-primary" @click="openMain">
          <ExternalLink :size="12" />
          打开主界面
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tray-card {
  width: 100vw;
  height: 100vh;
  background: linear-gradient(135deg, var(--bg-banner) 0%, var(--bg-card) 60%);
  border: 1px solid var(--border);
  padding: 14px 16px;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
}

.top-row {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 10px;
}

.left-info {
  min-width: 0;
}

.operator-name {
  font-size: 16px;
  font-weight: 700;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.sub-status {
  margin-top: 3px;
  font-size: 11.5px;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.right-status {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
  padding-top: 2px;
}

.signal-bars {
  display: flex;
  align-items: flex-end;
  gap: 2px;
}

.bar {
  width: 3px;
  border-radius: 1px;
  background: #d1d5db;
}

.bar.active {
  background: var(--green);
}

.dbm-text {
  margin-left: 5px;
  font-size: 12px;
  font-weight: 500;
  color: var(--text-primary);
}

.bottom-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-top: 10px;
  border-top: 1px solid var(--border);
}

.sim-indicator {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 12px;
  color: var(--green);
  font-weight: 500;
}

.sim-indicator.offline {
  color: var(--text-muted);
}

.dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
}

.dot-green {
  background: var(--green);
}

.dot-gray {
  background: var(--text-muted);
}

.pill-red {
  background: var(--red-bg);
  color: var(--red);
}

.pill-red.clickable {
  cursor: pointer;
}

.actions {
  margin-left: auto;
  display: flex;
  gap: 6px;
}

.btn-sm {
  padding: 4px 9px;
  font-size: 11.5px;
}

.spin {
  animation: spin 0.7s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
