<script setup lang="ts">
import { CheckCircle2, CircleAlert, CloudCog, LoaderCircle, X } from "@lucide/vue";
import { computed, onMounted, ref } from "vue";
import type { CloudHealthCheckItem } from "../services/cloudHealthCheck";

const props = defineProps<{ items: CloudHealthCheckItem[]; running: boolean }>();
const emit = defineEmits<{ close: [] }>();
const closeButton = ref<HTMLButtonElement>();
const completed = computed(() => props.items.filter((item) => item.status !== "checking").length);
const total = computed(() => props.items.length);
const percent = computed(() => total.value ? Math.round(completed.value / total.value * 100) : 0);
onMounted(() => closeButton.value?.focus());
</script>

<template>
  <div class="health-backdrop" @click.self="emit('close')">
    <section class="health-dialog" role="dialog" aria-modal="true" aria-labelledby="health-title">
      <header><span class="health-icon"><CloudCog :size="20" /></span><div><p>云端资料库</p><h2 id="health-title">连接检测</h2></div><button ref="closeButton" aria-label="关闭云端检测" title="关闭云端检测" @click="emit('close')"><X :size="18" /></button></header>
      <div class="health-progress"><div><span>{{ running ? '正在并发检测' : '检测完成' }}</span><b>{{ completed }} / {{ total }}</b></div><progress :value="completed" :max="total || 1">{{ percent }}%</progress></div>
      <div v-if="items.length" class="health-list"><article v-for="item in items" :key="item.id" :class="item.status"><span class="health-state"><LoaderCircle v-if="item.status === 'checking'" :size="17" /><CheckCircle2 v-else-if="item.status === 'passed'" :size="17" /><CircleAlert v-else :size="17" /></span><span class="health-copy"><b>{{ item.name }}</b><small v-if="item.status === 'checking'">正在检测读写、列举与清理能力…</small><small v-else-if="item.status === 'passed'">连接与访问正常</small><small v-else>{{ item.reason || '无法连接或权限不足' }}</small></span></article></div><p v-else class="health-empty">尚未配置可检测的云端同步源。</p>
      <footer><button @click="emit('close')">{{ running ? '后台继续检测' : '关闭' }}</button></footer>
    </section>
  </div>
</template>

<style scoped>
.health-backdrop { position: fixed; z-index: 80; inset: 0; display: grid; place-items: center; padding: 24px; background: #18181b99; backdrop-filter: blur(3px); }.health-dialog { width: min(510px, calc(100vw - 48px)); overflow: hidden; color: var(--text); background: var(--surface); border: 1px solid var(--border-2); border-radius: 12px; box-shadow: 0 24px 80px var(--shadow-color); }.health-dialog header { display: grid; grid-template-columns: 40px 1fr 38px; align-items: center; gap: 11px; padding: 17px 18px; border-bottom: 1px solid var(--border); }.health-icon { display: grid; place-items: center; width: 38px; height: 38px; color: var(--primary-dark); background: var(--primary-soft); border-radius: 9px; }.health-dialog header p,.health-dialog header h2 { margin: 0; }.health-dialog header p { color: var(--text-3); font-size: 9px; font-weight: 700; }.health-dialog header h2 { margin-top: 3px; font-size: 17px; }.health-dialog header button { display: grid; place-items: center; width: 38px; height: 38px; color: var(--text-2); background: transparent; border-radius: 7px; }.health-dialog header button:hover { background: var(--hover); }.health-progress { padding: 15px 18px 13px; border-bottom: 1px solid var(--border); }.health-progress div { display: flex; justify-content: space-between; color: var(--text-2); font-size: 11px; }.health-progress b { color: var(--text); font-variant-numeric: tabular-nums; }.health-progress progress { width: 100%; height: 6px; margin-top: 9px; overflow: hidden; appearance: none; border: 0; border-radius: 999px; }.health-progress progress::-webkit-progress-bar { background: var(--subtle); }.health-progress progress::-webkit-progress-value { background: var(--primary); }.health-list { max-height: min(390px, calc(100vh - 280px)); overflow-y: auto; }.health-list article { display: grid; grid-template-columns: 26px minmax(0, 1fr); align-items: center; gap: 10px; min-height: 62px; padding: 10px 18px; border-bottom: 1px solid var(--border); }.health-state { display: grid; place-items: center; color: var(--text-3); }.checking .health-state { color: var(--primary); }.checking .health-state svg { animation: spin 1s linear infinite; }.passed .health-state { color: var(--success); }.failed .health-state { color: var(--danger); }.health-copy { display: flex; min-width: 0; flex-direction: column; gap: 4px; }.health-copy b { overflow: hidden; font-size: 11px; text-overflow: ellipsis; white-space: nowrap; }.health-copy small { overflow: hidden; color: var(--text-3); font-size: 10px; line-height: 1.4; text-overflow: ellipsis; white-space: nowrap; }.failed .health-copy small { color: var(--danger); }.health-empty { margin: 0; padding: 28px 18px; color: var(--text-3); font-size: 11px; text-align: center; }.health-dialog footer { display: flex; justify-content: flex-end; padding: 13px 18px; border-top: 1px solid var(--border); }.health-dialog footer button { min-height: 36px; padding: 0 14px; color: var(--on-primary); background: var(--primary); border-radius: 7px; font-size: 11px; font-weight: 650; }.health-dialog footer button:hover { background: var(--primary-dark); }@keyframes spin { to { transform: rotate(360deg); } }
</style>
