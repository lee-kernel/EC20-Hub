<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { Monitor, Moon, Sun } from "lucide-vue-next";
import type { AppSettings, ThemeMode } from "../types";

const props = defineProps<{
  settings: AppSettings;
}>();

const emit = defineEmits<{
  (e: "update-settings", settings: AppSettings): void;
}>();

const autoStart = ref(false);
const autoStartBusy = ref(false);
const tip = ref<string | null>(null);

const themeOptions: { value: ThemeMode; label: string; icon: typeof Sun }[] = [
  { value: "light", label: "浅色", icon: Sun },
  { value: "dark", label: "深色", icon: Moon },
  { value: "system", label: "跟随系统", icon: Monitor },
];

function showTip(msg: string) {
  tip.value = msg;
  setTimeout(() => {
    if (tip.value === msg) tip.value = null;
  }, 3500);
}

function setTheme(theme: ThemeMode) {
  emit("update-settings", {
    ...props.settings,
    theme,
  });
}

function toggleBool(key: keyof Pick<AppSettings, "takeoverSms" | "notifySms" | "startMinimized">) {
  emit("update-settings", {
    ...props.settings,
    [key]: !props.settings[key],
  });
}

async function handleToggleAutoStart() {
  if (autoStartBusy.value) return;
  autoStartBusy.value = true;
  const target = !autoStart.value;
  try {
    autoStart.value = await invoke<boolean>("set_autostart_enabled", { enable: target });
    showTip(autoStart.value ? "已开启开机自启动（计划任务最高权限）" : "已关闭开机自启动");
  } catch (e) {
    showTip(String(e));
  } finally {
    autoStartBusy.value = false;
  }
}

onMounted(async () => {
  try {
    autoStart.value = await invoke<boolean>("get_autostart_enabled");
  } catch {
    autoStart.value = false;
  }
});
</script>

<template>
  <div class="settings-wrap">
    <div v-if="tip" class="tip-bar">{{ tip }}</div>

    <!-- 外观与启动 -->
    <div class="card">
      <div class="card-title">外观与启动</div>

      <div class="setting-row">
        <div class="setting-info">
          <div class="setting-name">界面主题</div>
          <div class="setting-desc">切换浅色、深色或跟随 Windows 系统外观</div>
        </div>
        <div class="seg-group">
          <button
            v-for="opt in themeOptions"
            :key="opt.value"
            class="seg-btn"
            :class="{ active: settings.theme === opt.value }"
            @click="setTheme(opt.value)"
          >
            <component :is="opt.icon" :size="13" />
            <span>{{ opt.label }}</span>
          </button>
        </div>
      </div>

      <div class="divider" />

      <div class="setting-row">
        <div class="setting-info">
          <div class="setting-name">开机自启动</div>
          <div class="setting-desc">
            登录 Windows 时通过计划任务以管理员权限自动启动，并静默驻留托盘
          </div>
        </div>
        <label class="switch">
          <input
            type="checkbox"
            :checked="autoStart"
            :disabled="autoStartBusy"
            @change="handleToggleAutoStart"
          />
          <span class="slider" />
        </label>
      </div>

      <div class="divider" />

      <div class="setting-row">
        <div class="setting-info">
          <div class="setting-name">启动时最小化到托盘</div>
          <div class="setting-desc">手动打开软件时默认隐藏主窗口，在系统托盘后台运行</div>
        </div>
        <label class="switch">
          <input
            type="checkbox"
            :checked="settings.startMinimized"
            @change="toggleBool('startMinimized')"
          />
          <span class="slider" />
        </label>
      </div>
    </div>

    <!-- 短信与通知 -->
    <div class="card">
      <div class="card-title">短信与通知</div>

      <div class="setting-row">
        <div class="setting-info">
          <div class="setting-name">接管模块短信</div>
          <div class="setting-desc">自动将新短信归档至本机，并清理 SIM 卡与模块内部存储</div>
        </div>
        <label class="switch">
          <input
            type="checkbox"
            :checked="settings.takeoverSms"
            @change="toggleBool('takeoverSms')"
          />
          <span class="slider" />
        </label>
      </div>

      <div class="divider" />

      <div class="setting-row">
        <div class="setting-info">
          <div class="setting-name">新短信通知</div>
          <div class="setting-desc">收到新短信或验证码时弹出 Windows 系统通知提醒</div>
        </div>
        <label class="switch">
          <input
            type="checkbox"
            :checked="settings.notifySms"
            @change="toggleBool('notifySms')"
          />
          <span class="slider" />
        </label>
      </div>
    </div>
  </div>
</template>

<style scoped>
.settings-wrap {
  display: flex;
  flex-direction: column;
  gap: 12px;
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
  margin-bottom: 10px;
}

.setting-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 6px 0;
}

.setting-info {
  min-width: 0;
}

.setting-name {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
}

.setting-desc {
  margin-top: 2px;
  font-size: 11.5px;
  color: var(--text-secondary);
}

.divider {
  height: 1px;
  background: var(--border-soft);
  margin: 6px 0;
}

.seg-group {
  display: inline-flex;
  background: var(--bg-subtle);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 2px;
  gap: 2px;
  flex-shrink: 0;
}

.seg-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 4px 10px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 500;
  transition: background 0.15s, color 0.15s, transform 0.08s ease;
}

.seg-btn:hover {
  color: var(--text-primary);
}

.seg-btn:active {
  transform: scale(0.96);
}

.seg-btn.active {
  background: var(--bg-card);
  color: var(--accent);
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.08);
}
</style>
