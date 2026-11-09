//! `--merge-semantics` and `--same-change` in real git, colocated jj and internal jj repositories
//! (spec §6.1, `docs/PLAN.md` §B3), headless and through `inspect`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BINARY: &str = env!("CARGO_BIN_EXE_triad-merge");
const RESOLVED: i32 = 0;
const CONFLICTS: i32 = 1;
const ERROR: i32 = 3;

/// `git-mutation-0655` from `fixtures/oracle`: `git merge-file` merges it cleanly to
/// [`GIT_MERGED`]; jj-lib leaves one conflict.
const BASE: &str = "a\nb\nc\na\nb\nc\na\nb\nc\n";
const LEFT: &str = "a\nb\n a\na\nb\n c\n";
const RIGHT: &str = "a \nb\nc\na\nb\n c\n";
const GIT_MERGED: &str = "a \nb\n a\na\nb\n c\n";

/// Both sides make the same change: `accept` resolves it to the change, `keep` leaves it a
/// conflict.
const SAME_BASE: &str = "a\n";
const SAME_SIDE: &str = "b\n";

/// A directory for the run's settings file and jj's config, outside every repository.
struct World {
    dir: tempfile::TempDir,
}

impl World {
    fn new() -> Self {
        Self { dir: tempfile::tempdir().expect("a temporary directory") }
    }

    fn settings(&self) -> PathBuf {
        self.dir.path().join("config.triad")
    }

    fn jj_config(&self) -> PathBuf {
        self.dir.path().join("jj.toml")
    }

