<script setup lang="ts">
import { computed, onActivated, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Copy, RefreshCw, Send, Trash2, X } from "lucide-vue-next";
import type { SmsMessage } from "../types";

const props = defineProps<{
  messages: SmsMessage[];
  connected: boolean;
}>();

const emit = defineEmits<{
  (e: "reload-messages"): void;
  (e: "refresh", done?: (ok: boolean, msg: string) => void): void;
  (e: "mark-read"): void;
}>();

const search = ref("");
const selectedSender = ref<string | null>(null);
const newRecipient = ref("");
const newText = ref("");
const sending = ref(false);
const syncing = ref(false);
const clearingModem = ref(false);
const confirmAction = ref<"modem" | "local" | null>(null);
const statusTip = ref<string | null>(null);
let confirmTimer: ReturnType<typeof setTimeout> | null = null;

interface Conversation {
  sender: string;
  latest: SmsMessage;
  count: number;
  unread: number;
}

const conversations = computed<Conversation[]>(() => {
  const map = new Map<string, SmsMessage[]>();
  const q = search.value.trim().toLowerCase();
  for (const m of props.messages) {
    if (q && !m.sender.toLowerCase().includes(q) && !m.text.toLowerCase().includes(q)) {
      continue;
    }
    const list = map.get(m.sender) ?? [];
    list.push(m);
    map.set(m.sender, list);
  }
  const out: Conversation[] = [];
  for (const [sender, list] of map.entries()) {
    const unread = list.filter((m) => !m.read).length;
    out.push({ sender, latest: list[0], count: list.length, unread });
  }
  return out;
});

const activeMessages = computed(() => {
  const q = search.value.trim().toLowerCase();
  return props.messages.filter((m) => {
    if (selectedSender.value && m.sender !== selectedSender.value) return false;
    if (q && !m.sender.toLowerCase().includes(q) && !m.text.toLowerCase().includes(q)) {
      return false;
    }
    return true;
  });
});

const effectiveRecipient = computed(() =>
  (newRecipient.value || selectedSender.value || "").trim()
);

const canSend = computed(
  () =>
    props.connected &&
    !sending.value &&
    effectiveRecipient.value.length > 0 &&
    newText.value.trim().length > 0 &&
    newText.value.length <= 70
);

async function markAllRead() {
  await invoke("mark_all_read");
  emit("mark-read");
}

let unlistenNavigateSms: UnlistenFn | null = null;

onMounted(async () => {
  await markAllRead();
  unlistenNavigateSms = await listen("navigate-sms", async () => {
    selectedSender.value = null;
    search.value = "";
    await markAllRead();
  });
});
onActivated(markAllRead);
onUnmounted(() => {
  unlistenNavigateSms?.();
});

function showTip(msg: string) {
  statusTip.value = msg;
  setTimeout(() => {
    if (statusTip.value === msg) statusTip.value = null;
  }, 3000);
}

function selectConversation(sender: string | null) {
  selectedSender.value = sender;
  newRecipient.value = sender ?? "";
}

async function copyText(text: string, label: string) {
  await navigator.clipboard.writeText(text);
  showTip(label);
}

async function handleSync() {
  if (syncing.value) return;
  syncing.value = true;
  showTip("正在同步模块短信...");
  emit("refresh", (_ok, msg) => {
    syncing.value = false;
    showTip(msg);
  });
}

async function handleSend() {
  if (!canSend.value) return;
  const target = effectiveRecipient.value;
  const body = newText.value.trim();
  sending.value = true;
  try {
    await invoke("send_sms", { recipient: target, text: body });
    newText.value = "";
    selectedSender.value = target;
    newRecipient.value = target;
    emit("reload-messages");
    showTip(`短信已发送至 ${target}`);
  } catch (e) {
    showTip(`发送失败: ${e}`);
  } finally {
    sending.value = false;
  }
}

function handleComposeKeydown(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    handleSend();
  }
}

async function handleDelete(id: string) {
  await invoke("delete_message", { id });
  emit("reload-messages");
}

function requestConfirm(kind: "modem" | "local"): boolean {
  if (confirmAction.value === kind) {
    confirmAction.value = null;
    if (confirmTimer) clearTimeout(confirmTimer);
    return true;
  }
  confirmAction.value = kind;
  if (confirmTimer) clearTimeout(confirmTimer);
  confirmTimer = setTimeout(() => {
    if (confirmAction.value === kind) confirmAction.value = null;
  }, 3000);
  return false;
}

async function handleClearModem() {
  if (!requestConfirm("modem")) return;
  clearingModem.value = true;
  try {
    const res = await invoke<string>("clear_modem_storage");
    showTip(res);
    emit("refresh");
  } catch (e) {
    showTip(String(e));
  } finally {
    clearingModem.value = false;
  }
}

