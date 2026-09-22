<script setup lang="ts">
import { locale, t } from "../services/i18n";
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { ArrowRight, Cloud, FolderArchive, HardDrive, ShieldCheck, X } from "@lucide/vue";
import type { TutorialView } from "../services/onboarding";
import { placeTutorialCard } from "../services/tutorialGeometry";
import appIcon from "../assets/icon.png";
import TutorialThemePicker from "./TutorialThemePicker.vue";
import type { Appearance } from "../services/appearance";

const props = defineProps<{ welcome?: boolean; languageSaving?: boolean; appearancePage?: boolean; appearance: Appearance; appearanceSaving?: boolean; route?: boolean; finish?: boolean; step?: TutorialView; hasArchive?: boolean }>();
const emit = defineEmits<{ "language-change": [language: "zh-CN" | "en"]; "appearance-change": [appearance: Appearance]; start: []; skip: []; local: []; cloud: []; next: []; "use-existing": [] }>();
const card = ref<HTMLElement>();
const paused = ref(false);
const languageChosen = ref(false);
const targetRect = ref<{ left: number; top: number; right: number; bottom: number }>();
const placement = ref({ left: 12, top: 12 });
const viewport = ref({ width: window.innerWidth, height: window.innerHeight });
const previousFocus = document.activeElement as HTMLElement | null;
let target: HTMLElement | null = null;
let interactionArea: HTMLElement | null = null;
const formStep = computed(() => Boolean(props.step && ["cloud-form", "sources", "name", "storage", "automation", "create-submit"].includes(props.step.target)));
const interactionRect = ref<{ left: number; top: number; right: number; bottom: number }>();
const spotlightRect = computed(() => targetRect.value);
let frame = 0;
let lastTarget = "";
const fullPage = computed(() => props.welcome || props.appearancePage || props.route || props.finish);
const masks = computed(() => {
  const r = interactionRect.value, { width, height } = viewport.value;
  if (!r) return [{ left: 0, top: 0, width, height }];
  return [
    { left: 0, top: 0, width, height: r.top },
    { left: 0, top: r.bottom, width, height: height - r.bottom },
    { left: 0, top: r.top, width: r.left, height: r.bottom - r.top },
    { left: r.right, top: r.top, width: width - r.right, height: r.bottom - r.top },
  ];
});
const pixels = (value: Record<string, number>) => Object.fromEntries(Object.entries(value).map(([key, amount]) => [key, `${amount}px`]));
function measure() {
  frame = 0;
  const interrupted = Boolean(document.querySelector('[role="alertdialog"]'));
  if (interrupted !== paused.value) {
    paused.value = interrupted;
    document.body.classList.toggle("tutorial-form-active", formStep.value && !interrupted);
    if (!interrupted) { void changed(); return; }
  }
  if (paused.value) return;
  viewport.value = { width: window.innerWidth, height: window.innerHeight };
  target = props.step ? document.querySelector<HTMLElement>(`[data-tour="${props.step.target}"]`) : null;
  interactionArea = formStep.value ? target?.closest<HTMLElement>('[role="dialog"]') ?? target : target;
  if (target && target.getClientRects().length) {
    const r = target.getBoundingClientRect();
    targetRect.value = { left: Math.max(0, r.left - 5), top: Math.max(0, r.top - 5), right: Math.min(window.innerWidth, r.right + 5), bottom: Math.min(window.innerHeight, r.bottom + 5) };
    const area = interactionArea?.getBoundingClientRect() ?? r;
    interactionRect.value = { left: Math.max(0, area.left - 5), top: Math.max(0, area.top - 5), right: Math.min(window.innerWidth, area.right + 5), bottom: Math.min(window.innerHeight, area.bottom + 5) };
    const box = card.value?.getBoundingClientRect();
    placement.value = placeTutorialCard(targetRect.value, window.innerWidth, window.innerHeight, box?.width ?? 340, box?.height ?? 220);
    if (formStep.value) placement.value = window.innerWidth >= 900
      ? { left: window.innerWidth - 352, top: Math.max(12, (window.innerHeight - (box?.height ?? 220)) / 2) }
      : { left: 12, top: window.innerHeight - 228 };
  } else {
    targetRect.value = undefined;
    interactionRect.value = undefined;
    placement.value = { left: Math.max(12, (window.innerWidth - 340) / 2), top: Math.max(12, (window.innerHeight - 220) / 2) };
  }
}
function schedule() { if (!frame) frame = requestAnimationFrame(measure); }
async function changed() {
  document.body.classList.toggle("tutorial-form-active", formStep.value && !paused.value);
  await nextTick();
  target = props.step ? document.querySelector<HTMLElement>(`[data-tour="${props.step.target}"]`) : null;
  if (target && lastTarget !== props.step?.target) {
    target.scrollIntoView({ block: "nearest", inline: "nearest", behavior: "instant" });
    lastTarget = props.step!.target;
  }
  measure();
  resizeObserver.disconnect();
  if (interactionArea) resizeObserver.observe(interactionArea);
  if (target) resizeObserver.observe(target);
  await nextTick();
  card.value?.focus({ preventScroll: true });
  schedule();
}
function click(event: MouseEvent) {
  if (document.querySelector('[role="alertdialog"]')) return;
  if (props.step?.mode === "explain" && target?.contains(event.target as Node)) {
    event.preventDefault(); event.stopImmediatePropagation(); emit("next");
  }
}
function keydown(event: KeyboardEvent) {
  if (document.querySelector('[role="alertdialog"]')) return;
  if (event.key === "Escape") { event.preventDefault(); event.stopImmediatePropagation(); emit("skip"); return; }
  if (props.step?.mode === "explain" && target?.contains(event.target as Node) && ["Enter", " "].includes(event.key)) {
    event.preventDefault(); event.stopImmediatePropagation(); emit("next"); return;
  }
  if (event.key !== "Tab") return;
  const selector = 'button:not(:disabled), input:not(:disabled):not([type="hidden"]), select:not(:disabled), textarea:not(:disabled), a[href]';
  const targets = props.step?.mode !== "explain" && interactionArea ? [interactionArea, ...interactionArea.querySelectorAll<HTMLElement>(selector), ...document.querySelectorAll<HTMLElement>('[role="listbox"] button')] : [];
  const controls = [...targets, ...(card.value?.querySelectorAll<HTMLElement>(selector) ?? [])].filter((item) => item.tabIndex >= 0 && item.getClientRects().length > 0);
  if (!controls.length) return;
  const index = controls.indexOf(document.activeElement as HTMLElement);
  event.preventDefault(); event.stopImmediatePropagation();
  controls[index === -1 ? (event.shiftKey ? controls.length - 1 : 0) : (index + (event.shiftKey ? -1 : 1) + controls.length) % controls.length]?.focus();
}
const observer = new MutationObserver(schedule);
const resizeObserver = new ResizeObserver(schedule);
onMounted(() => {
  document.addEventListener("click", click, true);
  document.addEventListener("keydown", keydown, true);
  window.addEventListener("resize", schedule);
  document.addEventListener("scroll", schedule, true);
  observer.observe(document.querySelector(".app-shell") ?? document.body, { childList: true, subtree: true });
  void changed();
});
watch(() => [props.step?.target, props.welcome, props.appearancePage, props.route, props.finish], changed);
onBeforeUnmount(() => {
  cancelAnimationFrame(frame); observer.disconnect(); resizeObserver.disconnect();
  document.body.classList.remove("tutorial-form-active");
  document.removeEventListener("click", click, true); document.removeEventListener("keydown", keydown, true);
  window.removeEventListener("resize", schedule); document.removeEventListener("scroll", schedule, true);
  if (previousFocus?.isConnected) previousFocus.focus({ preventScroll: true });
});
</script>