    /// The binary in `cwd`, its settings file and jj's config in this world only.
    fn command(&self, cwd: &Path) -> Command {
        let mut command = Command::new(BINARY);
        command
            .current_dir(cwd)
            .env("TRIAD_LOG_DIR", self.dir.path().join("logs"))
            .env("TRIAD_CONFIG", self.settings())
            .env("JJ_CONFIG", self.jj_config())
            .env("GIT_CONFIG_GLOBAL", self.dir.path().join("gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("TRIAD_LOG");
        command
    }

    /// A headless merge of `inputs` written to `output`, run in `cwd`.
    fn merge(&self, cwd: &Path, inputs: [&str; 3], output: &Path, args: &[&str]) -> Output {
        let dir = self.dir.path().join("inputs");
        fs::create_dir_all(&dir).expect("the inputs directory");
        let paths = ["base", "left", "right"].map(|name| dir.join(name));
        for (path, text) in paths.iter().zip(inputs) {
            fs::write(path, text).expect("an input is written");
        }
        self.command(cwd)
            .arg("--headless")
            .args(&paths)
            .arg("-o")
            .arg(output)
            .args(args)
            .output()
            .expect("the binary runs")
    }
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("an exit code")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

const DIFFER: [&str; 3] = [BASE, LEFT, RIGHT];
const SAME: [&str; 3] = [SAME_BASE, SAME_SIDE, SAME_SIDE];

#[test]
fn outside_any_repository_auto_merges_as_git() {
    let world = World::new();
    let out = world.dir.path().join("out.txt");
    let run = world.merge(world.dir.path(), DIFFER, &out, &[]);
    assert_eq!(code(&run), RESOLVED, "{}", stderr(&run));
    assert_eq!(fs::read_to_string(&out).expect("written"), GIT_MERGED);
}

#[test]
fn in_a_git_repository_auto_merges_as_git() {
    let world = World::new();
    let repo = vcs_git::testing::Fixture::new();
    let out = repo.root().join("out.txt");
    let run = world.merge(repo.root().as_std_path(), DIFFER, out.as_std_path(), &[]);
    assert_eq!(code(&run), RESOLVED, "{}", stderr(&run));
    assert_eq!(fs::read_to_string(&out).expect("written"), GIT_MERGED);
}

#[test]
fn in_a_jj_repository_auto_merges_as_jj() {
    for repo in [vcs_jj::testing::Fixture::colocated(), vcs_jj::testing::Fixture::internal()] {
        let world = World::new();
        let out = repo.root().join("out.txt");
        let run = world.merge(repo.root().as_std_path(), DIFFER, out.as_std_path(), &[]);
        assert_eq!(code(&run), CONFLICTS, "{}", stderr(&run));
    }
}

#[test]
fn the_flag_picks_jj_outside_any_repository() {
    let world = World::new();
    let out = world.dir.path().join("out.txt");
    let run = world.merge(world.dir.path(), DIFFER, &out, &["--merge-semantics", "jj"]);
    assert_eq!(code(&run), CONFLICTS, "{}", stderr(&run));
}

#[test]
fn git_semantics_in_a_jj_repository_fails_with_3_and_writes_nothing() {
    for repo in [vcs_jj::testing::Fixture::colocated(), vcs_jj::testing::Fixture::internal()] {
        let world = World::new();
        let out = repo.root().join("out.txt");
        let run = world.merge(
            repo.root().as_std_path(),
            DIFFER,
            out.as_std_path(),
            &["--merge-semantics", "git"],
        );
        assert_eq!(code(&run), ERROR);
        let root = dunce::canonicalize(repo.root()).expect("the root exists");
        let message = stderr(&run);
        assert!(message.contains(&format!("{} is a jj repository", root.display())), "{message}");
        assert!(message.contains("`jj resolve`"), "{message}");
        assert!(!out.exists(), "no output is written");
    }
}

#[test]
fn git_semantics_from_the_settings_file_in_a_jj_repository_fails_with_3() {
    let world = World::new();
    fs::write(world.settings(), "merge-semantics = git\n").expect("settings are written");
    let repo = vcs_jj::testing::Fixture::new();
    let out = repo.root().join("out.txt");
    let run = world.merge(repo.root().as_std_path(), DIFFER, out.as_std_path(), &[]);
    assert_eq!(code(&run), ERROR, "{}", stderr(&run));
    assert!(!out.exists(), "no output is written");
}

#[test]
fn the_semantics_flag_wins_over_the_settings_file() {
    let world = World::new();
    fs::write(world.settings(), "merge-semantics = jj\n").expect("settings are written");
    let out = world.dir.path().join("out.txt");
    let by_file = world.merge(world.dir.path(), DIFFER, &out, &[]);
    assert_eq!(code(&by_file), CONFLICTS, "{}", stderr(&by_file));
    let by_flag = world.merge(world.dir.path(), DIFFER, &out, &["--merge-semantics", "git"]);
    assert_eq!(code(&by_flag), RESOLVED, "{}", stderr(&by_flag));
    assert_eq!(fs::read_to_string(&out).expect("written"), GIT_MERGED);
}

#[test]
fn the_same_change_flag_wins_over_the_settings_file_under_git_semantics() {
    let world = World::new();
    fs::write(world.settings(), "same-change = keep\n").expect("settings are written");
    let out = world.dir.path().join("out.txt");
    let by_file = world.merge(world.dir.path(), SAME, &out, &[]);
    assert_eq!(code(&by_file), CONFLICTS, "{}", stderr(&by_file));
    let by_flag = world.merge(world.dir.path(), SAME, &out, &["--same-change", "accept"]);
    assert_eq!(code(&by_flag), RESOLVED, "{}", stderr(&by_flag));
    assert_eq!(fs::read_to_string(&out).expect("written"), SAME_SIDE);
}

#[test]
fn the_settings_file_wins_over_jjs_config() {
    let world = World::new();
    fs::write(world.settings(), "same-change = accept\n").expect("settings are written");
    fs::write(world.jj_config(), "[merge]\nsame-change = \"keep\"\n").expect("config");
    let repo = vcs_jj::testing::Fixture::new();
    let out = repo.root().join("out.txt");
    let run = world.merge(repo.root().as_std_path(), SAME, out.as_std_path(), &[]);
    assert_eq!(code(&run), RESOLVED, "{}", stderr(&run));
}

/// Every file under `dir`, with its bytes.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_owned()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("the directory lists") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let bytes = fs::read(&path).expect("the file reads");
                files.push((path, bytes));
            }
        }
    }
    files.sort();
    files
}

