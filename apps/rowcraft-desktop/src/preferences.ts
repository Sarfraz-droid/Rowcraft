export type EditorPreferences = { fontSize: number; lineWrapping: boolean };
export const EDITOR_PREFERENCES_KEY = "rowcraft.editor.v1";
export const DEFAULT_EDITOR_PREFERENCES: EditorPreferences = { fontSize: 13, lineWrapping: false };
export const EDITOR_FONT_SIZES = [11, 12, 13, 14, 15, 16, 18, 20] as const;

export function loadEditorPreferences(): EditorPreferences {
  try {
    const saved = JSON.parse(localStorage.getItem(EDITOR_PREFERENCES_KEY) ?? "null");
    return {
      fontSize: EDITOR_FONT_SIZES.includes(saved?.fontSize) ? saved.fontSize : DEFAULT_EDITOR_PREFERENCES.fontSize,
      lineWrapping: typeof saved?.lineWrapping === "boolean" ? saved.lineWrapping : DEFAULT_EDITOR_PREFERENCES.lineWrapping,
    };
  } catch { return { ...DEFAULT_EDITOR_PREFERENCES }; }
}

/** Physical pointer coordinates need converting to app coordinates when zoomed. */
export function getAppScale(): number {
  return Number.parseFloat(document.documentElement.style.getPropertyValue("zoom")) || 1;
}
