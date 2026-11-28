// Whether the page is narrow enough that cards on page text show under
// their paragraph, as artboards 09, 12 and 13 draw a phone. The width is
// the one at which flare-server's stylesheet puts the margin under the
// article: a 760-pixel box plus the page's padding on both sides.
import { useSyncExternalStore } from "react";

const NARROW = "(max-width: 824px)";

function query(): MediaQueryList | undefined {
  return typeof window.matchMedia === "function" ? window.matchMedia(NARROW) : undefined;
}

function subscribe(changed: () => void): () => void {
  const list = query();
  list?.addEventListener("change", changed);
  return () => list?.removeEventListener("change", changed);
}

export function useNarrow(): boolean {
  return useSyncExternalStore(subscribe, () => query()?.matches ?? false);
}
