//! Plan test 16, conformance: the `flare` command's tree, tool record and
//! failure codes against crumbkit's conventions with crumbkit-test (CLI-6).
//!
//! They live in an integration test so the checks that read help pages can
//! run the built binary. `SNAPSHOTS=overwrite cargo nextest run -p
//! flare-cli --test conventions` writes and rewrites the golden files.
#![cfg(test)]

use clap::CommandFactory as _;
use crumbkit_test::{Allow, Check, Examples, Rule, Suite};
use flare_cli::{Cli, TOOL};

/// flare's opt-outs from crumbkit's conventions, which every check reads.
/// The tool record declares the root verbs `serve`, `open`, `publish`,
/// `bind`, `login`, `logout`, `share`, `grant`, `shares`, `activity` and
/// `metrics` with their reason, so they need no opt-out here. `show` is crumbkit's
/// built-in verb.
const ALLOW: &[Allow] = &[Allow::new(
    Rule::ShowIds,
    "show",
    "flare show prints the whole Markdown of one version, up to 256 KiB, and --version names a version of one share, so it takes one id",
)];

/// flare's built binary and the directory of its golden files.
const EXAMPLES: Examples<'_> =
    Examples::new(env!("CARGO_BIN_EXE_flare"), "tests/golden/examples").allow(ALLOW);

/// Everything the checks read, which `conventions!` expands to one test for
/// each check. `usage`, `install`, `check` and `print` are the verbs that
/// `self usage` and `skills` add under a resource.
const SUITE: Suite<'_> = Suite::new(|| (Cli::command(), TOOL))
    .allow(ALLOW)
    .verbs(&[
        "serve", "open", "publish", "bind", "login", "logout", "share", "grant", "shares",
        "activity", "metrics", "usage", "install", "check", "print",
    ])
    .examples(EXAMPLES)
    .skip(
        Check::PathWeights,
        "With crumbkit's default budget of 8,000 bytes a path, the path of `flare skills print` reads 8,494 bytes, because the command prints the 6,270-byte skill.",
    )
    .skip(
        Check::Recoveries,
        "The recovery of server::not_running is `flare serve`, which does not exit, the other recoveries need a scratch board, and the sharing commands' recoveries need a flare server; tests/open.rs, tests/serve.rs and tests/sharing.rs assert each failure and the command its recovery names.",
    )
    .skip(
        Check::Twins,
        "Each refusal needs a scratch board or a running server, so tests/pages.rs, tests/serve.rs, tests/open.rs and tests/sharing.rs pair each refused command line with its admitted twin.",
    );

crumbkit_test::conventions!(SUITE);
