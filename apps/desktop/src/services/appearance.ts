export const colorThemes = ["teal", "indigo", "violet", "amber", "rose", "gray"] as const;
export const colorModes = ["light", "dark"] as const;

export type ColorTheme = typeof colorThemes[number];
export type ColorMode = typeof colorModes[number];
export type Appearance = { colorTheme: ColorTheme; colorMode: ColorMode };
type AppearanceInput = { colorTheme?: string; colorMode?: string };

export function normalizeAppearance(value: AppearanceInput, toggleMode = false): Appearance {
  const colorTheme = colorThemes.includes(value.colorTheme as ColorTheme) ? value.colorTheme as ColorTheme : "teal";
  const colorMode = colorModes.includes(value.colorMode as ColorMode) ? value.colorMode as ColorMode : "light";
  return { colorTheme, colorMode: toggleMode ? colorMode === "light" ? "dark" : "light" : colorMode };
}

export function applyAppearance(value: AppearanceInput): Appearance {
  const appearance = normalizeAppearance(value);
  document.documentElement.dataset.colorTheme = appearance.colorTheme;
  document.documentElement.dataset.colorMode = appearance.colorMode;
  return appearance;
}
