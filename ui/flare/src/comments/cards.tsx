// The card of one comment, as the comment parts mockup draws it: in
// Comments at the end of the page (artboards 02 to 07 and 13) and beside the
// page text (artboards 08 to 12). A comment on page text shows its quote,
// and on a later version a line that its text moved or changed. A deleted
// comment that still has replies keeps its place as a line saying so, with
// no author or time.
import { deletedText } from "@flare/headless/comments/a11y";
import type { Card } from "@flare/headless/comments/margin";
import { commentedOn, movedNote, outdatedNote, quoteText } from "@flare/headless/comments/passage";
import type { Comment, LiveComment, Reply } from "@flare/headless/comments/thread";
import { ReplyField } from "./fields.tsx";
import { initials, shownTime } from "./format.ts";

export interface Clock {
  /** The time now, in milliseconds since 1970. */
  readonly now: () => number;
  /** The zone times show in; the reader's own when undefined. */
  readonly timeZone: string | undefined;
}

function Byline({ post, clock }: { readonly post: LiveComment | Reply; readonly clock: Clock }) {
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

/** The lines above a comment's byline that say what text it is about. */
function Quoted({ card, viewed }: { readonly card: Card; readonly viewed: number }) {
  const quote = <div className="quote">{quoteText(card.anchor.quote)}</div>;
  switch (card.state) {
    case "anchored":
    case "unplaced":
      return quote;
    case "re-anchored":
      return (
        <>
          {quote}
          <div className="a-note">{movedNote(card.anchor.version)}</div>
        </>
      );
    case "outdated":
      return (
        <>
          <div className="a-note outdated">{outdatedNote(viewed)}</div>
          <div className="lab">{commentedOn(card.anchor.version)}</div>
          {quote}
        </>
      );
  }
}

export interface CardProps {
  readonly comment: Comment;
  readonly flare: string;
  readonly owner: string;
  readonly clock: Clock;
  readonly onReply: (posted: Reply) => void;
  /** Where the comment's text is on the viewed version, for a comment on page text. */
  readonly card?: Card | undefined;
  /** The version the page shows. */
  readonly viewed?: number | undefined;
}

export function CommentCard({ comment, flare, owner, clock, onReply, card, viewed }: CardProps) {
  return (
    <article className="card">
      {card !== undefined && viewed !== undefined && <Quoted card={card} viewed={viewed} />}
      {comment.deleted ? (
        <div className="c-body deleted">{deletedText}</div>
      ) : (
        <>
          <Byline post={comment} clock={clock} />
          <div className="c-body">{comment.body}</div>
        </>
      )}
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
