import { expect, open, test } from "nsh/test";

test("a masked input's text never shows, only its length", async () => {
  const app = await open("./fixtures/agent-masked.tsx", { width: 360, height: 640, clock: {} });
  await app.type("id:agent-secret", "hunter2");
  await app.waitFor("length:7");
  expect(app.texts().some((text) => text.includes("hunter2"))).toBe(false);
});
