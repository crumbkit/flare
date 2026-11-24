// The `send` a composer calls, built from a TanStack Query mutation. The
// mutation throws an ApiError for every failure (BASE-23); `send` maps a
// refusal and a full flare back into the composer's result union, and lets
// every other failure reject, which the composer shows as no answer with
// Retry. A retried draft keeps its key, so the server stores it once.
import type { Draft, Send } from "@flare/headless/comments/composer";
import type { CommentId, Posted } from "@flare/headless/comments/thread";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ApiError } from "../api/client.ts";
import { commentsQuery, postComment } from "../api/comments.ts";

/**
 * The send of one field on `flare`: a reply to `parent`, or a comment on the
 * page when `parent` is undefined. `accepts` checks that the server stored
 * the kind of post the field writes.
 */
export function useSend<P extends Posted>(
  flare: string,
  parent: CommentId | undefined,
  accepts: (posted: Posted) => posted is P,
): Send<P> {
  const client = useQueryClient();
  const { mutateAsync } = useMutation({
    mutationFn: (draft: Draft) =>
      postComment({
        flare,
        ...(parent === undefined ? {} : { parent }),
        body: draft.text,
        key: draft.key,
      }),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: commentsQuery(flare).queryKey });
    },
  });
  return async (draft) => {
    try {
      const posted = await mutateAsync(draft);
      if (!accepts(posted)) throw new ApiError("malformed");
      return { outcome: "sent", posted };
    } catch (error) {
      // A full flare refuses every new post, so the field keeps the text and
      // offers Copy, as it does for a refusal. The words name lost access,
      // because the headless core has one refused message; a flare holds
      // 2,000 posts before this happens.
      if (error instanceof ApiError && (error.kind === "refused" || error.kind === "full")) {
        return { outcome: "refused" };
      }
      throw error;
    }
  };
}
