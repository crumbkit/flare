//! Runs the comment and cursor store suites against the in-memory stores,
//! so each suite is known to pass on a correct implementation.
#![cfg(test)]

use flare_core::contract::comment_store::{self, CommentFixture};
use flare_core::contract::cursor_store::{self, CursorFixture};
use flare_core::contract::{Threads, block_on};
use flare_core::memory::{MemoryCommentStore, MemoryCursorStore};

macro_rules! suite {
    ($module:ident, $store:ident, $fixture:ident; $($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            let fixture = $fixture::standard().unwrap();
            block_on($module::$name(&$store::new(), &fixture)).unwrap();
        }
    )*};
}

mod comments {
    use super::*;

    suite!(comment_store, MemoryCommentStore, CommentFixture;
        a_comment_reads_back,
        a_flares_list_holds_only_its_comments,
        every_change_is_an_event_at_a_larger_position,
        events_are_bounded_by_position_and_limit,
        a_repeated_key_answers_the_first_comment,
        an_edit_replaces_the_text_and_keeps_the_rest,
        a_delete_erases_the_text_and_keeps_the_rest,
        an_anchor_reads_back_and_a_delete_erases_it,
        an_unknown_comment_is_not_found,
    );

    #[test]
    fn concurrent_comments_get_distinct_positions() {
        let fixture = CommentFixture::standard().unwrap();
        let store = MemoryCommentStore::new();
        block_on(comment_store::concurrent_comments_get_distinct_positions(
            &store, &fixture, &Threads,
        ))
        .unwrap();
    }

    #[test]
    fn concurrent_edit_and_delete_leave_one_consistent_state() {
        let fixture = CommentFixture::standard().unwrap();
        let store = MemoryCommentStore::new();
        block_on(
            comment_store::concurrent_edit_and_delete_leave_one_consistent_state(
                &store, &fixture, &Threads,
            ),
        )
        .unwrap();
    }

    #[test]
    fn the_flare_limit_holds() {
        let fixture = CommentFixture::standard().unwrap();
        let store = MemoryCommentStore::new().with_flare_limit(3);
        block_on(comment_store::the_flare_limit_holds(&store, &fixture, 3)).unwrap();
    }
}

mod cursors {
    use super::*;

    suite!(cursor_store, MemoryCursorStore, CursorFixture;
        an_advance_makes_the_cursor,
        a_cursor_never_moves_back,
        cursors_are_kept_apart,
    );

    #[test]
    fn the_cursor_limit_holds() {
        let fixture = CursorFixture::standard().unwrap();
        let store = MemoryCursorStore::new().with_limit(3);
        block_on(cursor_store::the_cursor_limit_holds(&store, &fixture, 3)).unwrap();
    }
}