<template>
  <Teleport to="body">
    <div v-if="!paused" class="tutorial-layer" :class="{ 'full-page': fullPage, 'form-step': formStep }">
      <template v-if="!fullPage">
        <div v-for="(mask, index) in masks" :key="index" class="tutorial-dim" :class="{ 'no-target': !interactionRect }" :style="pixels(mask)" />
        <div v-if="spotlightRect" class="tutorial-shade" :style="pixels({ left: spotlightRect.left, top: spotlightRect.top, width: spotlightRect.right - spotlightRect.left, height: spotlightRect.bottom - spotlightRect.top })" />
        <div v-if="targetRect" class="tutorial-outline" :style="pixels({ left: targetRect.left, top: targetRect.top, width: targetRect.right - targetRect.left, height: targetRect.bottom - targetRect.top })" />
      </template>
      <section ref="card" class="tutorial-card" :class="{ 'welcome-card': fullPage, 'appearance-card': appearancePage }" :style="fullPage ? undefined : pixels(placement)" role="dialog" aria-modal="true" aria-labelledby="tutorial-title" aria-describedby="tutorial-body" tabindex="-1">
        <button class="tutorial-close" :aria-label="t('跳过教程')" :title="t('跳过教程')" @click="emit('skip')"><X :size="18" /></button>
        <template v-if="welcome && !languageChosen">
          <div class="welcome-greetings" aria-hidden="true"><span v-for="(greeting, index) in ['Hello', '你好', 'Hola', 'Bonjour']" :key="greeting" :style="{ animationDelay: `${index * 3}s` }">{{ greeting }}</span></div>
          <p class="tutorial-eyebrow">CHRONICLE</p>
          <h1 id="tutorial-title">{{ t('选择你的语言') }}</h1>
          <p id="tutorial-body">{{ t('从熟悉的语言开始，之后可在设置中更改。') }}</p>
          <div class="welcome-languages" :aria-label="t('界面语言')">
            <button type="button" :aria-pressed="locale === 'zh-CN'" :disabled="languageSaving" @click="emit('language-change', 'zh-CN')"><span>简体中文</span></button>
            <button type="button" :aria-pressed="locale === 'en'" :disabled="languageSaving" @click="emit('language-change', 'en')"><span>English</span></button>
          </div>
          <button class="tutorial-primary" :disabled="languageSaving" @click="languageChosen = true">{{ t('继续') }} <ArrowRight :size="16" /></button>
        </template>
        <template v-else-if="welcome">
          <img class="tutorial-logo" :src="appIcon" alt="Chronicle" />
          <p class="tutorial-eyebrow">{{ t('欢迎使用 CHRONICLE') }}</p>
          <h1 id="tutorial-title">{{ t('让每次改变，都有迹可循') }}</h1>
          <p id="tutorial-body">{{ t('为文件、文件夹和注册表保存可恢复的历史快照，') }}<br />{{ t('也能同步到云端。用几次点击，建立你的第一份存档。') }}</p>
          <div class="tutorial-benefits"><span><FolderArchive :size="21" />{{ t('保存历史') }}</span><span><ShieldCheck :size="21" />{{ t('安心恢复') }}</span><span><Cloud :size="21" />{{ t('云端同步') }}</span></div>
          <button class="tutorial-primary" @click="emit('start')">{{ t('开始使用') }} <ArrowRight :size="16" /></button><button class="tutorial-link" @click="emit('skip')">{{ t('跳过教程，直接进入') }}</button>
        </template>
        <template v-else-if="appearancePage">
          <p class="tutorial-eyebrow">{{ t('你的 CHRONICLE') }}</p><h1 id="tutorial-title">{{ t('选一个喜欢的外观') }}</h1><p id="tutorial-body">{{ t('让熟悉的工作空间，有你喜欢的颜色。') }}</p>
          <TutorialThemePicker :appearance="appearance" :saving="appearanceSaving" @change="emit('appearance-change', $event)" />
          <button class="tutorial-primary" :disabled="appearanceSaving" @click="emit('next')">{{ appearanceSaving ? t('保存中…') : t('继续，选择使用方式') }} <ArrowRight :size="16" /></button>
        </template>
        <template v-else-if="route">
          <p class="tutorial-eyebrow">{{ t('开始之前') }}</p><h1 id="tutorial-title">{{ t('先从哪里开始？') }}</h1><p id="tutorial-body">{{ t('数据始终保存在本地。云端是可选的，你随时可以回来配置。') }}</p>
          <div class="tutorial-routes"><button @click="emit('cloud')"><Cloud :size="24" /><b>{{ t('先配置云端') }}</b><small>{{ t('连接同步源，方便跨设备使用') }}</small></button><button @click="emit('local')"><HardDrive :size="24" /><b>{{ t('暂时仅本地') }}</b><small>{{ t('无需账号，先创建第一份存档') }}</small></button></div>
        </template>
        <template v-else-if="finish">
          <ShieldCheck class="tutorial-finish-icon" :size="48" /><p class="tutorial-eyebrow">{{ t('准备就绪') }}</p><h1 id="tutorial-title">{{ t('你的时间线，从这里开始') }}</h1><p id="tutorial-body">{{ t('分类、排除规则和注册表备份可以在需要时再了解。') }}<br />{{ t('在「设置 → 关于」中可随时重新开始教程。') }}</p><button class="tutorial-primary" @click="emit('next')">{{ t('开始使用 Chronicle') }}</button>
        </template>
        <template v-else>
          <p class="tutorial-eyebrow">{{ t('快速入门 · {stage} / 7', { stage: step?.stage || 1 }) }}</p><h2 id="tutorial-title">{{ step?.title }}</h2><p id="tutorial-body" aria-live="polite">{{ targetRect ? step?.body : t('当前区域尚未显示。可以关闭当前弹窗后重试，或随时跳过教程。') }}</p>
          <div class="tutorial-card-actions"><button v-if="step?.primaryLabel && targetRect" class="tutorial-primary" @click="emit('next')">{{ step.primaryLabel }} <ArrowRight :size="14" /></button><button v-if="step?.target?.startsWith('cloud')" class="tutorial-link" @click="emit('local')">{{ t('暂时仅本地') }}</button><button v-if="step?.target === 'create-entry' && hasArchive" class="tutorial-link" @click="emit('use-existing')">{{ t('使用已有存档') }}</button><button class="tutorial-link" @click="emit('skip')">{{ t('跳过教程') }}</button></div>
        </template>
      </section>
    </div>
  </Teleport>
