// The comments section at the bottom of a shared page, as artboards 01 to 07
// of the comment parts mockup (LRJsZhYqKPHcug6pQKw8dA, version
// 1795398322-8ae4) draw it: the heading with the thread's count, every
// comment with its replies and a reply field, then the page field. On a
// reading page with an article, a comment on page text shows beside its text
// in the margin instead (Margin.tsx), and the section holds only the
// comments on the whole page, and on a phone those whose text is gone.
import type { ThreadLoad } from "@flare/headless/comments/thread";
import { useThread } from "@flare/react/useThread";
import { useQuery } from "@tanstack/react-query";
import { ApiError } from "../api/client.ts";
import { commentsQuery } from "../api/comments.ts";
import { CommentCard } from "./cards.tsx";
import { PageField } from "./fields.tsx";
import type { ReadingHost } from "./host.ts";
import { Margin, usePlacement } from "./Margin.tsx";

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

export interface SectionProps {
  /** The share whose comments the section shows. */
  readonly flare: string;
  /** The share's owner, whom a refused reader asks for access. */
  readonly owner: string;
  readonly now?: () => number;
  readonly timeZone?: string;
  /** The reading page's article and margin, where comments on its text show. */
  readonly page?: ReadingHost;
}

export function CommentsSection({ flare, owner, now = Date.now, timeZone, page }: SectionProps) {
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
  const placement = usePlacement(flare, page, view.state === "ready" ? view.thread : undefined);
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
      {view.state === "ready" && page !== undefined && (
        <Margin
          page={page}
          placement={placement}
          flare={flare}
          owner={owner}
          clock={clock}
          onAdd={thread.add}
        />
      )}
      {view.state === "ready" && (
        <>
          {placement.bottom.map((comment) => (
            <CommentCard
              key={comment.id}
              comment={comment}
              flare={flare}
              owner={owner}
              clock={clock}
              onReply={thread.add}
              card={placement.cards.get(comment.id)}
              viewed={page?.version}
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
