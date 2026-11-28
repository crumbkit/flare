// The page text of one version of a share, which finding a comment's text
// on a later version compares with. The reading page of the version is the
// only place its rendered text exists, so the query reads that page and
// takes the text of its article as the open page does.
import { queryOptions } from "@tanstack/react-query";
import { readPage } from "../comments/pageDom.ts";
import { ApiError, requestText } from "./client.ts";

/** The page text of version `version` of share `flare`. */
export function versionTextQuery(flare: string, version: number) {
  return queryOptions({
    queryKey: ["comments", "version-text", flare, version],
    queryFn: async () => {
      const html = await requestText(
        `/s/${encodeURIComponent(flare)}?version=${encodeURIComponent(String(version))}`,
      );
      const article = new DOMParser()
        .parseFromString(html, "text/html")
        .querySelector("article.doc");
      if (article === null) throw new ApiError("malformed");
      return readPage(article).text.text;
    },
    // A version's text never changes once published.
    staleTime: Number.POSITIVE_INFINITY,
  });
}
