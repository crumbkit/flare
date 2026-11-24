import { readFile } from "node:fs/promises";
import { join } from "node:path";
import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

const ORIGIN = "http://flare.test";
const PAGE = `${ORIGIN}/s/sample`;
const sample = process.env.FLARE_READING_SAMPLE ?? join(import.meta.dirname, "../../sample");
const assets = join(import.meta.dirname, "../../dist/assets");
const fonts = join(import.meta.dirname, "../../../../crates/flare-server/fonts");

/**
 * Serves the sample page, the stylesheet, the built files and the fonts that
 * flare-server vendors from one origin.
 */
async function serve(page: Page): Promise<void> {
  const policy = (await readFile(join(sample, "policy.txt"), "utf8")).trim();
  await page.route(`${ORIGIN}/**`, async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === "/s/sample") {
      await route.fulfill({
        contentType: "text/html; charset=utf-8",
        headers: { "Content-Security-Policy": policy },
        body: await readFile(join(sample, "page.html"), "utf8"),
      });
    } else if (path === "/flare/style.css") {
      await route.fulfill({
        contentType: "text/css; charset=utf-8",
        body: await readFile(join(sample, "style.css"), "utf8"),
      });
    } else if (
      path.startsWith("/flare/assets/") &&
      path.endsWith(".woff2") &&
      !path.includes("..")
    ) {
      await route.fulfill({
        contentType: "font/woff2",
        body: await readFile(join(fonts, path.slice("/flare/assets/".length))),
      });
    } else if (path.startsWith("/flare/assets/") && !path.includes("..")) {
      await route.fulfill({
        contentType: "text/javascript; charset=utf-8",
        body: await readFile(join(assets, path.slice("/flare/assets/".length))),
      });
    } else {
      await route.fulfill({ status: 404, body: "" });
    }
  });
}

test("a reader opens artboard 01's sample share with its diagram, caption and coloured code", async ({
  page,
}, info) => {
  const violations: string[] = [];
  const errors: string[] = [];
  await page.exposeFunction("reportViolation", (text: string) => violations.push(text));
  await page.addInitScript(() => {
    document.addEventListener("securitypolicyviolation", (event) => {
      const report = Reflect.get(window, "reportViolation");
      if (typeof report === "function") {
        report(`${event.effectiveDirective} blocked ${event.blockedURI}`);
      }
    });
  });
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") {
      errors.push(message.text());
    }
  });
  await serve(page);

  await page.goto(PAGE);
  await expect(page.locator("body")).toHaveAttribute("data-reading", "done", { timeout: 30_000 });

  await expect(page.getByRole("heading", { level: 1 })).toHaveText(
    "Weather holds for the island ferry",
  );
  await expect(page.locator(".share-meta")).toContainText("Shared by alice@example.com with you");
  await expect(page.locator(".bar")).toHaveText(/flare.*BO/s);
  const figure = page.locator("figure.diagram");
  await expect(figure).toHaveAttribute("data-diagram", "drawn");
  await expect(figure.locator("figcaption")).toHaveText(
    "Figure 1. What happens to one sailing when a forecast arrives.",
  );
  const drawing = figure.locator(".mm svg").filter({ visible: true });
  await expect(drawing).toHaveCount(1);
  // Figure 1 draws the mockup's five nodes, and its 14 px labels show at
  // about 11 px, as in the mockup, once the drawing is scaled into the
  // column. On a phone the drawing keeps that size and scrolls.
  await expect(drawing.locator("g.node")).toHaveText([
    "Forecast gustover the limit?",
    "Sailing stays on",
    "Hold the sailing",
    "Confirmed in40 minutes?",
    "Cancel and rebook",
  ]);
  const label = await drawing.evaluate((svg) => {
    const width = svg instanceof SVGSVGElement ? svg.viewBox.baseVal.width : 0;
    return (14 * svg.getBoundingClientRect().width) / width;
  });
  expect(label).toBeGreaterThanOrEqual(10.5);
  await expect(page.locator(".code pre.shiki")).toHaveCount(1);
  await expect(page.locator(".code button[data-copy]")).toBeVisible();
  // The code text is .92em of the block's 13 px, as the mockup draws it.
  await expect(page.locator(".code pre.shiki > code")).toHaveCSS("font-size", "11.96px");

  const table = page.locator(".doc > .scroll").first();
  if (info.project.name === "phone") {
    const widths = await table.evaluate((box) => ({
      scroll: box.scrollWidth,
      client: box.clientWidth,
      page: document.documentElement.scrollWidth,
      viewport: window.innerWidth,
    }));
    expect(widths.scroll).toBeGreaterThan(widths.client);
    expect(widths.page).toBeLessThanOrEqual(widths.viewport);
  }

  const axe = await new AxeBuilder({ page }).analyze();
  expect(
    axe.violations.flatMap((violation) =>
      violation.nodes.map(
        (node) => `${violation.id}: ${node.target.join(" ")}: ${node.failureSummary ?? ""}`,
      ),
    ),
  ).toEqual([]);
  expect(violations).toEqual([]);
  expect(errors).toEqual([]);
  await page.screenshot({
    path: info.outputPath(`reading-${info.project.name}.png`),
    fullPage: true,
  });
});
