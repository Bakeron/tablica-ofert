import type { SettingsSection } from "../components/SettingsDialog";

/** Where the user was in the Settings window: kept until the app closes, never saved. */
export const settingsMemory = {
  section: null as SettingsSection | null,
  scroll: 0,
  /** Unfolded parts (offer sources, AI providers, instructions). */
  unfolded: new Set<string>(),
};

export function rememberUnfolded(id: string, open: boolean) {
  if (open) settingsMemory.unfolded.add(id);
  else settingsMemory.unfolded.delete(id);
}
