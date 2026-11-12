//! `triad-merge install`: the spec §7.1 entries in git and jj config files (DESIGN.md §A7.2).

use std::path::{Path, PathBuf};

use merge_session::env::Env;
use merge_session::install::{self, InstallError, Installed};

const PROGRAM: &str = "/opt/triad/triad-merge";

fn dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("a temporary directory")
}

mod git {
    use super::*;

    /// The values spec §7.1 gives, for [`PROGRAM`].
    const MERGE_CMD: &str =
        r#"'/opt/triad/triad-merge' "$BASE" "$LOCAL" "$REMOTE" "$MERGED" --merge-semantics git"#;
    const DIFF_CMD: &str =
        r#"'/opt/triad/triad-merge' diff "$LOCAL" "$REMOTE" --title "$MERGED" --readonly-left"#;

    fn config(path: &Path) -> gix::config::File {
        let text = std::fs::read_to_string(path).expect("the config is written");
        gix::config::File::try_from(text.as_str()).expect("git can parse the config")
    }

    fn value(config: &gix::config::File, key: &str) -> Option<String> {
        config.string(key).map(|value| value.to_string())
    }

    fn install(path: &Path) -> Installed {
        install::git(path, Path::new(PROGRAM)).expect("the install succeeds")
    }

    /// Dotfile managers link `~/.gitconfig` into their own repository.
    #[cfg(unix)]
    #[test]
    fn a_linked_config_stays_a_link_and_the_linked_file_takes_the_entries() {
        let dir = dir();
        let (file, link) = (dir.path().join("dotfile"), dir.path().join(".gitconfig"));
        std::fs::write(&file, "[user]\n\tname = x\n").expect("seed");
        std::os::unix::fs::symlink(&file, &link).expect("link");
        let installed = install(&link);
        assert_eq!((installed.path.as_path(), installed.changed), (link.as_path(), true));
        assert_eq!(std::fs::read_link(&link).expect("still a link"), file);
        assert_entries(&file);
        assert_eq!(value(&config(&file), "user.name").as_deref(), Some("x"));
    }

    fn assert_entries(path: &Path) {
        let config = config(path);
        assert_eq!(value(&config, "merge.tool").as_deref(), Some("triad"));
        assert_eq!(value(&config, "mergetool.triad.cmd").as_deref(), Some(MERGE_CMD));
        assert_eq!(config.boolean("mergetool.triad.trustExitCode").ok(), Some(Some(true)));
        assert_eq!(value(&config, "diff.tool").as_deref(), Some("triad"));
        assert_eq!(value(&config, "difftool.triad.cmd").as_deref(), Some(DIFF_CMD));
    }

    #[test]
    fn an_absent_config_is_created_with_its_directory() {
        let dir = dir();
        let path = dir.path().join("git/config");
        assert_eq!(install(&path), Installed { path: path.clone(), changed: true });
        assert_entries(&path);
    }

