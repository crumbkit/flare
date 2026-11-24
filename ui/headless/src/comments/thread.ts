// The comments on one version of a shared page. A thread has one level of
// replies: a comment holds replies, and a reply holds none and names its
// parent by a comment id, so the types cannot express a reply to a reply.

declare const commentIdBrand: unique symbol;
declare const replyIdBrand: unique symbol;

/** The id the server gives a top-level comment. */
export type CommentId = string & { readonly [commentIdBrand]: true };
/** The id the server gives a reply. It is never a comment id. */
export type ReplyId = string & { readonly [replyIdBrand]: true };

/** An app's schema brands a parsed id with these, for example in a zod refine. */
export function isCommentId(raw: string): raw is CommentId {
  return raw.length > 0;
}

export function isReplyId(raw: string): raw is ReplyId {
  return raw.length > 0;
}

export interface Author {
  readonly name: string;
}

export interface Reply {
  readonly kind: "reply";
  readonly id: ReplyId;
  readonly parent: CommentId;
  readonly author: Author;
  readonly body: string;
  /** The instant the server stored the reply, as an RFC 3339 string. */
  readonly at: string;
}

export interface Comment {
  readonly kind: "comment";
  readonly id: CommentId;
  readonly author: Author;
  /** The comment's text, which is empty once its author deleted it. */
  readonly body: string;
  /** The instant the server stored the comment, as an RFC 3339 string. */
  readonly at: string;
  /**
   * Whether its author deleted the comment. A thread holds a deleted comment
   * only while replies still hang from it.
   */
  readonly deleted: boolean;
  readonly replies: readonly Reply[];
}

export type Thread = readonly Comment[];

/** What a composer posts: a comment on the whole page, or a reply to one. */
export type Posted = Comment | Reply;

/** What a composer writes, and for a reply, whom it answers. */
export type Target = { readonly kind: "comment" } | { readonly kind: "reply"; readonly to: Author };

/** The thread as a page shows it while it loads. */
export type ThreadLoad =
  | { readonly state: "loading" }
  | { readonly state: "failed" }
  | { readonly state: "ready"; readonly thread: Thread };

/**
 * Adds a post to the end of its place in the thread. A post whose id the
 * thread already holds, such as one a refetch returned, is not added again,
 * and a reply whose parent the thread does not hold is dropped.
 */
export function withPosted(thread: Thread, posted: Posted): Thread {
  switch (posted.kind) {
    case "comment":
      return thread.some((c) => c.id === posted.id) ? thread : [...thread, posted];
    case "reply":
      return thread.map((c) =>
        c.id !== posted.parent || c.replies.some((r) => r.id === posted.id)
          ? c
          : { ...c, replies: [...c.replies, posted] },
      );
  }
}
