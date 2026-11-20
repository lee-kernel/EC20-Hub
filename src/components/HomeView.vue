<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { ArrowDown, ArrowUp, GripVertical, RefreshCw } from "lucide-vue-next";
import type { AdapterPriorityItem, DashboardPayload } from "../types";

defineProps<{
  dashboard: DashboardPayload;
}>();

const emit = defineEmits<{
  (e: "refresh", done?: (ok: boolean, msg: string) => void): void;
}>();

const actionTip = ref<string | null>(null);
const busyAction = ref<"" | "refresh" | "reboot" | "route" | "usbnet">("");
const adapters = ref<AdapterPriorityItem[]>([]);
const priorityDirty = ref(false);
const priorityAction = ref<"" | "load" | "apply">("");
const dragIndex = ref<number | null>(null);

async function loadPriorities() {
  priorityAction.value = "load";
  try {
    adapters.value = await invoke<AdapterPriorityItem[]>("list_adapter_priorities");
    priorityDirty.value = false;
  } catch (e) {
    showTip(String(e));
  } finally {
    priorityAction.value = "";
  }
}

function moveAdapter(from: number, to: number) {
  if (to < 0 || to >= adapters.value.length || from === to) return;
  const list = [...adapters.value];
  const [item] = list.splice(from, 1);
  list.splice(to, 0, item);
  adapters.value = list;
  priorityDirty.value = true;
}

function onDragStart(index: number) {
  dragIndex.value = index;
}

function onDrop(targetIndex: number) {
  if (dragIndex.value !== null && dragIndex.value !== targetIndex) {
    moveAdapter(dragIndex.value, targetIndex);
  }
  dragIndex.value = null;
}

async function applyPriorities() {
  if (adapters.value.length === 0) return;
  priorityAction.value = "apply";
  try {
    const res = await invoke<string>("apply_adapter_priorities", {
      ifIndices: adapters.value.map((a) => a.ifIndex),
    });
    showTip(res);
    await loadPriorities();
  } catch (e) {
    showTip(String(e));
  } finally {
    priorityAction.value = "";
  }
}

onMounted(loadPriorities);

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  return `${(mb / 1024).toFixed(2)} GB`;
}

function showTip(msg: string) {
  actionTip.value = msg;
  setTimeout(() => {
    if (actionTip.value === msg) actionTip.value = null;
  }, 4500);
}

function handleRefresh() {
  if (busyAction.value) return;
  busyAction.value = "refresh";
  showTip("正在刷新模块状态...");
  emit("refresh", (_ok, msg) => {
    busyAction.value = "";
    showTip(msg);
  });
}

async function handleReboot() {
  busyAction.value = "reboot";
  try {
    const res = await invoke<string>("reboot_modem");
    showTip(res);
  } catch (e) {
    showTip(String(e));
  } finally {
    busyAction.value = "";
  }
}

async function handleCheckRoute() {
  busyAction.value = "route";
  try {
    const res = await invoke<string>("check_4g_route");
    showTip(res);
  } catch (e) {
    showTip(String(e));
  } finally {
    busyAction.value = "";
  }
}

function formatUsbnetMode(mode: number | null): string {
  switch (mode) {
    case 0:
      return "RMNET / QMI (0)";
    case 1:
      return "ECM (1)";
    case 2:
      return "MBIM (2 · Win 免驱蜂窝)";
    case 3:
      return "RNDIS (3)";
    default:
      return "--";
  }
}

async function handleSetUsbnet(mode: number) {
  if (busyAction.value) return;
  busyAction.value = "usbnet";
  try {
    const res = await invoke<string>("set_usbnet_mode", { mode });
    showTip(res);
  } catch (e) {
    showTip(String(e));
  } finally {
    busyAction.value = "";
  }
}
</script>

