import { nextTick, ref } from "vue";

export const colorThemes = ["teal", "indigo", "violet", "amber", "rose", "gray", "custom"] as const;
export const colorModes = ["light", "dark", "system"] as const;

export type ColorTheme = typeof colorThemes[number];
export type ColorMode = typeof colorModes[number];
export type ResolvedColorMode = Exclude<ColorMode, "system">;
export type Appearance = { colorTheme: ColorTheme; colorMode: ColorMode; customAccent?: string };
type AppearanceInput = { colorTheme?: string; colorMode?: string; customAccent?: string };
export const resolvedColorMode = ref<ResolvedColorMode>("light");

function contrastText(color: string): string {
  const channels = [1, 3, 5].map((i) => parseInt(color.slice(i, i + 2), 16) / 255).map((v) => v <= .04045 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4);
  const luminance = channels[0]! * .2126 + channels[1]! * .7152 + channels[2]! * .0722;
  return luminance > .179 ? "#111111" : "#ffffff";
}

export function resolveColorMode(mode: ColorMode): ResolvedColorMode {
  if (mode !== "system") return mode;
  return typeof window !== "undefined" && window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function normalizeAppearance(value: AppearanceInput, toggleMode = false): Appearance {
  const colorTheme = colorThemes.includes(value.colorTheme as ColorTheme) ? value.colorTheme as ColorTheme : "teal";
  const colorMode = colorModes.includes(value.colorMode as ColorMode) ? value.colorMode as ColorMode : "system";
  return { colorTheme, colorMode: toggleMode ? resolveColorMode(colorMode) === "light" ? "dark" : "light" : colorMode,
    ...(colorTheme === "custom" ? { customAccent: /^#[0-9a-f]{6}$/i.test(value.customAccent ?? "") ? value.customAccent!.toLowerCase() : "#b83e49" } : {}) };
}

export function applyAppearance(value: AppearanceInput): Appearance {
  const appearance = normalizeAppearance(value);
  document.documentElement.dataset.colorTheme = appearance.colorTheme;
  resolvedColorMode.value = resolveColorMode(appearance.colorMode);
  document.documentElement.dataset.colorMode = resolvedColorMode.value;
  if (appearance.colorTheme === "custom") {
    const color = appearance.customAccent!;
    const dark = resolvedColorMode.value === "dark";
    const secondary = "#" + [1, 3, 5].map((i) => Math.round(parseInt(color.slice(i, i + 2), 16) * (dark ? .35 : .4) + (dark ? 255 * .65 : 0)).toString(16).padStart(2, "0")).join("");
    document.documentElement.style.setProperty("--custom-accent", color);
    document.documentElement.style.setProperty("--custom-on-primary", contrastText(color));
    document.documentElement.style.setProperty("--custom-primary-dark", secondary);
    document.documentElement.style.setProperty("--custom-on-hover", contrastText(secondary));
  }
  return appearance;
}

export function watchSystemAppearance(currentAppearance: () => Appearance): () => void {
  const query = window.matchMedia("(prefers-color-scheme: dark)");
  const update = () => {
    const appearance = currentAppearance();
    if (appearance.colorMode === "system") applyAppearance(appearance);
  };
  query.addEventListener("change", update);
  return () => query.removeEventListener("change", update);
}

let activeTransition: ViewTransition | undefined;

export async function animateAppearance(value: AppearanceInput, origin?: { x: number; y: number }): Promise<void> {
  const appearance = normalizeAppearance(value);
  const root = document.documentElement;
  activeTransition?.skipTransition();
  if (!document.startViewTransition || window.matchMedia("(prefers-reduced-motion: reduce)").matches
    || (root.dataset.colorMode === resolveColorMode(appearance.colorMode) && root.dataset.colorTheme === appearance.colorTheme)) {
    applyAppearance(appearance);
    return;
  }

  const anchor = document.activeElement === document.body
    ? document.querySelector(".floating-theme-toggle") : document.activeElement;
  const rect = anchor?.getBoundingClientRect();
  const x = Math.max(0, Math.min(window.innerWidth, origin?.x ?? (rect ? rect.left + rect.width / 2 : window.innerWidth / 2)));
  const y = Math.max(0, Math.min(window.innerHeight, origin?.y ?? (rect ? rect.top + rect.height / 2 : window.innerHeight / 2)));
  const radius = Math.ceil(Math.hypot(Math.max(x, window.innerWidth - x), Math.max(y, window.innerHeight - y)));
  root.style.setProperty("--theme-reveal-x", `${x}px`);
  root.style.setProperty("--theme-reveal-y", `${y}px`);
  root.style.setProperty("--theme-reveal-radius", `${radius}px`);
  root.dataset.themeTransition = resolveColorMode(appearance.colorMode);
  let transition: ViewTransition | undefined;
  try {
    transition = document.startViewTransition(async () => {
      applyAppearance(appearance);
      await nextTick();
    });
    activeTransition = transition;
    await transition.finished;
  } catch {
    // A skipped or unsupported animation must not prevent the theme from applying.
    if (!transition || activeTransition === transition) applyAppearance(appearance);
  } finally {
    if (!transition || activeTransition === transition) {
      activeTransition = undefined;
      delete root.dataset.themeTransition;
      for (const key of ["--theme-reveal-x", "--theme-reveal-y", "--theme-reveal-radius"]) root.style.removeProperty(key);
    }
  }
}