    #[test]
    fn another_tool_and_other_settings_are_kept() {
        let dir = dir();
        let path = dir.path().join(".gitconfig");
        let before = "# my settings\n[user]\n\tname = Someone\n[merge]\n\ttool = meld\n\
                      [mergetool \"meld\"]\n\tcmd = meld \\\"$LOCAL\\\" \\\"$MERGED\\\" \\\"$REMOTE\\\"\n";
        std::fs::write(&path, before).expect("the config is written");
        assert!(install(&path).changed);
        assert_entries(&path);
        let config = config(&path);
        assert_eq!(value(&config, "user.name").as_deref(), Some("Someone"));
        assert_eq!(
            value(&config, "mergetool.meld.cmd").as_deref(),
            Some(r#"meld "$LOCAL" "$MERGED" "$REMOTE""#)
        );
        let after = std::fs::read_to_string(&path).expect("the config reads");
        assert!(after.starts_with("# my settings\n"), "{after}");
    }

    #[test]
    fn a_second_install_changes_nothing() {
        let dir = dir();
        let path = dir.path().join(".gitconfig");
        install(&path);
        let first = std::fs::read(&path).expect("the config reads");
        assert!(!install(&path).changed);
        assert_eq!(std::fs::read(&path).expect("the config reads"), first);
    }

    #[test]
    fn an_old_triad_entry_is_replaced_not_repeated() {
        let dir = dir();
        let path = dir.path().join(".gitconfig");
        install::git(&path, Path::new("/old/triad-merge")).expect("the first install");
        assert!(install(&path).changed);
        assert_entries(&path);
        let text = std::fs::read_to_string(&path).expect("the config reads");
        assert_eq!(text.matches("[mergetool \"triad\"]").count(), 1, "{text}");
        assert_eq!(text.matches("cmd =").count(), 2, "{text}");
        assert!(!text.contains("/old/"), "{text}");
    }

    #[test]
    fn a_config_git_cannot_parse_is_refused_and_left_alone() {
        let dir = dir();
        let path = dir.path().join(".gitconfig");
        let broken = "[merge\n\ttool = meld\n";
        std::fs::write(&path, broken).expect("the config is written");
        match install::git(&path, Path::new(PROGRAM)) {
            Err(InstallError::Parse { path: named, .. }) => assert_eq!(named, path),
            other => panic!("expected Parse, got {other:?}"),
        }
        assert_eq!(std::fs::read_to_string(&path).expect("reads"), broken);
    }

    #[test]
    fn a_config_that_is_a_directory_is_named() {
        let dir = dir();
        match install::git(dir.path(), Path::new(PROGRAM)) {
            Err(InstallError::Read { path, .. }) => assert_eq!(path, dir.path()),
            other => panic!("expected Read, got {other:?}"),
        }
    }

    fn env(home: &Path) -> Env {
        Env { home: Some(home.to_owned()), ..Env::default() }
    }

    fn touch(path: &Path) -> PathBuf {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("dirs");
        std::fs::write(path, "").expect("the file is written");
        path.to_owned()
    }

    #[test]
    fn the_global_config_is_gitconfig_in_home_by_default() {
        let home = dir();
        assert_eq!(
            install::git_config_path(&env(home.path())).expect("a path"),
            home.path().join(".gitconfig")
        );
    }

    #[test]
    fn an_existing_xdg_config_is_used_when_there_is_no_gitconfig() {
        let home = dir();
        let xdg = touch(&home.path().join(".config/git/config"));
        assert_eq!(install::git_config_path(&env(home.path())).expect("a path"), xdg);
        let custom = dir();
        let custom_xdg = touch(&custom.path().join("git/config"));
        let with_xdg = Env { xdg_config_home: Some(custom.path().to_owned()), ..env(home.path()) };
        assert_eq!(install::git_config_path(&with_xdg).expect("a path"), custom_xdg);
    }

    #[test]
    fn an_existing_gitconfig_wins_over_xdg() {
        let home = dir();
        touch(&home.path().join(".config/git/config"));
        let gitconfig = touch(&home.path().join(".gitconfig"));
        assert_eq!(install::git_config_path(&env(home.path())).expect("a path"), gitconfig);
    }

    #[test]
    fn git_config_global_wins_over_everything() {
        let home = dir();
        touch(&home.path().join(".gitconfig"));
        let chosen = home.path().join("elsewhere");
        let with_override = Env { git_config_global: Some(chosen.clone()), ..env(home.path()) };
        assert_eq!(install::git_config_path(&with_override).expect("a path"), chosen);
    }

    #[test]
    fn without_a_home_there_is_no_global_config() {
        match install::git_config_path(&Env::default()) {
            Err(InstallError::NoHome) => {}
            other => panic!("expected NoHome, got {other:?}"),
        }
    }
}

mod jj {
    use super::*;

    const WINDOWS_PROGRAM: &str = r"C:\Program Files\triad\triad-merge.exe";

    fn table(path: &Path) -> toml::Table {
        let text = std::fs::read_to_string(path).expect("the config is written");
        text.parse().expect("the config is TOML")
    }

    fn strings(values: &[&str]) -> toml::Value {
        toml::Value::Array(values.iter().map(|&v| toml::Value::from(v)).collect())
    }

    fn install(path: &Path, program: &str) -> Installed {
        install::jj(path, Path::new(program)).expect("the install succeeds")
    }

    /// The entries of spec §7.1, for `program`.
    fn assert_entries(path: &Path, program: &str) {
        let config = table(path);
        assert_eq!(config["ui"]["merge-editor"].as_str(), Some("triad"));
        let tool = &config["merge-tools"]["triad"];
        assert_eq!(tool["program"].as_str(), Some(program));
        assert_eq!(
            tool["merge-args"],
            strings(&[
                "$base",
                "$left",
                "$right",
                "-o",
                "$output",
                "--title",
                "$path",
                "--marker-length",
                "$marker_length",
                "--merge-semantics",
                "jj",
            ])
        );
        assert_eq!(tool["merge-tool-edits-conflict-markers"].as_bool(), Some(true));
        assert_eq!(
            tool["merge-conflict-exit-codes"],
            toml::Value::Array(vec![toml::Value::Integer(1)])
        );
        assert_eq!(tool["conflict-marker-style"].as_str(), Some("git"));
        assert_eq!(tool["diff-args"], strings(&["diff", "$left", "$right"]));
        assert_eq!(tool["diff-invocation-mode"].as_str(), Some("file-by-file"));
        assert_eq!(tool["diff-do-chdir"].as_bool(), Some(false));
    }

