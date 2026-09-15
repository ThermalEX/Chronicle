import { invoke, isTauri } from "@tauri-apps/api/core";

export type TutorialTip = "categories" | "exclusions" | "registry";
export type TutorialProgress = { status: "pending" | "completed" | "skipped" | "legacy"; seenTips: TutorialTip[] };
export type TutorialStep = "inactive" | "welcome" | "appearance" | "route" | "cloud-entry" | "cloud-form" | "create-entry" | "sources" | "name" | "storage" | "automation" | "create-submit" | "snapshot" | "timeline" | "restore" | "lock" | "sync" | "finish";
export type TutorialState = { step: TutorialStep; route: "local" | "cloud"; archiveId?: string };
export type TutorialEvent = { type: "start" | "choose-local" | "choose-cloud" | "cloud-opened" | "cloud-saved" | "create-opened" | "sources-ready" | "name-ready" | "next" | "archive-created" | "snapshot-created" | "locked" | "dialog-closed" | "skip" | "restart" | "use-existing"; archiveId?: string; hasSnapshot?: boolean };
export function createTutorialState(): TutorialState { return { step: "welcome", route: "local" }; }
export function advanceTutorial(state: TutorialState, event: TutorialEvent): TutorialState {
  if (event.type === "restart") return createTutorialState();
  if (event.type === "skip") return { ...state, step: "inactive" };
  if (state.step === "inactive") return state;
  if (event.type === "choose-local" && ["route", "cloud-entry", "cloud-form"].includes(state.step)) return { ...state, route: "local", step: "create-entry" };
  if (state.step === "route" && event.type === "choose-cloud") return { ...state, route: "cloud", step: "cloud-entry" };
  if (event.type === "dialog-closed") {
    if (state.step === "cloud-form") return { ...state, step: "cloud-entry" };
    if (["sources", "name", "storage", "automation", "create-submit"].includes(state.step)) return { ...state, step: "create-entry" };
  }
  if (state.step === "create-entry" && event.type === "use-existing" && event.archiveId) return { ...state, archiveId: event.archiveId, step: event.hasSnapshot ? "timeline" : "snapshot" };
  if (state.step === "create-submit" && event.type === "archive-created" && event.archiveId) return { ...state, archiveId: event.archiveId, step: "snapshot" };
  if (state.step === "snapshot" && event.type === "snapshot-created" && (!event.archiveId || event.archiveId === state.archiveId)) return { ...state, step: "timeline" };
  const transitions: Partial<Record<TutorialStep, Partial<Record<TutorialEvent["type"], TutorialStep>>>> = {
    welcome: { start: "appearance" }, appearance: { next: "route" }, "cloud-entry": { "cloud-opened": "cloud-form" }, "cloud-form": { "cloud-saved": "create-entry" },
    "create-entry": { "create-opened": "sources" }, sources: { "sources-ready": "name" }, name: { "name-ready": "storage" },
    storage: { next: "automation" }, automation: { next: "create-submit" }, snapshot: { next: "sync" },
    timeline: { next: "restore" }, restore: { next: "lock" }, lock: { locked: "sync", next: "sync" }, sync: { next: "finish" }, finish: { next: "inactive" },
  };
  return { ...state, step: transitions[state.step]?.[event.type] ?? state.step };
}

export function normalizeTutorialProgress(value: unknown): TutorialProgress {
  const data = value && typeof value === "object" ? value as Partial<TutorialProgress> : {};
  return {
    status: ["pending", "completed", "skipped", "legacy"].includes(data.status ?? "") ? data.status! : "legacy",
    seenTips: Array.isArray(data.seenTips) ? [...new Set(data.seenTips.filter((tip): tip is TutorialTip => ["categories", "exclusions", "registry"].includes(tip)))] : [],
  };
}
export function shouldOfferTutorial(progress: TutorialProgress): boolean { return progress.status === "pending"; }
const key = "chronicle.onboarding.local.v1";
export async function loadTutorialProgress(existingData: boolean): Promise<TutorialProgress> {
  if (isTauri()) return normalizeTutorialProgress(await invoke("load_onboarding"));
  const saved = localStorage.getItem(key);
  if (saved) { try { return normalizeTutorialProgress(JSON.parse(saved)); } catch { return { status: "legacy", seenTips: [] }; } }
  const progress: TutorialProgress = { status: existingData || Boolean(localStorage.getItem("chronicle.app-settings.v2")) || Boolean(localStorage.getItem("chronicle.cloud-settings.v2")) ? "legacy" : "pending", seenTips: [] };
  await saveTutorialProgress(progress);
  return progress;
}
export async function saveTutorialProgress(progress: TutorialProgress): Promise<void> {
  const safe = normalizeTutorialProgress(progress);
  if (isTauri()) await invoke("save_onboarding", { progress: safe });
  else localStorage.setItem(key, JSON.stringify(safe));
}

