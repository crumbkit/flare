// The comments section at the bottom of a shared page, as artboards 01 to 07
// of the comment parts mockup (LRJsZhYqKPHcug6pQKw8dA, version
// 1795398322-8ae4) draw it: the heading with the thread's count, every
// comment with its replies and a reply field, then the page field.
import type { Comment, Reply, ThreadLoad } from "@flare/headless/comments/thread";
import { useThread } from "@flare/react/useThread";
import { useQuery } from "@tanstack/react-query";
import { ApiError } from "../api/client.ts";
import { commentsQuery } from "../api/comments.ts";
import { PageField, ReplyField } from "./fields.tsx";
import { initials, shownTime } from "./format.ts";

/**
 * The line a reader sees when the server would give the same answer again,
 * so Retry would not help: a refusal or an ended session.
 */
function finalMessage(error: unknown, owner: string): string | undefined {
  if (!(error instanceof ApiError)) return undefined;
  switch (error.kind) {
    case "refused":
      return `The comments did not load because you no longer have access to this page. Ask ${owner} for access.`;
    case "signed-out":
      return "The comments did not load because your session ended. Reload the page to sign in again.";
    default:
      return undefined;
  }
}

interface Clock {
  /** The time now, in milliseconds since 1970. */
  readonly now: () => number;
  /** The zone times show in; the reader's own when undefined. */
  readonly timeZone: string | undefined;
}

function Byline({ post, clock }: { readonly post: Comment | Reply; readonly clock: Clock }) {
  return (
    <div className="c-head">
      <span className="who" aria-hidden="true">
        {initials(post.author.name)}
      </span>
      <b>{post.author.name}</b>
      <time dateTime={post.at}>{shownTime(post.at, clock.now(), clock.timeZone)}</time>
    </div>
  );
}

interface CardProps {
  readonly comment: Comment;
  readonly flare: string;
  readonly owner: string;
  readonly clock: Clock;
  readonly onReply: (posted: Reply) => void;
}

function CommentCard({ comment, flare, owner, clock, onReply }: CardProps) {
  return (
    <article className="card">
      <Byline post={comment} clock={clock} />
      <div className="c-body">{comment.body}</div>
      {comment.replies.map((reply) => (
        <div className="reply" key={reply.id}>
          <Byline post={reply} clock={clock} />
          <div className="c-body">{reply.body}</div>
        </div>
      ))}
      <ReplyField flare={flare} owner={owner} parent={comment} onSent={onReply} />
    </article>
  );
}

export interface SectionProps {
  /** The share whose comments the section shows. */
  readonly flare: string;
  /** The share's owner, whom a refused reader asks for access. */
  readonly owner: string;
  readonly now?: () => number;
  readonly timeZone?: string;
}

export function CommentsSection({ flare, owner, now = Date.now, timeZone }: SectionProps) {
  const query = useQuery(commentsQuery(flare));
  // A failed refetch keeps the data it had, so the thread and every draft
  // in its fields stay; only a first load that fails shows the failure.
  let load: ThreadLoad;
  if (query.data !== undefined) load = { state: "ready", thread: query.data };
  else if (query.isError && !query.isFetching) load = { state: "failed" };
  else load = { state: "loading" };
  const thread = useThread(load, () => {
    void query.refetch();
  });
  const final = load.state === "failed" ? finalMessage(query.error, owner) : undefined;
  const clock = { now, timeZone };
  const { view } = thread;
  return (
    <div className="thread" {...thread.sectionProps}>
      <div className="section-head">
        <h2 id="comments-heading">{thread.heading}</h2>
        {view.state === "loading" && <span className="status" {...view.statusProps} />}
        {view.state === "ready" && <span className="muted">{view.summary}</span>}
      </div>
      {view.state === "loading" && (
        <div className="card" aria-hidden="true">
          <div className="skeleton" style={{ width: "40%" }} />
          <div className="skeleton" />
          <div className="skeleton" style={{ width: "70%" }} />
        </div>
      )}
      {view.state === "failed" && final !== undefined && (
        <div className="alert red" role="alert">
          <p>{final}</p>
        </div>
      )}
      {view.state === "failed" && final === undefined && (
        <div className="alert amber" {...view.alertProps}>
          <p>{view.message}</p>
          <button className="ghost" {...view.retryProps} />
        </div>
      )}
      {view.state === "ready" && (
        <>
          {view.thread.map((comment) => (
            <CommentCard
              key={comment.id}
              comment={comment}
              flare={flare}
              owner={owner}
              clock={clock}
              onReply={thread.add}
            />
          ))}
          <PageField
            flare={flare}
            owner={owner}
            threadIsEmpty={view.thread.length === 0}
            onSent={thread.add}
          />
        </>
      )}
    </div>
  );
}