    #[test]
    fn an_absent_config_is_created_with_its_directory() {
        let dir = dir();
        let path = dir.path().join("jj/config.toml");
        assert_eq!(install(&path, PROGRAM), Installed { path: path.clone(), changed: true });
        assert_entries(&path, PROGRAM);
    }

    #[test]
    fn a_windows_path_survives_toml_quoting() {
        let dir = dir();
        let path = dir.path().join("config.toml");
        install(&path, WINDOWS_PROGRAM);
        assert_entries(&path, WINDOWS_PROGRAM);
    }

    #[test]
    fn another_tool_and_other_settings_are_kept() {
        let dir = dir();
        let path = dir.path().join("config.toml");
        let before = "# my settings\n[user]\nname = \"Someone\"\n\n[ui]\nmerge-editor = \"meld\"\n\n\
                      [merge-tools.meld]\nmerge-args = [\"$left\", \"$base\", \"$right\", \"-o\", \"$output\"]\n";
        std::fs::write(&path, before).expect("the config is written");
        assert!(install(&path, PROGRAM).changed);
        assert_entries(&path, PROGRAM);
        let config = table(&path);
        assert_eq!(config["user"]["name"].as_str(), Some("Someone"));
        assert_eq!(
            config["merge-tools"]["meld"]["merge-args"],
            strings(&["$left", "$base", "$right", "-o", "$output"])
        );
        let after = std::fs::read_to_string(&path).expect("the config reads");
        assert!(after.starts_with("# my settings\n"), "{after}");
    }

    #[test]
    fn a_second_install_changes_nothing() {
        let dir = dir();
        let path = dir.path().join("config.toml");
        install(&path, PROGRAM);
        let first = std::fs::read(&path).expect("the config reads");
        assert!(!install(&path, PROGRAM).changed);
        assert_eq!(std::fs::read(&path).expect("the config reads"), first);
    }

    #[test]
    fn an_old_triad_entry_is_replaced_not_repeated() {
        let dir = dir();
        let path = dir.path().join("config.toml");
        install(&path, "/old/triad-merge");
        assert!(install(&path, PROGRAM).changed);
        assert_entries(&path, PROGRAM);
        let text = std::fs::read_to_string(&path).expect("the config reads");
        assert!(!text.contains("/old/"), "{text}");
    }

    #[test]
    fn a_config_jj_cannot_parse_is_refused_and_left_alone() {
        let dir = dir();
        let path = dir.path().join("config.toml");
        let broken = "[ui\nmerge-editor = \"meld\"\n";
        std::fs::write(&path, broken).expect("the config is written");
        match install::jj(&path, Path::new(PROGRAM)) {
            Err(InstallError::Parse { path: named, .. }) => assert_eq!(named, path),
            other => panic!("expected Parse, got {other:?}"),
        }
        assert_eq!(std::fs::read_to_string(&path).expect("reads"), broken);
    }

    #[test]
    fn a_tool_that_is_not_a_table_is_refused_and_left_alone() {
        let dir = dir();
        let path = dir.path().join("config.toml");
        let odd = "merge-tools = \"none\"\n";
        std::fs::write(&path, odd).expect("the config is written");
        match install::jj(&path, Path::new(PROGRAM)) {
            Err(InstallError::Set { path: named, key, .. }) => {
                assert_eq!(named, path);
                assert_eq!(key, "merge-tools.triad.program");
            }
            other => panic!("expected Set, got {other:?}"),
        }
        assert_eq!(std::fs::read_to_string(&path).expect("reads"), odd);
    }

    fn env(home: &Path, config_dir: &Path) -> Env {
        Env {
            home: Some(home.to_owned()),
            user_config_dir: Some(config_dir.to_owned()),
            ..Env::default()
        }
    }

    #[test]
    fn the_user_config_is_in_the_platform_config_directory_by_default() {
        let (home, config) = (dir(), dir());
        assert_eq!(
            install::jj_config_path(&env(home.path(), config.path())).expect("a path"),
            config.path().join("jj/config.toml")
        );
    }

