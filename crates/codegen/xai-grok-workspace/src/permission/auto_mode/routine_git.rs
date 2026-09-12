//! Git 的 routine 快路径按参数形态放行；无法证明的形态继续走权限判断。

use crate::permission::exec_risk::{
    git_words_are_read_only_query, git_words_have_unsafe_query_option,
};

pub(super) fn git_words_are_routine(words: &[String]) -> bool {
    if words.first().map(String::as_str) != Some("git") {
        return false;
    }
    if git_words_are_read_only_query(words) {
        return true;
    }
    if git_words_have_unsafe_query_option(words) {
        return false;
    }
    let Some(verb) = words.get(1).map(String::as_str) else {
        return false;
    };
    let args = &words[2..];
    match verb {
        "add" | "commit" | "pull" | "fetch" => true,
        "worktree" => args.first().map(String::as_str) == Some("list"),
        "checkout" | "switch" => branch_switch_is_routine(verb, args),
        "stash" => stash_is_routine(args),
        _ => false,
    }
}

fn branch_switch_is_routine(verb: &str, args: &[String]) -> bool {
    let mut operands = 0;
    let mut creates_branch = false;
    let mut branch_only = verb == "switch";
    let mut words = args.iter().map(String::as_str);
    while let Some(word) = words.next() {
        match word {
            "-q" | "--quiet" | "--track" | "--no-track" | "--guess" | "--no-guess"
            | "--progress" | "--no-progress" => {}
            "-d" | "--detach" => branch_only = true,
            "-b" if verb == "checkout" && !creates_branch => {
                if !words.next().is_some_and(|name| !name.starts_with('-')) {
                    return false;
                }
                creates_branch = true;
                branch_only = true;
            }
            "-c" if verb == "switch" && !creates_branch => {
                if !words.next().is_some_and(|name| !name.starts_with('-')) {
                    return false;
                }
                creates_branch = true;
            }
            "-" => operands += 1,
            // 不接受 force、重置已有分支、pathspec、缩写或组合短选项。
            _ if word.starts_with('-') => return false,
            _ => operands += 1,
        }
    }
    // `checkout name` 可能是在还原同名文件；不根据文件扩展名猜测它是分支。
    operands <= 1 && (branch_only || operands == 0)
}

fn stash_is_routine(args: &[String]) -> bool {
    let mut words = args.iter().map(String::as_str);
    loop {
        match words.next() {
            Some("-q" | "--quiet") => {}
            Some("-m" | "--message") => {
                if words.next().is_none() {
                    return false;
                }
            }
            None | Some("push" | "save" | "pop" | "apply" | "list" | "show" | "branch") => {
                return true;
            }
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_or_ambiguous_git_shapes_are_not_routine() {
        for cmd in [
            "git checkout -- src/lib.rs",
            "git checkout HEAD -- Makefile",
            "git checkout -f main",
            "git checkout --fo main",
            "git checkout -qf main",
            "git checkout -B main HEAD",
            "git checkout --pathspec-from-file=paths.txt",
            "git checkout main Makefile",
            "git checkout Makefile",
            "git checkout src",
            "git checkout main",
            "git checkout -b",
            "git checkout -b -f main",
            "git switch --discard-changes main",
            "git switch --disc main",
            "git switch -C main HEAD",
            "git switch -c",
            "git switch --force main",
            "git stash drop",
            "git stash -q drop",
            "git stash clear",
            "git stash --unknown",
            "git stash -m",
            "git restore .",
            "git worktree remove sibling",
            "git push origin main",
            "git reset --hard",
            "Git switch main",
            "/usr/bin/git switch main",
        ] {
            let words = cmd
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            assert!(!git_words_are_routine(&words), "{cmd}");
        }
    }

    #[test]
    fn ordinary_git_workflow_stays_routine() {
        for cmd in [
            "git status",
            "git --no-pager log --oneline",
            "git diff HEAD",
            "git -C sub status",
            "git switch main",
            "git switch -",
            "git switch --quiet feature/example",
            "git switch -c feature/example main",
            "git checkout",
            "git checkout -b feature/example main",
            "git checkout --detach HEAD~1",
            "git stash",
            "git stash -m wip",
            "git stash push -m wip",
            "git stash pop",
            "git stash apply",
            "git stash list",
            "git stash show -p",
            "git add -A",
            "git commit -m fix",
            "git fetch origin",
            "git pull",
            "git worktree list",
        ] {
            let words = cmd
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            assert!(git_words_are_routine(&words), "{cmd}");
        }
    }
}