export type TutorialView = { target: string; title: string; body: string; stage: number; mode: "action" | "explain" | "form"; primaryLabel?: string };
export const tutorialViews: Partial<Record<TutorialStep, TutorialView>> = {
  "cloud-entry": { target: "cloud-entry", title: "先连接一个云端", body: "点击这里打开云端设置。没有云端账号也没关系，可以暂时仅本地使用。", stage: 2, mode: "action" },
  "cloud-form": { target: "cloud-form", title: "配置你的同步源", body: "添加同步源，填写连接信息，检测成功后保存。可自由操作此窗口；未准备好时点击“暂时仅本地”。", stage: 2, mode: "form" },
  "create-entry": { target: "create-entry", title: "创建第一个存档", body: "点击“添加存档”，把文件或文件夹保存为可恢复的历史快照。", stage: 3, mode: "action" },
  sources: { target: "sources", title: "选择需要保护的内容", body: "添加一个文件或文件夹。可组合多个来源；取消文件选择不会丢失当前进度。", stage: 3, mode: "form" },
  name: { target: "name", title: "给存档起个名字", body: "例如“游戏进度”或“工作配置”。名称便于查找，不会重命名原始文件。", stage: 3, mode: "form", primaryLabel: "名称已填好" },
  storage: { target: "storage", title: "决定保存到哪里", body: "“仅本地”保存在这台电脑；“本地与云端”允许同步到已配置的同步源。没有配置云端时也可以先保留本地。", stage: 4, mode: "form", primaryLabel: "继续" },
  automation: { target: "automation", title: "两个独立的自动化开关", body: "自动备份：文件变化后创建快照。自动上传：新快照生成后上传。两项一起开启才是全自动；也可以都保持关闭。", stage: 4, mode: "form", primaryLabel: "我已了解" },
  "create-submit": { target: "create-submit", title: "保存你的第一个存档", body: "点击创建。“创建后立即备份”会保存第一份快照；是否自动上传取决于刚才选择的设置。", stage: 4, mode: "action" },
  snapshot: { target: "snapshot", title: "保存一个时间节点", body: "点击创建快照，保存当前内容。如果正在生成初始快照，请稍候。也可以先跳过这一步。", stage: 5, mode: "action", primaryLabel: "暂不备份" },
  timeline: { target: "timeline", title: "每份快照都是一个历史状态", body: "这里按时间列出快照，可查看描述、变更和大小。今后随时可以回来找到需要的版本。", stage: 5, mode: "explain", primaryLabel: "知道了" },
  restore: { target: "restore", title: "恢复以前的状态", body: "这里可以恢复快照，恢复前会先创建安全快照。本步只是了解功能，点击高亮区域不会恢复文件。", stage: 6, mode: "explain", primaryLabel: "了解，不执行恢复" },
  lock: { target: "lock", title: "为重要快照加星", body: "点击星标可锁定或解锁快照。锁定后不参与自动清理，删除前需要解锁。不想改变状态也可以继续。", stage: 6, mode: "action", primaryLabel: "继续，不改变星标" },
  sync: { target: "sync", title: "把存档同步到云端", body: "此入口将存档同步到启用的云端源；需要“本地与云端”保存方式。此处仅介绍入口，不会发起上传。", stage: 6, mode: "explain", primaryLabel: "知道了" },
};
