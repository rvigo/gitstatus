use regex::Regex;
use std::{
    error::Error,
    fs::File,
    io::{self, BufRead},
    path::Path,
    process::Command,
    result::Result,
};

type StatusLine = (char, char, String);

fn main() -> Result<(), Box<dyn Error>> {
    let porcelain = Command::new("git")
        .args(["status", "--porcelain", "--branch"])
        .output()
        .expect("failed to wait on child");

    let stdout = porcelain.stdout;

    if porcelain.status.code().unwrap_or(1) != 0 {
        // not a git repo
        std::process::exit(0);
    }

    let mut untracked: Vec<StatusLine> = vec![];
    let mut staged: Vec<StatusLine> = vec![];
    let mut changed: Vec<StatusLine> = vec![];
    let mut deleted: Vec<StatusLine> = vec![];
    let mut conflicts: Vec<StatusLine> = vec![];
    let mut ahead = 0;
    let mut behind = 0;
    let mut branch = None;

    let initial_commit_re = Regex::new(r"Initial commit on").unwrap();
    let no_commits_re = Regex::new(r"No commits yet on").unwrap();
    let no_branch_re = Regex::new(r"no branch").unwrap();

    for line in stdout.lines().map_while(Result::ok) {
        let line = line.trim();
        if line.len() < 3 {
            continue;
        }
        let status = (
            line.chars().next().unwrap(),
            line.chars().nth(1).unwrap(),
            line[2..].to_string(),
        );

        match status {
            ('#', '#', ref git_ref) => {
                if initial_commit_re.is_match(git_ref) || no_commits_re.is_match(git_ref) {
                    branch = Some(
                        status
                            .2
                            .split_whitespace()
                            .last()
                            .unwrap_or_default()
                            .to_string(),
                    );
                } else if no_branch_re.is_match(git_ref) {
                    branch = get_tagname_or_hash();
                } else if git_ref.trim().split("...").count() == 1 {
                    branch = Some(git_ref.trim().to_string());
                } else {
                    let parts: Vec<&str> = git_ref.trim().split("...").collect();
                    branch = Some(parts[0].to_string());
                    let rest = parts[1];
                    if rest.split_whitespace().count() > 1 {
                        let divergence = rest
                            .split_whitespace()
                            .skip(1)
                            .collect::<Vec<&str>>()
                            .join(" ");
                        let divergence = divergence.trim_start_matches('[').trim_end_matches(']');
                        for div in divergence.split(", ") {
                            if div.contains("ahead") {
                                ahead = div["ahead ".len()..].trim().parse().unwrap_or(0);
                            } else if div.contains("behind") {
                                behind = div["behind ".len()..].trim().parse().unwrap_or(0);
                            }
                        }
                    }
                }
            }
            ('?', '?', _) => untracked.push(status),
            (_, 'M', _) => changed.push(status),
            (_, 'D', _) => deleted.push(status),
            ('U', _, _) => conflicts.push(status),
            (c, _, _) if c != ' ' => staged.push(status),
            _ => {}
        }
    }

    let stashed = get_stash();
    let clean = is_clean(&changed, &deleted, &staged, &conflicts, &untracked);

    let out = format!(
        "{} {} {} {} {} {} {} {} {} {}",
        branch.unwrap_or_default(),
        ahead,
        behind,
        staged.len(),
        conflicts.len(),
        changed.len(),
        untracked.len(),
        stashed,
        clean,
        deleted.len()
    );
    print!("{}", out);

    Ok(())
}

fn is_clean(
    changed: &[StatusLine],
    deleted: &[StatusLine],
    staged: &[StatusLine],
    conflicts: &[StatusLine],
    untracked: &[StatusLine],
) -> i32 {
    if changed.is_empty()
        && deleted.is_empty()
        && staged.is_empty()
        && conflicts.is_empty()
        && untracked.is_empty()
    {
        1
    } else {
        0
    }
}

fn get_stash() -> usize {
    let output = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .expect("failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stash_file = format!("{}/logs/refs/stash", stdout.trim());

    if let Ok(file) = File::open(Path::new(&stash_file)) {
        let reader = io::BufReader::new(file);
        reader.lines().count()
    } else {
        0
    }
}

fn get_tagname_or_hash() -> Option<String> {
    // Get the tag name
    let tags_output = Command::new("git")
        .args([
            "for-each-ref",
            "--points-at=HEAD",
            "--count=2",
            "--sort=-version:refname",
            "--format=%(refname:short)",
            "refs/tags",
        ])
        .output()
        .expect("failed to execute command");

    let tags = String::from_utf8_lossy(&tags_output.stdout)
        .split_whitespace()
        .map(|s| s.to_string())
        .collect::<Vec<String>>();

    if !tags.is_empty() {
        return Some(tags[0].to_owned() + if tags.len() > 1 { "+" } else { "" });
    }

    // Get the hash
    let hash_output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .expect("failed to execute command");

    let hash = String::from_utf8_lossy(&hash_output.stdout)
        .trim()
        .to_string();

    if !hash.is_empty() {
        Some(hash)
    } else {
        None
    }
}