async function handleClearLocal() {
  if (!requestConfirm("local")) return;
  await invoke("clear_messages");
  selectedSender.value = null;
  emit("reload-messages");
  showTip("已清空本机归档");
}
</script>

<template>
  <div class="sms-page">
    <div class="sms-toolbar card">
      <div class="search-wrap">
        <input v-model="search" class="search-input" placeholder="搜索号码或短信内容..." />
        <button
          v-if="search"
          class="search-clear"
          title="清除搜索"
          @click="search = ''"
        >
          <X :size="12" />
        </button>
      </div>
      <div class="toolbar-actions">
        <button class="btn" :disabled="syncing" @click="handleSync">
          <RefreshCw :size="13" :class="{ spin: syncing }" />
          同步模块
        </button>
        <button
          class="btn"
          :class="{ 'btn-warn': confirmAction === 'modem' }"
          :disabled="!connected || clearingModem"
          @click="handleClearModem"
        >
          {{
            clearingModem
              ? "清理中..."
              : confirmAction === "modem"
                ? "确认清理模块存储？"
                : "清理 SIM/模块存储"
          }}
        </button>
        <button
          class="btn btn-danger"
          :disabled="messages.length === 0"
          @click="handleClearLocal"
        >
          {{ confirmAction === "local" ? "确认清空本机记录？" : "清空本机记录" }}
        </button>
      </div>
    </div>

    <div v-if="statusTip" class="tip-bar">{{ statusTip }}</div>

    <div class="sms-body">
      <!-- 左侧会话列表 -->
      <div class="card conv-list">
        <div
          class="conv-item"
          :class="{ active: selectedSender === null }"
          @click="selectConversation(null)"
        >
          <div class="conv-top">
            <span class="conv-sender">全部短信</span>
            <span class="pill pill-gray">{{ messages.length }}</span>
          </div>
          <div class="conv-preview">查看所有收发记录</div>
        </div>

        <div
          v-for="conv in conversations"
          :key="conv.sender"
          class="conv-item"
          :class="{ active: selectedSender === conv.sender }"
          @click="selectConversation(conv.sender)"
        >
          <div class="conv-top">
            <span class="conv-sender">{{ conv.sender }}</span>
            <span class="pill pill-gray">{{ conv.count }}</span>
          </div>
          <div class="conv-preview">{{ conv.latest.text }}</div>
        </div>

        <div v-if="conversations.length === 0 && search.trim()" class="conv-empty">
          无匹配会话
        </div>
      </div>

      <!-- 右侧消息流 + 发送框 -->
      <div class="card msg-panel">
        <div class="msg-header">
          <span class="msg-header-title">
            {{ selectedSender ? `会话 · ${selectedSender}` : "全部短信" }}
          </span>
          <span class="pill pill-gray">{{ activeMessages.length }} 条</span>
          <div class="msg-header-actions">
            <button
              v-if="selectedSender"
              class="btn btn-sm"
              @click="copyText(selectedSender, `已复制号码: ${selectedSender}`)"
            >
              <Copy :size="11" />
              复制号码
            </button>
            <button
              v-if="selectedSender"
              class="btn btn-sm"
              @click="selectConversation(null)"
            >
              查看全部
            </button>
          </div>
        </div>

        <div class="msg-list">
          <div v-if="activeMessages.length === 0" class="empty-tip">
            {{ search.trim() ? "没有找到匹配的短信。" : "暂无短信记录，收到新短信会自动显示在这里。" }}
          </div>

          <div
            v-for="msg in activeMessages"
            :key="msg.id"
            class="msg-card"
            :class="msg.direction === 'out' ? 'msg-out' : 'msg-in'"
          >
            <div class="msg-meta">
              <span class="msg-sender">
                {{ msg.direction === "out" ? `发送给 ${msg.sender}` : msg.sender }}
              </span>
              <span class="msg-time">{{ msg.timestamp }}</span>
              <button
                class="icon-btn"
                title="复制短信内容"
                @click="copyText(msg.text, '已复制短信内容')"
              >
                <Copy :size="12" />
              </button>
              <button class="icon-btn icon-del" title="删除" @click="handleDelete(msg.id)">
                <Trash2 :size="12" />
              </button>
            </div>
            <div class="msg-text">{{ msg.text }}</div>
            <div v-if="msg.otpCode" class="otp-row">
              <button
                class="pill pill-blue otp-btn"
                @click="copyText(msg.otpCode!, `已复制验证码: ${msg.otpCode}`)"
              >
                <Copy :size="11" />
                复制验证码 {{ msg.otpCode }}
              </button>
            </div>
          </div>
        </div>

        <div class="compose-box">
          <div class="compose-row">
            <input
              v-model="newRecipient"
              class="recipient-input"
              :placeholder="selectedSender ? `回复 ${selectedSender}` : '收件人号码 (如 10001)'"
            />
            <span class="char-counter" :class="{ over: newText.length > 70 }">
              {{ newText.length }}/70
            </span>
          </div>
          <div class="compose-row">
            <textarea
              v-model="newText"
              rows="2"
              class="text-input"
              placeholder="输入短信内容（Enter 发送，Shift+Enter 换行，单条最多 70 字）..."
              @keydown="handleComposeKeydown"
            />
            <button
              class="btn btn-primary send-btn"
              :disabled="!canSend"
              @click="handleSend"
            >
              <Send :size="13" />
              {{ sending ? "发送中..." : "发送" }}
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.sms-page {
  display: flex;
  flex-direction: column;
  gap: 10px;
  height: 100%;
}

