use std::fs;
use std::path::{Path, PathBuf};

const HEAD_REF_PREFIX: &str = "ref: refs/heads/";
const GITDIR_PREFIX: &str = "gitdir: ";
const DETACHED_LABEL_LENGTH: usize = 7;

#[must_use]
pub fn current_branch(working_directory: &Path) -> Option<String> {
    let git_directory = locate_git_directory(working_directory)?;
    let head = fs::read_to_string(git_directory.join("HEAD")).ok()?;
    let trimmed = head.trim();
    if let Some(branch) = trimmed.strip_prefix(HEAD_REF_PREFIX) {
        return Some(branch.to_owned());
    }
    let detached: String = trimmed.chars().take(DETACHED_LABEL_LENGTH).collect();
    (!detached.is_empty()).then_some(detached)
}

fn locate_git_directory(working_directory: &Path) -> Option<PathBuf> {
    let mut candidate = Some(working_directory);
    while let Some(directory) = candidate {
        let marker = directory.join(".git");
        if marker.is_dir() {
            return Some(marker);
        }
        if marker.is_file() {
            return resolve_linked_git_directory(&marker, directory);
        }
        candidate = directory.parent();
    }
    None
}

fn resolve_linked_git_directory(marker: &Path, base: &Path) -> Option<PathBuf> {
    let contents = fs::read_to_string(marker).ok()?;
    let target = contents.trim().strip_prefix(GITDIR_PREFIX)?;
    let path = PathBuf::from(target);
    Some(if path.is_absolute() {
        path
    } else {
        base.join(path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        drop(fs::remove_dir_all(&path));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn reads_a_branch_from_a_plain_repository() {
        let root = scratch("instrumentline-git-plain");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/HEAD"), "ref: refs/heads/feat/statusline\n").unwrap();
        assert_eq!(current_branch(&root), Some("feat/statusline".to_owned()));
        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn walks_upward_from_a_nested_directory() {
        let root = scratch("instrumentline-git-nested");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("src/deep")).unwrap();
        fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(
            current_branch(&root.join("src/deep")),
            Some("main".to_owned())
        );
        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn a_detached_head_reports_a_short_commit() {
        let root = scratch("instrumentline-git-detached");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(
            root.join(".git/HEAD"),
            "3f2a1c9d8e7b6a5f4e3d2c1b0a9f8e7d6c5b4a39\n",
        )
        .unwrap();
        assert_eq!(current_branch(&root), Some("3f2a1c9".to_owned()));
        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn a_linked_worktree_follows_the_gitdir_pointer() {
        let root = scratch("instrumentline-git-worktree");
        let real = root.join("real-git");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("HEAD"), "ref: refs/heads/worktree-branch\n").unwrap();
        let checkout = root.join("checkout");
        fs::create_dir_all(&checkout).unwrap();
        let mut marker = fs::File::create(checkout.join(".git")).unwrap();
        write!(marker, "gitdir: {}", real.display()).unwrap();
        drop(marker);
        assert_eq!(
            current_branch(&checkout),
            Some("worktree-branch".to_owned())
        );
        drop(fs::remove_dir_all(&root));
    }

    #[test]
    fn a_directory_outside_any_repository_reports_nothing() {
        let root = scratch("instrumentline-git-none");
        assert_eq!(current_branch(&root), None);
        drop(fs::remove_dir_all(&root));
    }
}
