//! Read-only reader for the user's Claude memory files.
//!
//! Points at `~/.claude/projects/<slug>/memory/*.md` (the auto-memory
//! system) plus any `CLAUDE.md` files under the user's projects directory
//! and extracts brief fact-like lines. Pane uses these lines to occasionally
//! drop personalized speech ("Still on the bloodeye PWA?") instead of a
//! generic idle quip.
//!
//! Strict read-only: this module never writes, never deletes, never edits.
//! If the schema changes or a file is malformed we silently skip it rather
//! than crash the app.

use std::path::{Path, PathBuf};

/// A single fact mined from a memory file — one bullet or one-liner fact,
/// carrying its source path for debugging (never shown to the user).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryFact {
    pub source: PathBuf,
    pub text: String,
}

/// Parse a markdown body and return bullet-style facts. We strip the leading
/// "- " / "* " marker and any trailing punctuation noise. Non-bullet lines
/// are skipped.
///
/// Heuristic: facts under 200 chars only. Long prose paragraphs aren't the
/// kind of thing we'd show as a speech bubble.
pub fn parse_bullets(markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in markdown.lines() {
        let line = raw.trim();
        let stripped = if let Some(s) = line.strip_prefix("- ") {
            s
        } else if let Some(s) = line.strip_prefix("* ") {
            s
        } else {
            continue;
        };
        let trimmed = stripped.trim();
        if trimmed.is_empty() || trimmed.len() > 200 { continue; }
        out.push(trimmed.to_string());
    }
    out
}

/// Strip a YAML-style frontmatter block (`---\n...\n---\n`) from a markdown
/// body, returning just the content below it. If no frontmatter exists the
/// original text is returned unchanged.
pub fn strip_frontmatter(markdown: &str) -> &str {
    let trimmed = markdown.trim_start();
    if !trimmed.starts_with("---") { return markdown; }
    // Find the closing `---` on a line by itself, anywhere after the first.
    let after_open = &trimmed[3..];
    if let Some(end) = after_open.find("\n---") {
        let after = &after_open[end + 4..];
        // Skip the immediate newline after the closing ---.
        return after.strip_prefix('\n').unwrap_or(after);
    }
    markdown
}

/// Scan a directory tree rooted at `root` for memory-file candidates:
/// `MEMORY.md`, any file under a `memory/` subdir, and `CLAUDE.md`. Returns
/// the files we found; caller parses each.
pub fn discover_memory_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(root, 0, 4, &mut out);
    out
}

fn walk(dir: &Path, depth: u32, max_depth: u32, out: &mut Vec<PathBuf>) {
    if depth > max_depth { return; }
    let Ok(entries) = std::fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip node_modules, target, .git — cheap but avoids blowing up
            // on a massive monorepo. Depth cap also keeps this bounded.
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if matches!(name, "node_modules" | "target" | ".git" | "dist" | "build") {
                continue;
            }
            walk(&path, depth + 1, max_depth, out);
        } else if is_memory_file(&path) {
            out.push(path);
        }
    }
}

fn is_memory_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else { return false; };
    if name.eq_ignore_ascii_case("CLAUDE.md") { return true; }
    if name.eq_ignore_ascii_case("MEMORY.md") { return true; }
    // memory/ dir files: *.md under any `memory` subdirectory.
    if name.ends_with(".md") {
        let in_memory_dir = path.parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("memory"))
            .unwrap_or(false);
        if in_memory_dir { return true; }
    }
    false
}

/// Collect facts from a single file. Returns empty on any I/O or parse error
/// — read-only means we never report problems up; just skip and move on.
pub fn facts_from_file(path: &Path) -> Vec<MemoryFact> {
    let Ok(content) = std::fs::read_to_string(path) else { return Vec::new(); };
    let body = strip_frontmatter(&content);
    parse_bullets(body)
        .into_iter()
        .map(|text| MemoryFact { source: path.to_path_buf(), text })
        .collect()
}

/// Scan ~/.claude/projects and the user's active project directories for
/// memory files, returning the deduplicated fact pool. Cap output so an
/// enormous memory tree can't balloon the app's memory footprint.
pub fn scan_all(max_facts: usize) -> Vec<MemoryFact> {
    let Some(home) = dirs::home_dir() else { return Vec::new(); };
    let roots = [
        home.join(".claude").join("projects"),
        home.join(".claude"),
    ];
    let mut all: Vec<MemoryFact> = Vec::new();
    for root in roots {
        if !root.exists() { continue; }
        for file in discover_memory_files(&root) {
            all.extend(facts_from_file(&file));
            if all.len() >= max_facts { break; }
        }
        if all.len() >= max_facts { break; }
    }
    all.truncate(max_facts);
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parse_bullets_extracts_dash_and_star() {
        let md = "# Title\n\n- first fact\n* second fact\nparagraph\n- third";
        assert_eq!(parse_bullets(md), vec!["first fact", "second fact", "third"]);
    }

    #[test]
    fn parse_bullets_trims_and_skips_overlong_lines() {
        let long = "a".repeat(250);
        let md = format!("- short\n- {long}");
        let bullets = parse_bullets(&md);
        assert_eq!(bullets, vec!["short"]);
    }

    #[test]
    fn strip_frontmatter_handles_standard_block() {
        let md = "---\nname: foo\ntype: user\n---\nBody here\n- fact";
        let body = strip_frontmatter(md);
        assert!(body.starts_with("Body here"));
        assert!(body.contains("- fact"));
    }

    #[test]
    fn strip_frontmatter_no_op_when_missing() {
        let md = "No frontmatter here.\n- fact";
        assert_eq!(strip_frontmatter(md), md);
    }

    #[test]
    fn discover_memory_files_finds_claude_md_and_memory_dir_files() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("CLAUDE.md"), "# claude").unwrap();
        std::fs::create_dir_all(root.join("memory")).unwrap();
        std::fs::write(root.join("memory/MEMORY.md"), "# index").unwrap();
        std::fs::write(root.join("memory/notes.md"), "# notes").unwrap();
        std::fs::write(root.join("README.md"), "unrelated").unwrap();

        let files = discover_memory_files(root);
        let names: Vec<String> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|s| s.to_str()).map(String::from))
            .collect();
        assert!(names.contains(&"CLAUDE.md".to_string()));
        assert!(names.contains(&"MEMORY.md".to_string()));
        assert!(names.contains(&"notes.md".to_string()));
        assert!(!names.contains(&"README.md".to_string()));
    }

    #[test]
    fn discover_skips_heavy_directories() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("node_modules/sub/memory")).unwrap();
        std::fs::write(root.join("node_modules/sub/memory/ignore.md"), "- skipped").unwrap();
        std::fs::write(root.join("CLAUDE.md"), "- kept").unwrap();
        let files = discover_memory_files(root);
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn facts_from_file_round_trip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("CLAUDE.md");
        std::fs::write(&path, "---\nname: x\n---\n# heading\n- first\n- second").unwrap();
        let facts = facts_from_file(&path);
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].text, "first");
    }

    #[test]
    fn facts_from_missing_file_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nope.md");
        assert!(facts_from_file(&path).is_empty());
    }
}