    #[test]
    fn an_existing_jjconfig_in_home_wins() {
        let (home, config) = (dir(), dir());
        let jjconfig = home.path().join(".jjconfig.toml");
        std::fs::write(&jjconfig, "").expect("the file is written");
        assert_eq!(
            install::jj_config_path(&env(home.path(), config.path())).expect("a path"),
            jjconfig
        );
    }

    #[test]
    fn jj_config_names_the_file_and_its_first_entry_wins() {
        let (home, config) = (dir(), dir());
        let first = home.path().join("first.toml");
        let list = std::env::join_paths([first.as_path(), &home.path().join("second.toml")])
            .expect("the paths join");
        let with_list = Env { jj_config: Some(list), ..env(home.path(), config.path()) };
        assert_eq!(install::jj_config_path(&with_list).expect("a path"), first);
    }

    #[test]
    fn a_jj_config_directory_is_refused() {
        let (home, config) = (dir(), dir());
        let with_dir = Env {
            jj_config: Some(config.path().as_os_str().to_owned()),
            ..env(home.path(), config.path())
        };
        match install::jj_config_path(&with_dir) {
            Err(InstallError::ConfigDirectory(path)) => assert_eq!(path, config.path()),
            other => panic!("expected ConfigDirectory, got {other:?}"),
        }
    }

    #[test]
    fn without_a_config_directory_there_is_no_user_config() {
        match install::jj_config_path(&Env::default()) {
            Err(InstallError::NoHome) => {}
            other => panic!("expected NoHome, got {other:?}"),
        }
    }
}

#[cfg(unix)]
mod failures {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt as _;
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;

    const READ_ONLY: u32 = 0o555;
    const WRITABLE: u32 = 0o755;

    #[test]
    fn a_program_path_that_is_not_utf8_is_refused_and_writes_nothing() {
        let dir = dir();
        let config = dir.path().join("config");
        let program = Path::new(OsStr::from_bytes(b"/opt/\xff/triad-merge"));
        for result in [install::git(&config, program), install::jj(&config, program)] {
            assert!(
                matches!(&result, Err(InstallError::NotUtf8(path)) if path == program),
                "{result:?}"
            );
        }
        assert!(!config.exists());
    }

    #[test]
    fn a_config_directory_that_cannot_be_made_is_named_and_writes_nothing() {
        let dir = dir();
        let locked = dir.path().join("locked");
        std::fs::create_dir(&locked).expect("the directory is made");
        let set_mode = |mode| {
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(mode))
                .expect("chmod");
        };
        let wanted = locked.join("new");
        let config = wanted.join("config");
        set_mode(READ_ONLY);
        let results =
            [install::git(&config, Path::new(PROGRAM)), install::jj(&config, Path::new(PROGRAM))];
        set_mode(WRITABLE);
        for result in results {
            assert!(
                matches!(&result, Err(InstallError::Dir { path, .. }) if *path == wanted),
                "{result:?}"
            );
        }
        assert!(!wanted.exists());
    }
}

mod dispatch {
    use super::*;
    use vcs::RepoKind;

    #[test]
    fn without_a_config_each_vcs_writes_the_file_its_env_names() {
        let dir = dir();
        let (git, jj) = (dir.path().join("g"), dir.path().join("j.toml"));
        let env = Env {
            git_config_global: Some(git.clone()),
            jj_config: Some(jj.clone().into_os_string()),
            ..Env::default()
        };
        let program = Path::new(PROGRAM);
        let installed = install::install(RepoKind::Git, None, program, &env).expect("git");
        assert_eq!(installed.path, git);
        let installed = install::install(RepoKind::Jj, None, program, &env).expect("jj");
        assert_eq!(installed.path, jj);
        assert!(std::fs::read_to_string(&git).expect("git").contains("[mergetool \"triad\"]"));
        assert!(std::fs::read_to_string(&jj).expect("jj").contains("[merge-tools.triad]"));
    }

    #[test]
    fn a_given_config_wins_over_the_env() {
        let dir = dir();
        let given = dir.path().join("given");
        let env = Env { git_config_global: Some(dir.path().join("unused")), ..Env::default() };
        let installed =
            install::install(RepoKind::Git, Some(&given), Path::new(PROGRAM), &env).expect("git");
        assert_eq!(installed.path, given);
        assert!(!dir.path().join("unused").exists());
    }

    #[test]
    fn without_a_config_or_a_home_nothing_is_written() {
        let result = install::install(RepoKind::Jj, None, Path::new(PROGRAM), &Env::default());
        assert!(matches!(result, Err(InstallError::NoHome)), "{result:?}");
    }
}
