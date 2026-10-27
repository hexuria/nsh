import { expect, open, test } from "nsh/test";

const fixture = "./fixtures/agent-negative.tsx";

test("agent negative fixture exposes deterministic targets", async () => {
  const app = await open(fixture, { width: 360, height: 640, clock: {} });
  await app.waitFor("presses:0 value: scrolls:0");
  expect(app.texts()).toContain("presses:0 value: scrolls:0");
  expect(app.texts()).toContain("covered target");
  expect(app.texts()).toContain("cover");
  expect(app.texts()).toContain("disabled target");
});