<template>
  <div class="home-wrap">
    <!-- 顶部状态横幅 -->
    <div class="card banner">
      <div class="banner-left">
        <div class="operator-name">
          {{ dashboard.status.connected ? dashboard.status.operator : "模块未连接" }}
        </div>
        <div class="sub-status">
          <template v-if="dashboard.status.connected">
            {{ dashboard.status.regStatus || "已连接" }}
            <span v-if="dashboard.status.lastUpdated"> · 更新于 {{ dashboard.status.lastUpdated }}</span>
          </template>
          <template v-else>
            {{ dashboard.status.error || "正在扫描移远 AT 串口 (VID 2C7C)..." }}
          </template>
        </div>
      </div>

      <div class="banner-right">
        <div class="signal-bars" :title="`信号强度: ${dashboard.status.signalDbm ?? '--'} dBm`">
          <span
            v-for="bar in 4"
            :key="bar"
            class="bar"
            :class="{ active: dashboard.status.connected && bar <= dashboard.status.signalBars }"
            :style="{ height: `${6 + bar * 3}px` }"
          />
          <span class="dbm-text">
            {{ dashboard.status.signalDbm !== null ? `${dashboard.status.signalDbm} dBm` : "-- dBm" }}
          </span>
        </div>

        <span class="pill pill-blue">{{ dashboard.status.rat || "LTE" }}</span>

        <span class="sim-indicator">
          <span class="dot" :class="dashboard.status.simInserted ? 'dot-green' : 'dot-gray'" />
          {{ dashboard.status.simInserted ? "SIM 已插入" : "无 SIM 卡" }}
        </span>

        <button
          class="btn"
          :disabled="busyAction !== ''"
          title="立即刷新"
          @click="handleRefresh"
        >
          <RefreshCw :size="13" :class="{ spin: busyAction === 'refresh' }" />
        </button>

        <button
          class="btn"
          :disabled="!dashboard.status.connected || busyAction !== ''"
          @click="handleReboot"
        >
          {{ busyAction === "reboot" ? "重启中..." : "重启模块" }}
        </button>
      </div>
    </div>

    <div v-if="actionTip" class="tip-bar">{{ actionTip }}</div>

    <!-- 设备信息卡片 -->
    <div class="card">
      <div class="card-title">设备信息</div>
      <div class="info-grid">
        <div class="info-row">
          <span class="info-label">IMEI</span>
          <span class="info-value selectable">{{ dashboard.status.imei || "--" }}</span>
        </div>
        <div class="info-row">
          <span class="info-label">固件</span>
          <span class="info-value selectable">{{ dashboard.status.firmware || "--" }}</span>
        </div>
        <div class="info-row">
          <span class="info-label">ICCID</span>
          <span class="info-value selectable">{{ dashboard.status.iccid || "--" }}</span>
        </div>
        <div class="info-row">
          <span class="info-label">IMSI</span>
          <span class="info-value selectable">{{ dashboard.status.imsi || "--" }}</span>
        </div>
        <div class="info-row">
          <span class="info-label">AT 端口</span>
          <span class="info-value">{{ dashboard.status.portName || "自动检测中" }}</span>
        </div>
        <div class="info-row">
          <span class="info-label">网卡模式</span>
          <span class="info-value usbnet-cell">
            <span>{{ formatUsbnetMode(dashboard.status.usbnetMode) }}</span>
            <button
              v-if="dashboard.status.connected && dashboard.status.usbnetMode !== null && dashboard.status.usbnetMode !== 2"
              class="btn btn-xs btn-primary"
              :disabled="busyAction !== ''"
              title="切换为 MBIM (2) 以匹配 Windows 自带蜂窝网络驱动，避免 Code 43"
              @click="handleSetUsbnet(2)"
            >
              {{ busyAction === "usbnet" ? "切换中..." : "对齐 MBIM" }}
            </button>
            <button
              v-else-if="dashboard.status.connected && dashboard.status.usbnetMode === 2"
              class="btn btn-xs"
              :disabled="busyAction !== ''"
              title="切回移远 RMNET (0) 模式（需安装移远 NDIS 驱动）"
              @click="handleSetUsbnet(0)"
            >
              切回 RMNET
            </button>
          </span>
        </div>
      </div>
    </div>

    <!-- 网络与流量 -->
    <div class="card">
      <div class="card-header">
        <span class="card-title">网络与流量</span>
        <div class="header-actions">
          <button class="btn" :disabled="busyAction !== ''" @click="handleCheckRoute">
            {{ busyAction === "route" ? "检查中..." : "检查 4G 出口" }}
          </button>
        </div>
      </div>
      <div class="adapter-name">{{ dashboard.network.adapterName }}</div>
      <div class="traffic-grid">
        <div class="traffic-item">
          <div class="t-label">本次下载</div>
          <div class="t-val">{{ formatBytes(dashboard.network.sessionRxBytes) }}</div>
        </div>
        <div class="traffic-item">
          <div class="t-label">本次上传</div>
          <div class="t-val">{{ formatBytes(dashboard.network.sessionTxBytes) }}</div>
        </div>
        <div class="traffic-item">
          <div class="t-label">本次总流量</div>
          <div class="t-val">
            {{ formatBytes(dashboard.network.sessionRxBytes + dashboard.network.sessionTxBytes) }}
          </div>
        </div>
        <div class="traffic-item">
          <div class="t-label">网卡累计</div>
          <div class="t-val">
            {{ formatBytes(dashboard.network.totalRxBytes + dashboard.network.totalTxBytes) }}
          </div>
        </div>
      </div>
    </div>

    <!-- 网卡优先级 -->
    <div class="card">
      <div class="card-header">
        <span class="card-title">网卡优先级</span>
        <div class="header-actions">
          <button
            class="btn"
            :class="{ 'btn-primary': priorityDirty }"
            :disabled="!priorityDirty || priorityAction !== ''"
            @click="applyPriorities"
          >
            {{ priorityAction === "apply" ? "应用中..." : "应用顺序" }}
          </button>
          <button
            class="btn"
            :disabled="priorityAction !== ''"
            title="刷新网卡顺序"
            @click="loadPriorities"
          >
            <RefreshCw :size="13" :class="{ spin: priorityAction === 'load' }" />
          </button>
        </div>
      </div>

      <div v-if="adapters.length === 0" class="hint-text">
        {{ priorityAction === "load" ? "正在读取网卡列表..." : "未检测到可用网络适配器" }}
      </div>

      <div v-else class="prio-list">
        <div
          v-for="(item, idx) in adapters"
          :key="item.ifIndex"
          class="prio-row"
          :class="{ dragging: dragIndex === idx }"
          draggable="true"
          @dragstart="onDragStart(idx)"
          @dragover.prevent
          @drop="onDrop(idx)"
          @dragend="dragIndex = null"
        >
          <GripVertical :size="14" class="prio-grip" />
          <span class="prio-idx">{{ idx + 1 }}</span>
          <span class="prio-name" :title="item.description">{{ item.name }}</span>
          <span
            v-if="item.tag"
            class="pill"
            :class="item.isModem ? 'pill-green' : 'pill-blue'"
          >
            {{ item.tag }}
          </span>
          <span v-if="item.isDefaultRoute" class="pill pill-gray">当前出口</span>
          <span class="prio-metric">跃点 {{ item.metric }}</span>
          <div class="prio-arrows">
            <button
              class="icon-btn"
              :disabled="idx === 0"
              title="上移"
              @click="moveAdapter(idx, idx - 1)"
            >
              <ArrowUp :size="12" />
            </button>
            <button
              class="icon-btn"
              :disabled="idx === adapters.length - 1"
              title="下移"
              @click="moveAdapter(idx, idx + 1)"
            >
              <ArrowDown :size="12" />
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.home-wrap {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.home-wrap .card {
  padding: 11px 15px;
}

.banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  background: linear-gradient(90deg, var(--bg-banner) 0%, var(--bg-card) 55%);
  padding: 11px 16px;
}