.sms-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 10px 14px;
}

.search-wrap {
  position: relative;
  display: flex;
  align-items: center;
}

.search-input {
  width: 240px;
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 5px 26px 5px 10px;
  font-size: 12px;
  background: var(--bg-card);
  color: var(--text-primary);
  outline: none;
}

.search-input:focus {
  border-color: var(--accent);
}

.search-clear {
  position: absolute;
  right: 6px;
  border: none;
  background: transparent;
  color: var(--text-muted);
  display: flex;
  align-items: center;
  padding: 2px;
  cursor: pointer;
}

.search-clear:hover {
  color: var(--text-primary);
}

.toolbar-actions {
  display: flex;
  gap: 8px;
}

.btn-warn {
  border-color: #f59e0b;
  background: #fef3c7;
  color: #92400e;
}

.btn-sm {
  padding: 3px 8px;
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

.tip-bar {
  background: var(--tip-bg);
  border: 1px solid var(--tip-border);
  color: var(--tip-text);
  border-radius: 8px;
  padding: 7px 12px;
  font-size: 12px;
}

.sms-body {
  display: grid;
  grid-template-columns: 240px 1fr;
  gap: 10px;
  flex: 1;
  min-height: 0;
}

.conv-list {
  padding: 8px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.conv-item {
  padding: 8px 10px;
  border-radius: 8px;
  cursor: pointer;
}

.conv-item:hover {
  background: var(--bg-hover);
}

.conv-item.active {
  background: var(--blue-bg);
}

.conv-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
}

.conv-sender {
  font-weight: 600;
  font-size: 12.5px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.conv-preview {
  margin-top: 3px;
  font-size: 11.5px;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.conv-empty {
  text-align: center;
  color: var(--text-muted);
  font-size: 12px;
  padding: 20px 0;
}

.msg-panel {
  display: flex;
  flex-direction: column;
  min-height: 0;
  padding: 12px;
}

.msg-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-bottom: 8px;
  margin-bottom: 8px;
  border-bottom: 1px solid var(--border);
}

.msg-header-title {
  font-size: 13px;
  font-weight: 600;
}

.msg-header-actions {
  margin-left: auto;
  display: flex;
  gap: 6px;
}

.msg-list {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding-right: 4px;
}

.empty-tip {
  text-align: center;
  color: var(--text-muted);
  margin-top: 60px;
}

.msg-card {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 10px 12px;
  background: var(--bg-subtle);
}

.msg-out {
  background: var(--bg-banner);
  border-color: var(--tip-border);
}

.msg-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11px;
  color: var(--text-secondary);
  margin-bottom: 5px;
}

.msg-sender {
  font-weight: 600;
  color: var(--text-primary);
}

.msg-time {
  margin-left: auto;
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

.icon-btn:hover {
  color: var(--accent);
}

.icon-del:hover {
  color: var(--red);
}

.msg-text {
  font-size: 13px;
  line-height: 1.5;
  user-select: text;
  white-space: pre-wrap;
  word-break: break-word;
}

.otp-row {
  margin-top: 6px;
}

.otp-btn {
  border: none;
  cursor: pointer;
}

.compose-box {
  margin-top: 10px;
  padding-top: 10px;
  border-top: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.compose-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.recipient-input {
  width: 200px;
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 5px 8px;
  font-size: 12px;
  background: var(--bg-card);
  color: var(--text-primary);
  outline: none;
}

.char-counter {
  margin-left: auto;
  font-size: 11.5px;
  color: var(--text-muted);
}

.char-counter.over {
  color: var(--red);
  font-weight: 600;
}

.text-input {
  flex: 1;
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 6px 10px;
  font-size: 12px;
  font-family: inherit;
  background: var(--bg-card);
  color: var(--text-primary);
  resize: none;
  line-height: 1.4;
  outline: none;
}

.recipient-input:focus,
.text-input:focus {
  border-color: var(--accent);
}

.send-btn {
  align-self: stretch;
  padding: 0 14px;
}
</style>
