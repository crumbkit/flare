// The comments resource: the schemas of the contract's comment types
// (schema/flare-api.schema.json), the query of one flare's thread and the
// write of a comment or a reply. Both turn the contract's form into the
// headless core's thread, with each author named by their identifier.
import {
  type Comment,
  isCommentId,
  isReplyId,
  type Posted,
  type Reply,
  type Thread,
} from "@flare/headless/comments/thread";
import { queryOptions } from "@tanstack/react-query";
import { z } from "zod";
import { ApiError, request } from "./client.ts";

// zod compiles a faster parser with `new Function` unless it runs jitless.
// The reading page's policy has no 'unsafe-eval', so the attempt would be
// reported as a policy violation; jitless parsing never makes it.
z.config({ jitless: true });

/** The text a deleted comment shows when replies still hang from it. */
export const deletedText = "Its author deleted this comment.";

const commentId = z.string().refine(isCommentId);
const replyId = z.string().refine(isReplyId);

export const wireReply = z.object({
  id: replyId,
  flare: z.string(),
  parent: commentId,
  author: z.string(),
  at: z.string(),
  edited: z.string().nullish(),
  deleted: z.boolean(),
  body: z.string().nullish(),
});

export const wireComment = z.object({
  id: commentId,
  flare: z.string(),
  author: z.string(),
  at: z.string(),
  edited: z.string().nullish(),
  deleted: z.boolean(),
  body: z.string().nullish(),
  replies: z.array(wireReply),
});

export const wireThread = z.object({
  flare: z.string(),
  comments: z.array(wireComment),
});

export const wirePosted = z.discriminatedUnion("kind", [
  wireComment.extend({ kind: z.literal("comment") }),
  wireReply.extend({ kind: z.literal("reply") }),
]);

type WireReply = z.infer<typeof wireReply>;
type WireComment = z.infer<typeof wireComment>;

// The section shows no edited mark and no deleted reply: editing and
// deleting from the browser come in a later slice, and until then a
// deleted reply is left out and an edited post shows its current text.
function reply(wire: WireReply): Reply {
  return {
    kind: "reply",
    id: wire.id,
    parent: wire.parent,
    author: { name: wire.author },
    body: wire.body ?? "",
    at: wire.at,
  };
}

function comment(wire: WireComment): Comment | undefined {
  const replies = wire.replies.filter((each) => !each.deleted).map(reply);
  if (wire.deleted && replies.length === 0) return undefined;
  return {
    kind: "comment",
    id: wire.id,
    author: { name: wire.author },
    body: wire.deleted ? deletedText : (wire.body ?? ""),
    at: wire.at,
    replies,
  };
}

function thread(wire: z.infer<typeof wireThread>): Thread {
  return wire.comments.flatMap((each) => comment(each) ?? []);
}

/** The thread of flare `flare`, oldest comment first. */
export function commentsQuery(flare: string) {
  return queryOptions({
    queryKey: ["comments", "thread", flare],
    queryFn: async () =>
      thread(await request(`/api/comments?flare=${encodeURIComponent(flare)}`, wireThread)),
  });
}

/** What `POST /api/comments` takes: a comment, or a reply to `parent`. */
export interface NewComment {
  readonly flare: string;
  readonly parent?: string;
  readonly body: string;
  readonly key: string;
}

/** Writes a comment or a reply and returns it as the server stored it. */
export async function postComment(draft: NewComment): Promise<Posted> {
  const posted = await request("/api/comments", wirePosted, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(draft),
  });
  switch (posted.kind) {
    case "comment": {
      const stored = comment({ ...posted, replies: [] });
      if (stored === undefined) throw new ApiError("malformed");
      return stored;
    }
    case "reply":
      return reply(posted);
  }
}
