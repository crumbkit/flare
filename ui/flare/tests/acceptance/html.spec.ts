import { expect, type Page, test } from "@playwright/test";

// A signed-in reader opens an uploaded HTML share. The reading page frames
// the file from the content host in a sandbox without allow-same-origin.
// The file is a Compliance page that `flightdeck compliance build` wrote
// (tests/fixtures/compliance/compliance.html): one file that runs its own
// scripts and loads nothing outside itself. A flare-server test in
// crates/flare-server/tests/server/html.rs starts the server, uploads the
// file and gives the origin, the reader's session cookie and the share's id.
function setting(name: string): string {
  const value = process.env[name];
  if (value === undefined || value === "") throw new Error(`${name} is not set`);
  return value;
}

const origin = setting("FLARE_JOURNEY_ORIGIN");
const cookie = setting("FLARE_JOURNEY_COOKIE");
const share = setting("FLARE_JOURNEY_SHARE");

/** Collects Content-Security-Policy violations and logged errors in every frame. */
async function watch(page: Page): Promise<{ violations: string[]; errors: string[] }> {
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
    if (message.type() === "error") errors.push(message.text());
  });
  return { violations, errors };
}

test("an uploaded Compliance page draws in the sandboxed frame with no policy violation", async ({
  page,
  context,
  browserName,
}) => {
  // A `__Host-` cookie cannot be set for an http origin, so the test sends
  // the session cookie itself. Chromium applies extra headers to every
  // request. WebKit on Linux does not apply them to a navigation, so the
  // test adds the cookie to requests for the site origin there. The content
  // host is another origin and reads no cookie.
  if (browserName === "webkit") {
    await context.route(`${origin}/**`, async (route) => {
      const response = await route.fetch({
        headers: { ...route.request().headers(), cookie },
      });
      await route.fulfill({ response });
    });
  } else {
    await context.setExtraHTTPHeaders({ Cookie: cookie });
  }
  const { violations, errors } = await watch(page);

  await page.goto(`${origin}/s/${share}`);
  const frame = page.locator("iframe");
  await expect(frame).toHaveAttribute("sandbox", "allow-scripts");
  await expect(frame).toHaveAttribute("referrerpolicy", "no-referrer");
  const inner = page.frameLocator("iframe");
  await expect(inner.getByText("6 checks fail in 3 repositories")).toBeVisible();

  // Drawing the file breaks no policy of its own.
  expect(violations).toEqual([]);
  expect(errors).toEqual([]);

  const content = page.frames().find((each) => each !== page.mainFrame());
  if (content === undefined) throw new Error("the page has no frame");
  expect(new URL(content.url()).origin).not.toBe(origin);
  // The sandbox gives the file an opaque origin that reaches no cookie.
  expect(await content.evaluate(() => window.origin)).toBe("null");
  const cookieRead = await content.evaluate(() => {
    try {
      return document.cookie;
    } catch (error) {
      return error instanceof Error ? error.name : "thrown";
    }
  });
  expect(cookieRead).toBe("SecurityError");
  const reached = await content.evaluate(async (target) => {
    try {
      // biome-ignore lint/style/noRestrictedGlobals: the probe is the frame's own script trying to reach flare
      const reply = await fetch(`${target}/api/shares?scope=mine`);
      return String(reply.status);
    } catch {
      return "blocked";
    }
  }, origin);
  expect(reached).toBe("blocked");

  // The one violation after the file drew is the probe's own request.
  expect(violations).toEqual([`connect-src blocked ${origin}/api/shares?scope=mine`]);
});