#[test]
fn reading_jjs_config_writes_nothing() {
    let world = World::new();
    let home = world.dir.path().join("home");
    fs::create_dir_all(home.join(".config/jj")).expect("the user config directory");
    fs::write(home.join(".config/jj/config.toml"), "[merge]\nsame-change = \"keep\"\n")
        .expect("config");
    let repo = vcs_jj::testing::Fixture::new();
    let before = (snapshot(&home), snapshot(&repo.root().join(".jj").into_std_path_buf()));
    let out = world.dir.path().join("out.txt");
    let run = world
        .command(repo.root().as_std_path())
        .env_remove("JJ_CONFIG")
        .env("HOME", &home)
        .env_remove("XDG_CONFIG_HOME")
        .arg("--headless")
        .args(["base", "left", "right"].into_iter().zip(SAME).map(|(name, text)| {
            let path = world.dir.path().join(name);
            fs::write(&path, text).expect("an input");
            path
        }))
        .arg("-o")
        .arg(&out)
        .output()
        .expect("the binary runs");
    assert_eq!(code(&run), CONFLICTS, "the user config was read: {}", stderr(&run));
    let after = (snapshot(&home), snapshot(&repo.root().join(".jj").into_std_path_buf()));
    assert_eq!(before, after);
}

#[test]
fn jjs_resolve_directory_means_jj_from_outside_any_repository() {
    let world = World::new();
    let resolve = world.dir.path().join("jj-resolve-Ab12Cd");
    fs::create_dir(&resolve).expect("the resolve directory");
    let out = resolve.join("output_f.txt");
    let run = world.merge(world.dir.path(), DIFFER, &out, &[]);
    assert_eq!(code(&run), CONFLICTS, "{}", stderr(&run));
    let git = world.merge(world.dir.path(), DIFFER, &out, &["--merge-semantics", "git"]);
    assert_eq!(code(&git), ERROR);
    assert!(stderr(&git).contains("is jj's resolve directory"), "{}", stderr(&git));
}

#[test]
fn same_change_comes_from_jjs_config_under_jj_semantics_only() {
    let world = World::new();
    fs::write(world.jj_config(), "[merge]\nsame-change = \"keep\"\n").expect("config");
    let repo = vcs_jj::testing::Fixture::new();
    let out = repo.root().join("out.txt");
    let jj = world.merge(repo.root().as_std_path(), SAME, out.as_std_path(), &[]);
    assert_eq!(code(&jj), CONFLICTS, "{}", stderr(&jj));
    let flag = world.merge(
        repo.root().as_std_path(),
        SAME,
        out.as_std_path(),
        &["--same-change", "accept"],
    );
    assert_eq!(code(&flag), RESOLVED, "{}", stderr(&flag));
    let git_out = world.dir.path().join("out.txt");
    let git = world.merge(world.dir.path(), SAME, &git_out, &[]);
    assert_eq!(code(&git), RESOLVED, "jj's config is not read for git: {}", stderr(&git));
}

#[test]
fn the_repositorys_own_jj_config_wins_over_the_users() {
    let world = World::new();
    fs::write(world.jj_config(), "[merge]\nsame-change = \"accept\"\n").expect("config");
    let repo = vcs_jj::testing::Fixture::new();
    fs::write(repo.root().join(".jj/repo/config.toml"), "[merge]\nsame-change = \"keep\"\n")
        .expect("repo config");
    let out = repo.root().join("out.txt");
    let run = world.merge(repo.root().as_std_path(), SAME, out.as_std_path(), &[]);
    assert_eq!(code(&run), CONFLICTS, "{}", stderr(&run));
}

#[test]
fn a_bad_jj_config_value_fails_with_3_naming_the_file() {
    let world = World::new();
    fs::write(world.jj_config(), "[merge]\nsame-change = \"often\"\n").expect("config");
    let repo = vcs_jj::testing::Fixture::new();
    let out = repo.root().join("out.txt");
    let run = world.merge(repo.root().as_std_path(), SAME, out.as_std_path(), &[]);
    assert_eq!(code(&run), ERROR);
    let message = stderr(&run);
    assert!(message.contains(&world.jj_config().display().to_string()), "{message}");
    assert!(message.contains("merge.same-change"), "{message}");
    assert!(!out.exists(), "no output is written");
}

