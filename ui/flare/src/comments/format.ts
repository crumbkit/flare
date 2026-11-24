// How a comment's author and time show: the author's initials beside their
// identifier, as the reading page shows its owner, and the time of a post as
// the comment parts mockup draws it, such as "21 Nov, 16:02".

/**
 * Up to two capital letters for `identifier`: the first letters of the first
 * two words of its local part, or the first two letters of a one-word local
 * part. flare-server's reading.rs gives the owner's initials by the same rule.
 */
export function initials(identifier: string): string {
  const local = identifier.split("@")[0] ?? "";
  const words = local.split(/[._+-]/).filter((word) => word !== "");
  const [first, second] = words;
  if (first === undefined) return "?";
  const letters = second === undefined ? [...first].slice(0, 2) : [first[0], second[0]];
  return letters.join("").toUpperCase();
}

/** How long after a post its time still shows as "just now". */
const JUST_NOW_MS = 60_000;

/**
 * The time of a post stored at `at`, an RFC 3339 string, as a reader in
 * `timeZone` sees it at `now`. The default zone is the reader's own.
 */
export function shownTime(at: string, now: number, timeZone?: string): string {
  const time = Date.parse(at);
  if (now - time < JUST_NOW_MS) return "just now";
  const parts = new Intl.DateTimeFormat("en-GB", {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
    ...(timeZone === undefined ? {} : { timeZone }),
  }).formatToParts(time);
  const part = (type: Intl.DateTimeFormatPartTypes) =>
    parts.find((each) => each.type === type)?.value ?? "";
  return `${part("day")} ${part("month")}, ${part("hour")}:${part("minute")}`;
}
