//! Plan test 16, conformance: the `flare` command's tree, tool record and
//! failure codes against crumbkit's conventions with crumbkit-test (CLI-6).
//!
//! They live in an integration test so the checks that read help pages can
//! run the built binary. `SNAPSHOTS=overwrite cargo nextest run -p
//! flare-cli --test conventions` writes and rewrites the golden files.
#![cfg(test)]

use clap::CommandFactory as _;
use crumbkit_test::{Allow, Check, Examples, Suite};
use flare_cli::{Cli, TOOL};

/// flare's opt-outs from crumbkit's conventions, which every check reads.
/// The tool record declares the root verbs `serve`, `open`, `publish` and
/// `bind` with their reason, so they need no opt-out here.
const ALLOW: &[Allow] = &[];

/// flare's built binary and the directory of its golden files.
const EXAMPLES: Examples<'_> =
    Examples::new(env!("CARGO_BIN_EXE_flare"), "tests/golden/examples").allow(ALLOW);

/// Everything the checks read, which `conventions!` expands to one test for
/// each check.
const SUITE: Suite<'_> = Suite::new(|| (Cli::command(), TOOL))
    .allow(ALLOW)
    .verbs(&["serve", "open", "publish", "bind", "usage"])
    .examples(EXAMPLES)
    .skip(
        Check::Examples,
        "Every example needs a board and the board program, `serve` does not exit and `open` needs a running server, so no example has fixed output; tests/pages.rs and tests/serve.rs run each command on a scratch board.",
    )
    .skip(
        Check::OutputBudget,
        "It runs the examples, which need a scratch board; see the Examples skip.",
    )
    .skip(
        Check::PathWeights,
        "The path weights read the example golden files, which the Examples skip leaves out.",
    )
    .skip(
        Check::Recoveries,
        "The recovery of server::not_running is `flare serve`, which does not exit, and the other recoveries need a scratch board; tests/open.rs and tests/serve.rs assert each failure and the command its recovery names.",
    )
    .skip(
        Check::Twins,
        "Each refusal needs a scratch board or a running server, so tests/pages.rs, tests/serve.rs and tests/open.rs pair each refused command line with its admitted twin.",
    );

crumbkit_test::conventions!(SUITE);
