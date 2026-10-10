import { describe, expect, it } from "vitest";
import { defaultAppSettings } from "./settings";
import { settingsChanges } from "./settingsChanges";

describe("settings refresh scopes", () => {
  it("does not refresh repository or automation for language, notification or color changes", () => {
    expect(settingsChanges(defaultAppSettings, {...defaultAppSettings, language:"en", notifications:false, colorTheme:"indigo"})).toEqual({automation:false,device:false,library:false,storage:false,appearance:true});
  });
  it("refreshes automation for retention and debounce only", () => {
    expect(settingsChanges(defaultAppSettings, {...defaultAppSettings, retentionCount:5})).toEqual({automation:true,device:false,library:false,storage:false,appearance:false});
    expect(settingsChanges(defaultAppSettings, {...defaultAppSettings, autoBackupDelaySeconds:17}).automation).toBe(true);
  });
  it("refreshes capacity after the recycle path changes without restarting watchers", () => {
    expect(settingsChanges(defaultAppSettings, {...defaultAppSettings, recycleBinPath:"D:/recycle"})).toEqual({automation:false,device:false,library:false,storage:true,appearance:false});
  });
});
