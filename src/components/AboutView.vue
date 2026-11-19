<template>
  <div class="about-wrap">
    <div class="card hero-card">
      <div class="app-logo" aria-hidden="true">
        <svg viewBox="0 0 512 512" width="44" height="44" fill="none">
          <rect x="0" y="0" width="512" height="512" rx="112" ry="112" fill="#0078D4" />
          <g stroke="#FFFFFF" stroke-width="20" stroke-linejoin="round" stroke-linecap="round">
            <polygon points="256.00,105.52 321.82,143.52 321.82,219.52 256.00,257.52 190.18,219.52 190.18,143.52" />
            <polygon points="170.00,254.48 235.82,292.48 235.82,368.48 170.00,406.48 104.18,368.48 104.18,292.48" />
            <polygon points="342.00,254.48 407.82,292.48 407.82,368.48 342.00,406.48 276.18,368.48 276.18,292.48" />
          </g>
        </svg>
      </div>
      <div class="hero-info">
        <div class="hero-title-row">
          <span class="about-title">EC20 Hub</span>
          <span class="version-tag">v0.1.0</span>
        </div>
        <div class="about-desc">
          Quectel EC20 / EC25 4G 模块 Windows 桌面管理工具，基于 Tauri v2 + Rust + Vue 3 构建。
        </div>
        <div class="about-meta">
          本项目为开源软件，依据 <strong>MIT</strong> 许可证发布。Copyright © 2026 EC20 Hub Contributors.
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card-title">适配与使用说明</div>
      <ul class="about-list">
        <li>自动识别 <code>VID_2C7C &amp; PID_0125</code> 移远 AT 端口（如 <code>COM4</code>）。</li>
        <li>支持 GSM 7-bit 与 UCS2 中文编码及 UDH 长短信分段重组，自动提取验证码。</li>
        <li>开启「接管模块短信」后，新短信将归档至本机并清理 SIM 卡与模块存储，避免存满拒收。</li>
        <li>关闭主窗口默认最小化到系统托盘；左键托盘图标弹出状态悬浮窗，右键可打开主窗口或退出。</li>
        <li>程序启动时请求管理员权限，用于「网卡优先级」跃点数调整与计划任务开机自启。</li>
      </ul>
    </div>

    <div class="card">
      <div class="card-title">第三方库</div>
      <ul class="lib-list">
        <li><code>Tauri v2</code> <span>(Apache-2.0 / MIT License)</span></li>
        <li><code>Vue 3</code> <span>(MIT License)</span></li>
        <li><code>Lucide Vue Next</code> <span>(ISC License)</span></li>
        <li><code>serialport</code> <span>(MPL-2.0 License)</span></li>
        <li><code>sysinfo</code> <span>(MIT License)</span></li>
        <li><code>tokio</code> <span>(MIT License)</span></li>
        <li><code>serde / serde_json</code> <span>(Apache-2.0 / MIT License)</span></li>
        <li><code>chrono</code> <span>(Apache-2.0 / MIT License)</span></li>
        <li><code>regex</code> <span>(Apache-2.0 / MIT License)</span></li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.about-wrap {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.hero-card {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 16px 18px;
}

.app-logo {
  flex-shrink: 0;
  display: flex;
}

.hero-info {
  min-width: 0;
}

.hero-title-row {
  display: flex;
  align-items: baseline;
  gap: 8px;
}

.about-title {
  font-size: 18px;
  font-weight: 700;
  color: var(--text-primary);
}

.version-tag {
  font-size: 12px;
  color: var(--accent);
  font-weight: 500;
}

.about-desc {
  margin-top: 4px;
  font-size: 12.5px;
  color: var(--text-primary);
  line-height: 1.5;
}

.about-meta {
  margin-top: 4px;
  font-size: 11.5px;
  color: var(--text-secondary);
}

.card-title {
  font-size: 13px;
  font-weight: 600;
  margin-bottom: 8px;
  color: var(--text-primary);
}

.about-list,
.lib-list {
  padding-left: 18px;
  font-size: 12px;
  line-height: 1.75;
  color: var(--text-secondary);
}

.lib-list {
  columns: 2;
  column-gap: 24px;
}

.lib-list span {
  color: var(--text-muted);
}

code {
  background: var(--bg-hover);
  padding: 1px 5px;
  border-radius: 4px;
  color: var(--text-primary);
}
</style>
