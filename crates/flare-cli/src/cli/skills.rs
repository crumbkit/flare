//! The grammar of `flare skills install`, `check` and `print`, which
//! crumbkit-skills serves, and the skill that the binary embeds.

use crumbkit_skills::{Skill, skills_command};

skills_command!(
    /// The commands for flare's agent skill.
    pub(crate) FlareSkills,
    "flare"
);

/// flare's agent skill, which `flare skills install` writes into each agent
/// skills directory and `flare skills print` prints.
pub(crate) const SKILL: Skill = Skill::new(
    "flare",
    env!("CARGO_PKG_VERSION"),
    include_str!("../../skill/SKILL.md"),
);
