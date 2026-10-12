import { computed, ref } from "vue";
import defaultIcon from "../assets/icon.png";

export interface LocalIconState { mode: "color" | "image"; imageDataUrl?: string | null; warning?: string | null }
export const localIcon = ref<LocalIconState>({ mode: "color" });
export const currentAppIcon = computed(() => localIcon.value.mode === "image" && localIcon.value.imageDataUrl ? localIcon.value.imageDataUrl : defaultIcon);
