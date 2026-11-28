import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { ApiError } from "./api/client.ts";
import { readingHost } from "./comments/host.ts";
import { CommentsSection } from "./comments/Section.tsx";

// The comments entry, which every reading page loads. It draws the comments
// section in place of the placeholder the server wrote after the article,
// reading the share and its owner from the placeholder's data attributes,
// and the cards on the article's text in a margin beside it. A page without
// the placeholder is left alone.

/** How many times a query that got no answer is asked again. */
const RETRIES = 2;

// Only a request that got no answer is retried, at most twice. A refused or
// malformed answer would come back the same (BASE-23).
const client = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: (count, error) =>
        count < RETRIES && error instanceof ApiError && error.kind === "no-answer",
    },
  },
});

/** The reading page's article and margin, absent on a page with no Markdown article. */
function pageOf(document: Document) {
  const page = readingHost(document);
  return page === undefined ? {} : { page };
}

const placeholder = document.querySelector<HTMLElement>("section#comments[data-flare]");
const flare = placeholder?.dataset.flare;
const owner = placeholder?.dataset.owner;
if (placeholder && flare && owner) {
  createRoot(placeholder).render(
    <StrictMode>
      <QueryClientProvider client={client}>
        <CommentsSection flare={flare} owner={owner} {...pageOf(document)} />
      </QueryClientProvider>
    </StrictMode>,
  );
}
