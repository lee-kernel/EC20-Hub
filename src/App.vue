<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Home, Info, MessageSquare, Settings } from "lucide-vue-next";
import HomeView from "./components/HomeView.vue";
import SmsView from "./components/SmsView.vue";
import SettingsView from "./components/SettingsView.vue";
import AboutView from "./components/AboutView.vue";
import { applyTheme } from "./theme";
import type { AppSettings, DashboardPayload, SmsMessage } from "./types";

type Tab = "home" | "sms" | "settings" | "about";
const activeTab = ref<Tab>("home");

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

const messages = ref<SmsMessage[]>([]);
let unlistenStatus: UnlistenFn | null = null;
let unlistenSms: UnlistenFn | null = null;
let unlistenNavigateSms: UnlistenFn | null = null;

async function loadDashboard() {
  dashboard.value = await invoke<DashboardPayload>("get_dashboard_state");
  applyTheme(dashboard.value.settings.theme);
}

async function loadMessages() {
  messages.value = await invoke<SmsMessage[]>("get_messages");
}

async function refreshNow(done?: (ok: boolean, msg: string) => void) {
  try {
    dashboard.value = await invoke<DashboardPayload>("refresh_now");
    applyTheme(dashboard.value.settings.theme);
    await loadMessages();
    if (dashboard.value.status.connected) {
      done?.(true, `已同步模块 (${dashboard.value.status.portName})`);
    } else {
      done?.(false, dashboard.value.status.error || "模块未连接");
    }
  } catch (e) {
    done?.(false, `刷新失败: ${e}`);
  }
}

async function updateSettings(next: AppSettings) {
  dashboard.value.settings = await invoke<AppSettings>("update_settings", { settings: next });
  applyTheme(dashboard.value.settings.theme);
}

function handleSmsRead() {
  dashboard.value.unreadCount = 0;
  for (const m of messages.value) {
    m.read = true;
  }
}

onMounted(async () => {
  await loadDashboard();
  await loadMessages();
  unlistenStatus = await listen<DashboardPayload>("status-updated", (event) => {
    dashboard.value = event.payload;
    applyTheme(dashboard.value.settings.theme);
  });
  unlistenSms = await listen("sms-updated", () => {
    loadMessages();
  });
  unlistenNavigateSms = await listen("navigate-sms", async () => {
    activeTab.value = "sms";
    await loadMessages();
  });
});

onUnmounted(() => {
  unlistenStatus?.();
  unlistenSms?.();
  unlistenNavigateSms?.();
});
</script>

<template>
  <div class="shell">
    <!-- 左侧边栏 -->
    <aside class="sidebar">
      <div class="nav-list">
        <button
          class="nav-item"
          :class="{ active: activeTab === 'home' }"
          @click="activeTab = 'home'"
        >
          <Home :size="16" />
          <span>首页</span>
        </button>

        <button
          class="nav-item"
          :class="{ active: activeTab === 'sms' }"
          @click="activeTab = 'sms'"
        >
          <MessageSquare :size="16" />
          <span>短信</span>
          <span v-if="dashboard.unreadCount > 0" class="badge">
            {{ dashboard.unreadCount }}
          </span>
        </button>

        <button
          class="nav-item"
          :class="{ active: activeTab === 'settings' }"
          @click="activeTab = 'settings'"
        >
          <Settings :size="16" />
          <span>配置</span>
        </button>

        <button
          class="nav-item"
          :class="{ active: activeTab === 'about' }"
          @click="activeTab = 'about'"
        >
          <Info :size="16" />
          <span>关于与更新</span>
        </button>
      </div>
    </aside>

    <!-- 主内容区 -->
    <main class="main-area">
      <div class="top-title">EC20 Hub</div>
      <div class="content-scroll">
        <KeepAlive>
          <HomeView
            v-if="activeTab === 'home'"
            :dashboard="dashboard"
            @refresh="refreshNow"
          />
          <SmsView
            v-else-if="activeTab === 'sms'"
            :messages="messages"
            :connected="dashboard.status.connected"
            @reload-messages="loadMessages"
            @refresh="refreshNow"
            @mark-read="handleSmsRead"
          />
          <SettingsView
            v-else-if="activeTab === 'settings'"
            :settings="dashboard.settings"
            @update-settings="updateSettings"
          />
          <AboutView v-else />
        </KeepAlive>
      </div>
    </main>
  </div>
</template>

<style scoped>
.shell {
  display: flex;
  width: 100vw;
  height: 100vh;
  background: var(--bg-app);
}

.sidebar {
  width: 196px;
  background: var(--bg-sidebar);
  border-right: 1px solid var(--border);
  padding: 16px 10px;
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
}

.nav-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 8px 12px;
  border-radius: 8px;
  border: none;
  background: transparent;
  color: var(--text-primary);
  font-size: 13px;
  font-weight: 500;
  text-align: left;
}

.nav-item:hover {
  background: var(--bg-hover);
}

.nav-item.active {
  background: var(--accent);
  color: #ffffff;
}

.badge {
  margin-left: auto;
  background: var(--red);
  color: #ffffff;
  font-size: 10.5px;
  font-weight: 600;
  padding: 1px 6px;
  border-radius: 999px;
}

.nav-item.active .badge {
  background: rgba(255, 255, 255, 0.25);
}

.main-area {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
  padding: 12px 18px;
}

.top-title {
  font-size: 15px;
  font-weight: 700;
  margin-bottom: 8px;
  color: var(--text-primary);
}

.content-scroll {
  flex: 1;
  overflow-y: auto;
  min-height: 0;
}
</style>
