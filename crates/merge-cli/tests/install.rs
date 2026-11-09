//! `triad-merge install` on the command line: the program it names and what it reports
//! (spec §7.1). `merge-session`'s tests cover the entries themselves.

fn dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("a temporary directory")
}

mod command_line {
    use super::*;
    use merge_cli::{ExitCode, Outcome};

    fn run(args: &[&str]) -> (Outcome, String, String) {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let outcome = merge_cli::run(
            std::iter::once("triad-merge").chain(args.iter().copied()),
            None,
            &mut stdout,
            &mut stderr,
        );
        let text = |bytes| String::from_utf8(bytes).expect("UTF-8");
        (outcome, text(stdout), text(stderr))
    }

    /// This test binary stands in for `triad-merge`: install names the running program, by
    /// its plain absolute path, without Windows' verbatim `\\?\` prefix.
    fn program() -> String {
        std::env::current_exe()
            .and_then(dunce::canonicalize)
            .expect("the test's path")
            .to_str()
            .expect("UTF-8")
            .to_owned()
    }

    #[test]
    fn install_git_writes_the_running_program_and_says_where() {
        let dir = dir();
        let path = dir.path().join("gitconfig");
        let path_str = path.to_str().expect("UTF-8");
        let (outcome, stdout, stderr) = run(&["install", "--git", "--config", path_str]);
        assert_eq!(outcome.exit_code(), Some(ExitCode::Resolved));
        assert_eq!(stderr, "");
        assert_eq!(stdout, format!("triad-merge: installed in {path_str}\n"));
        // Read back through git's parser: the file escapes each `\` of a Windows path.
        let text = std::fs::read_to_string(&path).expect("the config is written");
        let config = gix::config::File::try_from(text.as_str()).expect("git can parse it");
        let cmd = config.string("difftool.triad.cmd").expect("the diff tool is set");
        assert_eq!(
            cmd.to_string(),
            format!(r#"'{}' diff "$LOCAL" "$REMOTE" --title "$MERGED" --readonly-left"#, program())
        );

        let (outcome, stdout, _) = run(&["install", "--git", "--config", path_str]);
        assert_eq!(outcome.exit_code(), Some(ExitCode::Resolved));
        assert_eq!(stdout, format!("triad-merge: already installed in {path_str}\n"));
    }

    #[test]
    fn install_jj_writes_the_running_program() {
        let dir = dir();
        let path = dir.path().join("config.toml");
        let path_str = path.to_str().expect("UTF-8");
        let (outcome, stdout, stderr) = run(&["install", "--jj", "--config", path_str]);
        assert_eq!(outcome.exit_code(), Some(ExitCode::Resolved));
        assert_eq!(stderr, "");
        assert_eq!(stdout, format!("triad-merge: installed in {path_str}\n"));
        let config: toml::Table =
            std::fs::read_to_string(&path).expect("the config is written").parse().expect("TOML");
        assert_eq!(config["merge-tools"]["triad"]["program"].as_str(), Some(program().as_str()));
    }

    #[test]
    fn an_install_that_fails_exits_with_3() {
        let dir = dir();
        let path = dir.path().join("gitconfig");
        std::fs::write(&path, "[broken\n").expect("the config is written");
        let (outcome, stdout, stderr) =
            run(&["install", "--git", "--config", path.to_str().expect("UTF-8")]);
        assert_eq!(outcome.exit_code(), Some(ExitCode::Error));
        assert_eq!(stdout, "");
        assert!(
            stderr.starts_with(&format!("triad-merge: cannot parse {}", path.display())),
            "{stderr}"
        );
    }
}
