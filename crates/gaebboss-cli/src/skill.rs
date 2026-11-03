//! `gaebboss skill`: installing the bundled Claude Code skill, which teaches
//! coding agents the CLI, Python and Rust surfaces of gaebboss.

use std::fs;
use std::path::{Path, PathBuf};

use clap::Subcommand;

/// Compiled into the binary so the installed skill matches the installed version.
pub const SKILL_MD: &str = include_str!("../skill/SKILL.md");

#[derive(Subcommand)]
pub enum SkillCommand {
    /// Install the skill for coding agents: into ./.claude/skills/gaebboss
    /// (this project), or with --global into ~/.claude/skills/gaebboss.
    Install {
        /// Install into ~/.claude/skills instead of ./.claude/skills.
        #[arg(long, short = 'g')]
        global: bool,
    },
    /// Print the skill document to stdout.
    Show,
}

pub fn cmd_skill(command: SkillCommand) -> Result<(), String> {
    match command {
        SkillCommand::Install { global } => {
            let path = install_into(&skills_root(global)?)?;
            println!("installed {}", path.display());
            Ok(())
        }
        SkillCommand::Show => {
            print!("{SKILL_MD}");
            Ok(())
        }
    }
}

/// `.claude/skills` under the home directory for `--global`, under the
/// current directory otherwise.
fn skills_root(global: bool) -> Result<PathBuf, String> {
    let local = PathBuf::from(".claude").join("skills");
    if !global {
        return Ok(local);
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or("cannot locate the home directory: neither HOME nor USERPROFILE is set")?;
    Ok(PathBuf::from(home).join(local))
}

/// Writes `root/gaebboss/SKILL.md`, replacing an earlier install.
fn install_into(root: &Path) -> Result<PathBuf, String> {
    let dir = root.join("gaebboss");
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let path = dir.join("SKILL.md");
    fs::write(&path, SKILL_MD).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_and_overwrites() {
        let root = tempfile::tempdir().unwrap();
        let path = install_into(root.path()).unwrap();
        assert_eq!(path, root.path().join("gaebboss").join("SKILL.md"));
        assert_eq!(fs::read_to_string(&path).unwrap(), SKILL_MD);
        fs::write(&path, "stale").unwrap();
        install_into(root.path()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), SKILL_MD);
    }

    #[test]
    fn skill_document_has_frontmatter_and_no_em_dashes() {
        assert!(SKILL_MD.starts_with("---\nname: gaebboss\n"));
        assert!(SKILL_MD.contains("description:"));
        assert!(!SKILL_MD.contains('\u{2014}'));
    }

    #[test]
    fn repository_copy_matches_the_bundled_skill() {
        let copy = format!(
            "{}/../../skills/gaebboss/SKILL.md",
            env!("CARGO_MANIFEST_DIR")
        );
        assert_eq!(fs::read_to_string(copy).unwrap(), SKILL_MD);
    }

    #[test]
    fn local_root_is_relative_and_global_root_is_under_home() {
        assert_eq!(
            skills_root(false).unwrap(),
            PathBuf::from(".claude").join("skills")
        );
        let global = skills_root(true).unwrap();
        assert!(global.ends_with(Path::new(".claude").join("skills")));
        assert!(global.is_absolute());
    }
}