#[test]
fn inspect_follows_the_working_directorys_repository() {
    let world = World::new();
    let dir = world.dir.path().join("inputs");
    fs::create_dir_all(&dir).expect("dir");
    let paths = ["base", "left", "right"].map(|name| dir.join(name));
    for (path, text) in paths.iter().zip(DIFFER) {
        fs::write(path, text).expect("an input is written");
    }
    let inspect = |cwd: &Path, args: &[&str]| {
        world.command(cwd).arg("inspect").args(&paths).args(args).output().expect("the binary runs")
    };
    let repo = vcs_jj::testing::Fixture::new();
    assert_eq!(code(&inspect(world.dir.path(), &[])), RESOLVED);
    assert_eq!(code(&inspect(repo.root().as_std_path(), &[])), CONFLICTS);
    assert_eq!(code(&inspect(world.dir.path(), &["--merge-semantics", "jj"])), CONFLICTS);
    assert_eq!(code(&inspect(repo.root().as_std_path(), &["--merge-semantics", "git"])), ERROR);
}

/// `jj-word-resolvable` from `fixtures/oracle`: a line conflict whose words all resolve, to
/// [`WORDS_MERGED`].
const WORDS: [&str; 3] = ["let x = 1;\n", "let y = 1;\n", "let x = 2;\n"];
const WORDS_MERGED: &str = "let y = 2;\n";

#[test]
fn the_hunk_level_flag_merges_words_in_both_semantics() {
    let world = World::new();
    let out = world.dir.path().join("out.txt");
    for semantics in ["git", "jj"] {
        let line = world.merge(world.dir.path(), WORDS, &out, &["--merge-semantics", semantics]);
        assert_eq!(code(&line), CONFLICTS, "{semantics}: {}", stderr(&line));
        let by_word = world.merge(
            world.dir.path(),
            WORDS,
            &out,
            &["--merge-semantics", semantics, "--hunk-level", "word"],
        );
        assert_eq!(code(&by_word), RESOLVED, "{semantics}: {}", stderr(&by_word));
        assert_eq!(fs::read_to_string(&out).expect("written"), WORDS_MERGED);
    }
}

#[test]
fn the_hunk_level_comes_from_the_settings_file() {
    let world = World::new();
    fs::write(world.settings(), "hunk-level = word\n").expect("settings are written");
    let out = world.dir.path().join("out.txt");
    let run = world.merge(world.dir.path(), WORDS, &out, &[]);
    assert_eq!(code(&run), RESOLVED, "{}", stderr(&run));
    let flag = world.merge(world.dir.path(), WORDS, &out, &["--hunk-level", "line"]);
    assert_eq!(code(&flag), CONFLICTS, "{}", stderr(&flag));
}

#[test]
fn the_hunk_level_comes_from_jjs_config_under_jj_semantics_only() {
    let world = World::new();
    fs::write(world.jj_config(), "[merge]\nhunk-level = \"word\"\n").expect("config");
    let repo = vcs_jj::testing::Fixture::new();
    let out = repo.root().join("out.txt");
    let jj = world.merge(repo.root().as_std_path(), WORDS, out.as_std_path(), &[]);
    assert_eq!(code(&jj), RESOLVED, "{}", stderr(&jj));
    assert_eq!(fs::read_to_string(&out).expect("written"), WORDS_MERGED);
    let flag =
        world.merge(repo.root().as_std_path(), WORDS, out.as_std_path(), &["--hunk-level", "line"]);
    assert_eq!(code(&flag), CONFLICTS, "{}", stderr(&flag));
    let git_out = world.dir.path().join("out.txt");
    let git = world.merge(world.dir.path(), WORDS, &git_out, &[]);
    assert_eq!(code(&git), CONFLICTS, "jj's config is not read for git: {}", stderr(&git));
}

#[test]
fn a_same_change_flag_still_reads_the_hunk_level_from_jjs_config() {
    let world = World::new();
    fs::write(world.jj_config(), "[merge]\nhunk-level = \"word\"\n").expect("config");
    let repo = vcs_jj::testing::Fixture::new();
    let out = repo.root().join("out.txt");
    let run = world.merge(
        repo.root().as_std_path(),
        WORDS,
        out.as_std_path(),
        &["--same-change", "accept"],
    );
    assert_eq!(code(&run), RESOLVED, "{}", stderr(&run));
}

