import { expect, test } from "@playwright/test";

const viewports = [
  { width: 320, height: 720 },
  { width: 390, height: 844 },
  { width: 768, height: 900 },
  { width: 1024, height: 900 },
  { width: 1440, height: 900 },
];

async function settle(page: import("@playwright/test").Page) {
  await page.goto("/");
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(650);
}

test.describe("desktop", () => {
  test.use({ viewport: { width: 1440, height: 900 } });

  test("renders the honest story without console failures or external requests", async ({ page }) => {
    const consoleErrors: string[] = [];
    const pageErrors: string[] = [];
    const externalRequests: string[] = [];

    page.on("console", (message) => {
      if (message.type() === "error") consoleErrors.push(message.text());
    });
    page.on("pageerror", (error) => pageErrors.push(error.message));
    page.on("request", (request) => {
      const url = request.url();
      if (!url.startsWith("http://127.0.0.1:3100") && !url.startsWith("data:") && !url.startsWith("blob:")) {
        externalRequests.push(url);
      }
    });

    await settle(page);

    await expect(page.getByRole("heading", { level: 1 })).toHaveText(
      /Your voice,\s*rebuilt locally\./,
    );
    await expect(page.getByText("Under 500 ms is still out of reach.")).toBeVisible();
    await expect(page.getByText("Linux experimental")).toBeVisible();
    await expect(page.getByText("No public build / no account")).toBeVisible();

    await page.waitForTimeout(500);
    expect(pageErrors).toEqual([]);
    expect(consoleErrors).toEqual([]);
    expect(externalRequests).toEqual([]);
  });

  test("switches the phrase schematic and explains the tradeoff", async ({ page }) => {
    await settle(page);

    const speaking = page.getByRole("button", { name: "Speaking", exact: true });
    await speaking.click();

    await expect(speaking).toHaveAttribute("aria-pressed", "true");
    await expect(page.getByText(/DexVoice is playing the phrase/)).toHaveAttribute(
      "aria-live",
      "polite",
    );
    await expect(
      page.getByText("Interactive diagram. No audio is captured or played on this page."),
    ).toBeVisible();
  });

  test("operates the signal-path tabs by pointer and keyboard", async ({ page }) => {
    await settle(page);

    const mic = page.getByRole("tab", { name: /Mic/ });
    const phonon = page.getByRole("tab", { name: /Phonon/ });

    await mic.focus();
    await page.keyboard.press("ArrowRight");
    await expect(page.getByRole("tab", { name: /Parakeet/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await expect(page.getByRole("tabpanel")).toContainText("Parakeet commits words live");

    await phonon.click();
    await expect(phonon).toHaveAttribute("aria-selected", "true");
    await expect(page.getByRole("tabpanel")).toContainText("Phonon speaks it back");

    await page.waitForTimeout(650);
    const activeBox = await phonon.boundingBox();
    const inactiveBox = await mic.boundingBox();
    expect(activeBox?.width ?? 0).toBeGreaterThan((inactiveBox?.width ?? 0) + 24);

    await page.getByRole("tab", { name: /PipeWire/ }).click();
    await expect(page.getByRole("tabpanel")).toContainText(
      "Verify the app's microphone permissions",
    );
    await expect(page.getByRole("tabpanel")).not.toContainText("any other app");
  });

  test("copies the exact Linux command", async ({ page }) => {
    await settle(page);
    await page.locator("#setup").scrollIntoViewIfNeeded();

    const copy = page.getByRole("button", { name: /Copy: \.\/scripts\/install-models\.sh/ });
    await expect(copy).toBeVisible();
    await copy.click();

    await expect(page.locator('button[data-copied="true"]')).toContainText("Copied");
    const clipboard = await page.evaluate(() => navigator.clipboard.readText());
    expect(clipboard).toBe("./scripts/install-models.sh");
  });

  test("scopes the shutdown caveat and the re-synthesis limit", async ({ page }) => {
    await settle(page);

    await page.locator("#status").scrollIntoViewIfNeeded();
    const status = page.locator("#status");
    await expect(status.getByText("Shutdown cleanup")).toBeVisible();
    await expect(status.getByText(/deliberately closing the app/)).toBeVisible();
    await expect(status.getByText(/ordinary-operation crash/)).toBeVisible();
    await expect(status.getByText(/driver issue/i)).toHaveCount(0);

    const resynthesis = page.getByText("Re-synthesis, not a filter.");
    await resynthesis.scrollIntoViewIfNeeded();
    await expect(resynthesis).toBeVisible();
    await expect(page.getByText(/original emotion, timing, and delivery/)).toBeVisible();
    expect(await page.locator("audio, video").count()).toBe(0);
    expect(await page.getByRole("button", { name: /play/i }).count()).toBe(0);
  });

  test("pins the behavior chapter while the phrase resolves", async ({ page }) => {
    await settle(page);

    const sectionTop = await page
      .locator("#limits")
      .evaluate((section) => section.getBoundingClientRect().top + window.scrollY);

    await page.evaluate((top) => window.scrollTo(0, top + 420), sectionTop);
    await page.waitForTimeout(700);

    const pin = page.locator('[data-pin="behavior"]');
    await expect
      .poll(async () => pin.evaluate((element) => window.getComputedStyle(element).position))
      .toBe("fixed");
    await expect
      .poll(async () => pin.evaluate((element) => Math.round(element.getBoundingClientRect().top)))
      .toBe(0);

    const visibleWords = await page
      .locator(".behavior-word")
      .evaluateAll((words) =>
        words.filter((word) => Number(window.getComputedStyle(word).opacity) > 0.5).length,
      );
    expect(visibleWords).toBeGreaterThan(5);
  });

  test("keeps the hero on two lines at every supported width", async ({ page }) => {
    await settle(page);

    for (const viewport of viewports) {
      await page.setViewportSize(viewport);
      await page.waitForTimeout(120);

      const lineCounts = await page.locator("h1").evaluate((heading) => {
        const spans = Array.from(heading.querySelectorAll<HTMLElement>(".hero-title-line"));
        return spans.map((span) => {
          const range = document.createRange();
          range.selectNodeContents(span);
          return new Set(Array.from(range.getClientRects()).map((rect) => Math.round(rect.top))).size;
        });
      });

      expect(lineCounts, `hero line count at ${viewport.width}px`).toEqual([1, 1]);
    }
  });

  test("has no horizontal page overflow at supported widths", async ({ page }) => {
    await settle(page);

    for (const viewport of viewports) {
      await page.setViewportSize(viewport);
      await page.evaluate(() => window.scrollTo(0, 0));
      await page.waitForTimeout(180);

      const metrics = await page.evaluate(() => ({
        scrollWidth: document.documentElement.scrollWidth,
        clientWidth: document.documentElement.clientWidth,
        bodyScrollWidth: document.body.scrollWidth,
      }));

      expect(metrics.scrollWidth, `document width at ${viewport.width}px`).toBeLessThanOrEqual(
        metrics.clientWidth + 1,
      );
      expect(metrics.bodyScrollWidth, `body width at ${viewport.width}px`).toBeLessThanOrEqual(
        metrics.clientWidth + 1,
      );
    }
  });
});

test.describe("mobile", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test("opens the mobile menu, focuses the first link, and closes on escape", async ({ page }) => {
    await settle(page);

    const menu = page.getByRole("button", { name: "Open navigation menu" });
    await menu.click();

    const panel = page.getByRole("navigation", { name: "Mobile" });
    await expect(page.getByRole("button", { name: "Close navigation menu" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
    await expect(panel.getByRole("link", { name: "Signal path" })).toBeVisible();
    await expect(panel.getByRole("link", { name: "Signal path" })).toBeFocused();

    await page.keyboard.press("Escape");
    await expect(page.getByRole("button", { name: "Open navigation menu" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });

  test("navigates from the mobile disclosure and copies on a narrow screen", async ({ page }) => {
    await settle(page);

    await page.getByRole("button", { name: "Open navigation menu" }).click();
    await page
      .getByRole("navigation", { name: "Mobile" })
      .getByRole("link", { name: "Linux", exact: true })
      .click();
    await expect(page).toHaveURL(/#setup$/);

    const copy = page.getByRole("button", { name: /Copy: \.\/scripts\/setup-virtual-mic\.sh/ });
    await copy.scrollIntoViewIfNeeded();
    await copy.click();
    await expect(page.locator('button[data-copied="true"]')).toContainText("Copied");
    const clipboard = await page.evaluate(() => navigator.clipboard.readText());
    expect(clipboard).toBe("./scripts/setup-virtual-mic.sh");
  });

  test("keeps the stage rail usable on a small screen", async ({ page }) => {
    await settle(page);
    await page.locator("#pipeline").scrollIntoViewIfNeeded();

    const phonon = page.getByRole("tab", { name: /Phonon/ });
    await phonon.click();
    await expect(phonon).toHaveAttribute("aria-selected", "true");
    await expect(page.getByRole("tabpanel")).toContainText("Phonon speaks it back");
  });

  test("wraps a long setup command instead of hiding it", async ({ page }) => {
    await settle(page);
    await page.locator("#setup").scrollIntoViewIfNeeded();

    const code = page.locator("#setup code").nth(1);
    const metrics = await code.evaluate((element) => ({
      scrollWidth: element.scrollWidth,
      clientWidth: element.clientWidth,
      text: element.textContent,
    }));

    expect(metrics.text).toBe("./scripts/install-models.sh");
    expect(metrics.scrollWidth).toBeLessThanOrEqual(metrics.clientWidth + 1);
  });
});

test.describe("reduced motion", () => {
  test.use({ viewport: { width: 1440, height: 900 }, reducedMotion: "reduce" });

  test("shows the full phrase immediately and removes the scroll cue", async ({ page }) => {
    await settle(page);
    await page.locator("#limits").scrollIntoViewIfNeeded();
    await page.waitForTimeout(300);

    const opacities = await page
      .locator(".behavior-word")
      .evaluateAll((words) => words.map((word) => window.getComputedStyle(word).opacity));

    expect(new Set(opacities)).toEqual(new Set(["1"]));
    await expect(page.locator(".behavior-scroll-cue")).toBeHidden();
    await expect(page.getByText("It waits for the pause.", { exact: false })).toBeVisible();
  });
});
