// Builders for the core's tests. Nothing outside the tests imports this file.
import {
  type Author,
  type Comment,
  type CommentId,
  isCommentId,
  isReplyId,
  type Reply,
  type ReplyId,
} from "./thread.ts";

export function commentId(raw: string): CommentId {
  if (!isCommentId(raw)) throw new Error(`not a comment id: ${raw}`);
  return raw;
}

export function replyId(raw: string): ReplyId {
  if (!isReplyId(raw)) throw new Error(`not a reply id: ${raw}`);
  return raw;
}

export const tomas: Author = { name: "Tomás Rivera" };
export const ana: Author = { name: "Ana Okafor" };
export const jonas: Author = { name: "Jonas Weber" };
export const priya: Author = { name: "Priya Nair" };

export function reply(id: string, parent: string, author: Author, body: string): Reply {
  return {
    kind: "reply",
    id: replyId(id),
    parent: commentId(parent),
    author,
    body,
    at: "2026-11-21T16:30:00Z",
  };
}

export function comment(
  id: string,
  author: Author,
  body: string,
  replies: readonly Reply[] = [],
): Comment {
  return {
    kind: "comment",
    id: commentId(id),
    author,
    body,
    at: "2026-11-21T16:02:00Z",
    deleted: false,
    replies,
  };
}

// The thread of the mockup's artboard 02: two comments, two replies.
export function mockupThread(): readonly Comment[] {
  return [
    comment("c1", tomas, "Can the table also show which vessels carry vehicles?", [
      reply("r1", "c1", ana, "Yes. All three do."),
      reply("r2", "c1", jonas, "I added the deck plans in my response above."),
    ]),
    comment("c2", jonas, "Do we pay for a night's stay?"),
  ];
}
