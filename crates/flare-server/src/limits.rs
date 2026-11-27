//! The limits that keep one client or one large record from exhausting
//! the server (threat 21).

use std::time::Duration;

/// The server's limits. Each says what it counts and what happens when it
/// is hit; [`Limits::default`] gives the values of the design.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    /// The most bytes of request header names and values together. A larger
    /// header block answers 431 `request::headers_too_large`. The default is
    /// 32 KiB.
    pub header_bytes: usize,
    /// The most bytes of a request body. A larger body answers 413
    /// `request::body_too_large`, read through `http_body_util::Limited`
    /// before any parsing. The default is 1 MiB.
    pub body_bytes: usize,
    /// The most bytes of a share or version sent with its images as
    /// `multipart/form-data`. A larger body answers 413
    /// `request::body_too_large`, read through `http_body_util::Limited`
    /// before any parsing, whose title names 4 MiB. Each image is also at
    /// most 5 MiB. The default is [`flare_core::MAX_UPLOAD_BYTES`], 4 MiB.
    /// A form with an `html` part may carry 64 KiB more for its framing.
    pub upload_bytes: usize,
    /// How long the server waits for a request's header block, and then
    /// for its body. A slower body answers 408 `request::slow`; a slower
    /// header block closes the connection. The default is 10 seconds.
    pub read_time: Duration,
    /// The most connections open at once. The next one waits in the accept
    /// queue until one closes. The default is 64.
    pub connections: usize,
    /// The most requests in work at once. The next one answers 503
    /// `server::busy`. The default is 16.
    pub in_work: usize,
    /// The most `/auth/oidc/callback` requests in work at once, each of
    /// which may hold a token request to the provider for up to its
    /// deadline. A callback takes one of these before its in-work slot, so
    /// callbacks never hold more than this many of the in-work slots. A
    /// callback that finds none free waits for one, in the order the
    /// callbacks arrived, for up to [`Limits::token_wait`]. It bounds token
    /// requests in flight, and does not limit how often clients call; a
    /// hosted site's proxy rate-limits the sign-in routes. The default is 4.
    pub token_calls: usize,
    /// How long a callback waits for a slot of [`Limits::token_calls`]. A
    /// callback still waiting when it passes answers 503 `server::busy`. A
    /// waiting callback holds its connection and no in-work slot, and the
    /// wait comes before [`Limits::request_time`] starts, so a callback
    /// answers within `token_wait + request_time`. The default is 10
    /// seconds, the deadline of one token request to the provider.
    pub token_wait: Duration,
    /// The most callbacks that wait for a slot of [`Limits::token_calls`]
    /// at once. The next callback answers 503 `server::busy` at once, so
    /// callbacks hold at most `token_calls + token_waiters` of the
    /// [`Limits::connections`] and the rest stay free for readers. The
    /// default is 16.
    pub token_waiters: usize,
    /// How long one request may take, from its route starting to its
    /// reply. A slower one is dropped and answers 503 `server::deadline`.
    /// The request's store calls are dropped before the 503 is sent;
    /// flare-board's stores stop a dropped call's board child and wait for
    /// it to end, so no write commits after the 503. The default is 30
    /// seconds.
    pub request_time: Duration,
    /// The most form tokens kept at once. One session keeps at most an
    /// eighth of them, and its next token drops its own oldest. A new
    /// token past the whole table drops the oldest row. A dropped token's
    /// post answers 403 `forms::token_unknown`. The default is 1,024.
    pub form_tokens: usize,
    /// How long a form token works. The default is one hour.
    pub form_token_life: Duration,
    /// The most hand-over codes kept at once. A new code past this drops
    /// the oldest and signs its session out. The default is 64.
    pub handover_codes: usize,
    /// How long a hand-over code works. The default is 60 seconds.
    pub handover_life: Duration,
    /// How long the local site serves without a request before it stops by
    /// itself, as a shutdown would. The hosted site never stops by itself.
    /// The default is 8 hours.
    pub idle_exit: Duration,
    /// How long a shutdown lets requests in work finish before it stops
    /// them. A stopped request's store calls are dropped before `serve`
    /// returns, and flare-board's stores stop and reap the board child of
    /// each. The default is 5 seconds.
    pub drain: Duration,
    /// How often the server drops expired launch codes, form tokens and
    /// hand-over codes and removes their openers. The default is 30
    /// seconds.
    pub sweep_every: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            header_bytes: 32 * 1024,
            body_bytes: 1024 * 1024,
            upload_bytes: flare_core::MAX_UPLOAD_BYTES,
            read_time: Duration::from_secs(10),
            connections: 64,
            in_work: 16,
            token_calls: 4,
            token_wait: Duration::from_secs(10),
            token_waiters: 16,
            request_time: Duration::from_secs(30),
            form_tokens: 1024,
            form_token_life: Duration::from_secs(60 * 60),
            handover_codes: 64,
            handover_life: Duration::from_secs(60),
            idle_exit: Duration::from_secs(8 * 60 * 60),
            drain: Duration::from_secs(5),
            sweep_every: Duration::from_secs(30),
        }
    }
}
