import type { AppSettings } from "./settings";

export interface SettingsChanges {
  automation: boolean;
  device: boolean;
  library: boolean;
  storage: boolean;
  appearance: boolean;
}

export function emptySettingsChanges(): SettingsChanges {
  return {automation:false,device:false,library:false,storage:false,appearance:false};
}

export function settingsChanges(before: AppSettings, after: AppSettings): SettingsChanges {
  return {
    automation: before.autoBackupDelaySeconds !== after.autoBackupDelaySeconds || before.retentionCount !== after.retentionCount,
    device: false,
    library: false,
    storage: before.recycleBinPath !== after.recycleBinPath,
    appearance: before.colorTheme !== after.colorTheme || before.colorMode !== after.colorMode || before.customAccent !== after.customAccent,
  };
}
