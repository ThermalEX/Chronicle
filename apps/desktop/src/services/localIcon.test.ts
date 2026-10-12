import { expect, it } from "vitest";
import { currentAppIcon, localIcon } from "./localIcon";
import defaultIcon from "../assets/icon.png";
it("uses a custom icon image but falls back to the Chronicle icon when unavailable", () => {
  localIcon.value = { mode: "image", imageDataUrl: "data:image/png;base64,AA==" };
  expect(currentAppIcon.value).toBe("data:image/png;base64,AA==");
  localIcon.value = { mode: "color", warning: "image-unreadable" };
  expect(currentAppIcon.value).toBe(defaultIcon);
});
