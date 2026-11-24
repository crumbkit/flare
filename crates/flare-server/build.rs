//! Embeds the reading page's script: every file Vite wrote to
//! `ui/flare/dist/assets`, and the name of the entry file from Vite's
//! manifest. flare-server serves each file at `/flare/assets/<file>`.
//!
//! With no bundle, which is the case when Node has not run, the build warns
//! and embeds nothing, so pages show plain code and diagram source. With
//! `FLARE_REQUIRE_UI=1` a missing bundle fails the build. CI's reading job
//! sets it; the repository has no release workflow yet, so a build that will
//! serve readers must set it itself.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The manifest's key for the entry module.
const ENTRY_SOURCE: &str = "src/main.ts";

/// Why the bundle could not be embedded.
#[derive(Debug)]
enum Missing {
    /// The manifest is absent: Vite has not built the bundle.
    NotBuilt(PathBuf),
    /// The manifest or a file could not be read or understood.
    Broken(String),
}

#[expect(
    clippy::disallowed_methods,
    reason = "a build script reads Cargo's variables from its environment; it has no snapshot"
)]
fn variable(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Tells Cargo `line`, such as a warning, an error or a rerun rule. A build
/// script talks to Cargo on standard output, and Clippy allows it there.
fn tell(line: &str) {
    println!("cargo::{line}");
}

fn main() {
    tell("rerun-if-env-changed=FLARE_REQUIRE_UI");
    let Some(manifest_dir) = variable("CARGO_MANIFEST_DIR") else {
        tell("error=Cargo did not set CARGO_MANIFEST_DIR");
        return;
    };
    let Some(out_dir) = variable("OUT_DIR") else {
        tell("error=Cargo did not set OUT_DIR");
        return;
    };
    let dist = Path::new(&manifest_dir).join("../../ui/flare/dist");
    tell(&format!("rerun-if-changed={}", dist.display()));
    let manifest = dist.join(".vite/manifest.json");
    tell(&format!("rerun-if-changed={}", manifest.display()));
    let source = match embedded(&dist, &manifest) {
        Ok(source) => source,
        Err(missing) => {
            let required = variable("FLARE_REQUIRE_UI").as_deref() == Some("1");
            let reason = match missing {
                Missing::NotBuilt(path) => format!(
                    "the reading page's script is not built ({} is missing); run `pnpm install && pnpm run build` in ui/",
                    path.display()
                ),
                Missing::Broken(reason) => reason,
            };
            if required {
                tell(&format!("error=FLARE_REQUIRE_UI=1 and {reason}"));
                return;
            }
            tell(&format!(
                "warning={reason}; pages will show plain code and diagram source"
            ));
            String::from(
                "pub(crate) const ENTRY: Option<&str> = None;\npub(crate) static FILES: &[(&str, &[u8])] = &[];\n",
            )
        }
    };
    let target = Path::new(&out_dir).join("ui_assets.rs");
    #[expect(
        clippy::disallowed_methods,
        reason = "a build script writes its generated source to OUT_DIR, which Cargo owns and rebuilds"
    )]
    if let Err(error) = std::fs::write(&target, source) {
        tell(&format!(
            "error=could not write {}: {error}",
            target.display()
        ));
    }
}

/// The generated source that embeds the bundle in `dist`.
fn embedded(dist: &Path, manifest: &Path) -> Result<String, Missing> {
    let text = match std::fs::read_to_string(manifest) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(Missing::NotBuilt(manifest.to_path_buf()));
        }
        Err(error) => {
            return Err(Missing::Broken(format!(
                "could not read {}: {error}",
                manifest.display()
            )));
        }
    };
    let parsed: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| Missing::Broken(format!("{} is not JSON: {error}", manifest.display())))?;
    let entry = parsed
        .get(ENTRY_SOURCE)
        .and_then(|entry| entry.get("file"))
        .and_then(serde_json::Value::as_str)
        .and_then(|file| file.strip_prefix("assets/"))
        .ok_or_else(|| {
            Missing::Broken(format!(
                "{} names no entry file for {ENTRY_SOURCE}",
                manifest.display()
            ))
        })?;
    let assets = dist.join("assets");
    let listing = std::fs::read_dir(&assets).map_err(|error| {
        Missing::Broken(format!("could not list {}: {error}", assets.display()))
    })?;
    let mut files = Vec::new();
    for item in listing {
        let item = item.map_err(|error| {
            Missing::Broken(format!("could not list {}: {error}", assets.display()))
        })?;
        let path = item.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if path.is_file() {
            let absolute = path.canonicalize().map_err(|error| {
                Missing::Broken(format!("could not resolve {}: {error}", path.display()))
            })?;
            files.push((name.to_owned(), absolute));
        }
    }
    files.sort();
    if !files.iter().any(|(name, _)| name == entry) {
        return Err(Missing::Broken(format!(
            "the entry file {entry} is not in {}",
            assets.display()
        )));
    }
    let mut source = format!("pub(crate) const ENTRY: Option<&str> = Some({entry:?});\n");
    source.push_str("pub(crate) static FILES: &[(&str, &[u8])] = &[\n");
    for (name, path) in &files {
        let _ = writeln!(
            source,
            "    ({name:?}, include_bytes!({:?})),",
            path.display().to_string()
        );
    }
    source.push_str("];\n");
    Ok(source)
}
