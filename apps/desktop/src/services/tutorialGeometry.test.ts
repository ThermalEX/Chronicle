import { expect, it } from "vitest";
import { placeTutorialCard } from "./tutorialGeometry";
it("keeps a card within a narrow viewport", () => {
  expect(placeTutorialCard({ left: 900, top: 650, right: 1000, bottom: 710 }, 1024, 720, 340, 190)).toEqual({ left: 672, top: 448 });
});
it("prefers the free space below the highlighted target", () => {
  expect(placeTutorialCard({ left: 30, top: 20, right: 140, bottom: 60 }, 1024, 720, 340, 190)).toEqual({ left: 30, top: 72 });
});
