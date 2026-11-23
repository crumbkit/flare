import { describe, expectTypeOf, it } from "vitest";
import { reply, replyId } from "./testThread.ts";
import type { Comment, CommentId, Reply } from "./thread.ts";

describe("thread types", () => {
  it("a reply has no replies of its own", () => {
    expectTypeOf<Reply>().not.toHaveProperty("replies");
    expectTypeOf<Comment["replies"][number]>().toEqualTypeOf<Reply>();
  });

  it("a reply cannot name another reply as its parent", () => {
    const parent = replyId("r1");
    // @ts-expect-error A reply id is not a comment id, so a reply to a reply does not type-check.
    const nested: Reply = { ...reply("r2", "c1", { name: "Ana Okafor" }, "x"), parent };
    expectTypeOf(nested.parent).toEqualTypeOf<CommentId>();
  });

  it("a reply cannot be placed where a comment belongs", () => {
    expectTypeOf<Reply>().not.toExtend<Comment>();
  });
});
