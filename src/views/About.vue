<template>
  <section class="ft-about ft-stagger ft-stagger-1">
    <div class="ft-page-header ft-about-header">
      <div class="ft-page-header-text">
        <div class="ft-page-eyebrow">关于</div>
        <h1 class="ft-about-title">FingerTip</h1>
        <p class="ft-page-subtitle">把每天的打字节奏，变成一首歌、一幅画。</p>
      </div>
      <div class="ft-about-version">v{{ version }}</div>
    </div>

    <div class="ft-about-grid">
      <div class="ft-about-card">
        <div class="ft-about-card-eyebrow">它是做什么的</div>
        <p class="ft-about-card-text">
          后台记录你的键盘行为，次日用 AI 产出一段音乐与一幅数字画作——你的输入节奏就是创作素材。
        </p>
      </div>
      <div class="ft-about-card">
        <div class="ft-about-card-eyebrow">隐私</div>
        <p class="ft-about-card-text">
          隐私优先：原始按键数据不出本机。仅在你主动生成时，才把聚合后的摘要发送给云端模型。
        </p>
      </div>
      <div class="ft-about-card">
        <div class="ft-about-card-eyebrow">技术</div>
        <p class="ft-about-card-text">
          本地 FingerTip-Engine（Python）优先，云端 MiniMax 兑底；三态能力路由自动选择可用路径。
        </p>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
// v0.4.2: 版本号从 package.json 读，避免再次写死过期（此前停在 v0.3.0）
import { version } from '../../package.json'
</script>

<style scoped>
.ft-about-header {
  align-items: flex-end;
}
.ft-about-title {
  font-family: var(--font-hand);
  font-size: 72px;
  font-weight: 700;
  line-height: 1;
  letter-spacing: -0.01em;
  color: var(--text-primary);
  margin-bottom: var(--sp-2);
}
.ft-about-version {
  font-family: var(--font-mono);
  font-size: 14px;
  font-weight: 600;
  color: var(--accent-warm);
  background: var(--accent-warm-soft);
  border: 1px solid rgba(214, 123, 79, 0.25);
  padding: var(--sp-1) var(--sp-3);
  border-radius: 100px;
  white-space: nowrap;
}

.ft-about-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: var(--sp-4);
}
.ft-about-card {
  background: var(--bg-surface);
  border: 1px solid var(--border-subtle);
  border-radius: var(--r-md);
  padding: var(--sp-6) var(--sp-8);
  box-shadow: var(--shadow-1);
  transition: transform 200ms ease, box-shadow 200ms ease;
}
.ft-about-card:hover {
  transform: translateY(-2px);
  box-shadow: var(--shadow-2);
}
.ft-about-card-eyebrow {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-tertiary);
  margin-bottom: var(--sp-3);
}
.ft-about-card-text {
  margin: 0;
  font-size: 14px;
  line-height: 1.7;
  color: var(--text-secondary);
}
</style>