/// One conflict on the middle line.
const CONFLICT: [&str; 3] = ["a\nb\nc\n", "a\nL\nc\n", "a\nR\nc\n"];
/// [`CONFLICT`] written in git's diff3 style, without labels.
const DIFF3_MARKERS: &str = "a\n<<<<<<<\nL\n|||||||\nb\n=======\nR\n>>>>>>>\nc\n";
/// [`CONFLICT`] written in git's merge style, without labels.
const MERGE_MARKERS: &str = "a\n<<<<<<<\nL\n=======\nR\n>>>>>>>\nc\n";

/// [`CONFLICT`] merged headless in a git repository whose config sets
/// `merge.conflictStyle` to `style`, if any, with `args`: the output.
fn git_markers(style: Option<&str>, settings: &str, args: &[&str]) -> String {
    let world = World::new();
    fs::write(world.settings(), settings).expect("settings are written");
    let repo = vcs_git::testing::Fixture::new();
    if let Some(style) = style {
        repo.append_config(&format!("[merge]\n\tconflictStyle = {style}\n"));
    }
    let out = repo.root().join("out.txt");
    let run = world.merge(repo.root().as_std_path(), CONFLICT, out.as_std_path(), args);
    assert_eq!(code(&run), CONFLICTS, "{}", stderr(&run));
    fs::read_to_string(&out).expect("written")
}

#[test]
fn without_git_config_or_a_setting_the_markers_are_diff3() {
    assert_eq!(git_markers(None, "", &[]), DIFF3_MARKERS);
}

#[test]
fn gits_conflict_style_sets_the_markers_without_a_setting() {
    assert_eq!(git_markers(Some("merge"), "", &[]), MERGE_MARKERS);
    assert_eq!(git_markers(Some("diff3"), "", &[]), DIFF3_MARKERS);
    assert_eq!(git_markers(Some("zdiff3"), "", &[]), DIFF3_MARKERS);
}

#[test]
fn a_conflict_style_git_does_not_know_gives_diff3() {
    assert_eq!(git_markers(Some("Merge"), "", &[]), DIFF3_MARKERS);
    assert_eq!(git_markers(Some("ours"), "", &[]), DIFF3_MARKERS);
}

#[test]
fn the_setting_wins_over_gits_conflict_style() {
    let diff3 = "conflict-style = diff3\n";
    assert_eq!(git_markers(Some("merge"), diff3, &[]), DIFF3_MARKERS);
    assert_eq!(git_markers(None, "conflict-style = merge\n", &[]), MERGE_MARKERS);
}

#[test]
fn the_setting_applies_outside_any_repository() {
    let world = World::new();
    fs::write(world.settings(), "conflict-style = merge\n").expect("settings are written");
    let out = world.dir.path().join("out.txt");
    let run = world.merge(world.dir.path(), CONFLICT, &out, &[]);
    assert_eq!(code(&run), CONFLICTS, "{}", stderr(&run));
    assert_eq!(fs::read_to_string(&out).expect("written"), MERGE_MARKERS);
}

#[test]
fn jj_semantics_ignore_the_conflict_style() {
    let world = World::new();
    fs::write(world.settings(), "conflict-style = merge\n").expect("settings are written");
    let repo = vcs_jj::testing::Fixture::new();
    let out = repo.root().join("out.txt");
    let run = world.merge(repo.root().as_std_path(), CONFLICT, out.as_std_path(), &[]);
    assert_eq!(code(&run), CONFLICTS, "{}", stderr(&run));
    assert_eq!(fs::read_to_string(&out).expect("written"), DIFF3_MARKERS);
    let out = world.dir.path().join("out.txt");
    // The config file still asks for merge markers.
    let run = world.merge(world.dir.path(), CONFLICT, &out, &["--merge-semantics", "jj"]);
    assert_eq!(code(&run), CONFLICTS, "{}", stderr(&run));
    assert_eq!(fs::read_to_string(&out).expect("written"), DIFF3_MARKERS);
}