</template>

<style scoped>
.welcome-greetings { position: relative; height: 110px; margin: 12px 0 26px; color: var(--primary); font-size: clamp(48px, 8vw, 76px); font-weight: 650; letter-spacing: -.04em; }
.welcome-greetings span { position: absolute; inset: 0; display: grid; place-items: center; opacity: 0; animation: welcome-greeting 12s infinite; }
@keyframes welcome-greeting { 0% { opacity: 0; transform: translateY(12px); } 3%, 21% { opacity: 1; transform: translateY(0); } 25%, 100% { opacity: 0; transform: translateY(-12px); } }
.welcome-languages { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 14px; max-width: 420px; margin: 28px auto; }
.welcome-languages button { display: grid; gap: 5px; text-align: left; padding: 18px 22px; border-radius: 12px; border: 1px solid var(--border-2); color: var(--text); background: var(--surface); transition: border-color .16s, background .16s; }
.welcome-languages button[aria-pressed="true"] { border-color: var(--primary); background: var(--primary-soft); box-shadow: inset 0 0 0 1px var(--primary); }
.welcome-languages span { font-size: 17px; font-weight: 650; }
.tutorial-layer { position: fixed; inset: 0; z-index: 10000; overflow: hidden; pointer-events: none; }
.tutorial-layer.full-page { display: grid; place-items: center; background: var(--app-background); pointer-events: auto; padding: 24px; overflow: auto; }
.tutorial-dim { position: absolute; pointer-events: auto; }
.tutorial-dim.no-target { background: #0b1529a6; }
.tutorial-shade { position: absolute; border-radius: 10px; box-shadow: 0 0 0 100vmax #0b1529a6; pointer-events: none; }
.tutorial-outline { position: absolute; border: 2px solid var(--primary); border-radius: 10px; pointer-events: none; }
.tutorial-card { position: absolute; width: min(340px, calc(100vw - 24px)); max-height: calc(100vh - 24px); overflow: auto; padding: 22px; color: var(--text); background: var(--surface); border: 1px solid var(--border-2); border-radius: 14px; box-shadow: 0 18px 60px #00000040; pointer-events: auto; }
.tutorial-card.welcome-card { position: relative; width: min(700px, 100%); padding: 52px 40px 38px; text-align: center; box-shadow: 0 20px 70px var(--shadow-color); }
.tutorial-card.appearance-card { width: min(640px, 100%); padding-top: 28px; padding-bottom: 24px; }
.tutorial-close { position: absolute; top: 12px; right: 12px; display: grid; place-items: center; width: 30px; height: 30px; color: var(--text-2); background: transparent; border-radius: 6px; }
.tutorial-logo { width: 76px; height: 76px; margin-bottom: 12px; }
.tutorial-eyebrow { color: var(--primary-dark); font-size: 11px; font-weight: 700; letter-spacing: .08em; margin: 0 0 12px; }
h1 { font-size: 27px; line-height: 1.35; margin: 12px 0 18px; } h2 { font-size: 17px; margin: 8px 0 12px; padding-right: 10px; }
#tutorial-body { font-size: 13px; line-height: 1.8; color: var(--text-2); margin: 0 0 18px; }
.tutorial-benefits { display: flex; justify-content: center; gap: 36px; margin: 30px 0; }.tutorial-benefits span { display: grid; justify-items: center; gap: 10px; color: var(--text-2); font-size: 12px; }
.tutorial-primary { display: inline-flex; align-items: center; justify-content: center; gap: 8px; padding: 11px 16px; color: var(--on-primary); background: var(--primary); border-radius: 8px; font-size: 12px; font-weight: 650; }
.tutorial-link { color: var(--text-2); background: transparent; padding: 10px 8px; font-size: 12px; }.welcome-card > .tutorial-link { display: block; margin: 8px auto 0; }
.tutorial-card-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; }
.tutorial-routes { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; margin-top: 28px; }.tutorial-routes button { display: grid; justify-items: start; text-align: left; gap: 12px; padding: 24px; border: 1px solid var(--border-2); background: var(--subtle); border-radius: 12px; color: var(--primary-dark); }.tutorial-routes small { color: var(--text-2); font-size: 12px; line-height: 1.5; }
.tutorial-finish-icon { color: var(--primary); margin-bottom: 20px; }button:hover { filter: brightness(.96); }button:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 3px; }
@media(max-width: 560px) { .tutorial-card.welcome-card { padding: 36px 20px 24px; }.tutorial-routes { grid-template-columns: 1fr; }h1 { font-size: 23px; } }
@media(prefers-reduced-motion: reduce) { * { scroll-behavior: auto !important; transition: none !important; }.welcome-greetings span { animation: none; }.welcome-greetings span:first-child { opacity: 1; } }
</style>

<style>
.tutorial-form-active .dialog-backdrop { padding: 16px 368px 16px 16px; }
.tutorial-form-active .create-dialog, .tutorial-form-active .cloud-center { width: 100%; max-height: calc(100vh - 32px); }
.tutorial-form-active [role="listbox"] { z-index: 10002; }
@media (max-width: 899px) {
  .tutorial-form-active .dialog-backdrop { padding: 12px 12px 244px; }
  .tutorial-form-active .create-dialog, .tutorial-form-active .cloud-center { max-height: calc(100vh - 256px); }
  .tutorial-layer.form-step .tutorial-card { width: calc(100vw - 24px); max-height: 216px; padding: 16px; }
}
</style>
