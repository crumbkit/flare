// The `send` a composer calls, built from a TanStack Query mutation. The
// mutation throws an ApiError for every failure (BASE-23); `send` maps each
// answer the server would repeat (lost access, a full flare, an ended
// session, invalid text) back into the composer's result union, and lets
// every other failure reject, which the composer shows as no answer with
// Retry. A retried draft keeps its key, so the server stores it once.
import type { Draft, Refusal, Send } from "@flare/headless/comments/composer";
import type { CommentId, Posted } from "@flare/headless/comments/thread";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ApiError, type ApiFailure } from "../api/client.ts";
import { commentsQuery, postComment } from "../api/comments.ts";

/** The refusal that each failure the server would repeat becomes; the rest may be retried. */
const refusals: Partial<Record<ApiFailure, Refusal>> = {
  refused: "refused",
  full: "full",
  "signed-out": "signed-out",
  invalid: "invalid",
};

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
      const reason = error instanceof ApiError ? refusals[error.kind] : undefined;
      if (reason !== undefined) return { outcome: "refused", reason };
      throw error;
    }
  };
}
