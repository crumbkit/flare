import { describe, expect, it } from "vitest";
import { initials, shownTime } from "./format.ts";

describe("initials", () => {
  it("come from the first two words of an identifier's local part, as the page shows the owner's", () => {
    expect(initials("priya.nair@example.com")).toBe("PN");
    expect(initials("ana_okafor@example.com")).toBe("AO");
    expect(initials("bob@example.com")).toBe("BO");
    expect(initials("x@example.com")).toBe("X");
    expect(initials("@example.com")).toBe("?");
  });
});

describe("the time of a post", () => {
  const now = Date.parse("2026-11-23T10:00:00Z");

  it("shows the day, the month and the time of day in the reader's zone", () => {
    expect(shownTime("2026-11-21T16:02:00Z", now, "UTC")).toBe("21 Nov, 16:02");
    expect(shownTime("2026-11-21T16:02:00Z", now, "Asia/Tokyo")).toBe("22 Nov, 01:02");
  });

  it("says just now for a post from the last minute", () => {
    expect(shownTime("2026-11-23T09:59:30Z", now, "UTC")).toBe("just now");
    expect(shownTime("2026-11-23T09:58:59Z", now, "UTC")).toBe("23 Nov, 09:58");
  });
});