.operator-name {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
}

.sub-status {
  font-size: 12px;
  color: var(--text-secondary);
  margin-top: 2px;
}

.banner-right {
  display: flex;
  align-items: center;
  gap: 12px;
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
  margin-left: 6px;
  font-size: 12px;
  font-weight: 500;
  color: var(--text-primary);
}

.sim-indicator {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 12px;
  color: var(--green);
  font-weight: 500;
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

.tip-bar {
  background: var(--tip-bg);
  border: 1px solid var(--tip-border);
  color: var(--tip-text);
  border-radius: 8px;
  padding: 8px 12px;
  font-size: 12px;
}

.card-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 7px;
}

.card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 7px;
}

.card-header .card-title {
  margin-bottom: 0;
}

.header-actions {
  display: flex;
  gap: 8px;
}

.info-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px 28px;
}

.info-row {
  display: flex;
  align-items: center;
  gap: 14px;
  font-size: 12px;
}

.info-label {
  width: 64px;
  color: var(--text-secondary);
  flex-shrink: 0;
}

.info-value {
  color: var(--text-primary);
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.selectable {
  user-select: text;
}

.usbnet-cell {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.btn-xs {
  padding: 2px 7px;
  font-size: 11px;
  border-radius: 5px;
}

.adapter-name {
  font-size: 11.5px;
  color: var(--text-muted);
  margin-bottom: 8px;
}

.traffic-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 10px;
}

.traffic-item {
  background: var(--bg-subtle);
  border: 1px solid var(--border-soft);
  border-radius: 8px;
  padding: 8px 12px;
}

.t-label {
  font-size: 11px;
  color: var(--text-secondary);
}

.t-val {
  font-size: 15px;
  font-weight: 700;
  margin-top: 3px;
  color: var(--text-primary);
}

.hint-text {
  margin-top: 6px;
  font-size: 11.5px;
  line-height: 1.4;
  color: var(--text-secondary);
}

.call-active-box {
  margin-top: 8px;
  padding: 8px 10px;
  border-radius: 8px;
  background: var(--green-bg);
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.caller-info {
  display: flex;
  align-items: center;
  gap: 6px;
  font-weight: 600;
  color: var(--green);
}

.call-btns {
  display: flex;
  gap: 6px;
}

.dial-row {
  margin-top: 8px;
  display: flex;
  gap: 6px;
}

.dial-input {
  flex: 1;
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 5px 8px;
  font-size: 12px;
  outline: none;
}

.dial-input:focus {
  border-color: var(--accent);
}

.prio-list {
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.prio-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 8px;
  border-radius: 6px;
  background: var(--bg-subtle);
  border: 1px solid transparent;
  cursor: grab;
  font-size: 12px;
}

.prio-row:hover {
  background: var(--bg-hover);
}

.prio-row.dragging {
  opacity: 0.5;
  border-color: var(--accent);
}

.prio-grip {
  color: var(--text-muted);
  flex-shrink: 0;
}

.prio-idx {
  width: 16px;
  text-align: center;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 500;
}

.prio-name {
  font-weight: 500;
  color: var(--text-primary);
}

.prio-metric {
  margin-left: auto;
  font-size: 11.5px;
  color: var(--text-muted);
}

.prio-arrows {
  display: flex;
  gap: 2px;
}

.icon-btn {
  border: none;
  background: transparent;
  color: var(--text-muted);
  padding: 2px;
  cursor: pointer;
  display: flex;
  align-items: center;
}

.icon-btn:hover:not(:disabled) {
  color: var(--accent);
}

.icon-btn:disabled {
  opacity: 0.3;
  cursor: default;
}

.pill-green {
  background: var(--green-bg);
  color: var(--green);
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
