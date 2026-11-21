//! The language a pane's text is highlighted and merged as, from file names
//! (`docs/DESIGN.md` §F5), and its grammar.
//!
//! git and jj hand a merge tool temporary inputs such as `BASE_1234.rs`, and jj's diff tool
//! files in a temporary directory, so the caller lists every name it knows, best first: the
//! inputs, then the output, then the `--title` path. The first name whose whole name or
//! extension triad knows gives the language. Without one, a script's `#!` line names it. None is plain
//! text.

use std::borrow::Cow;
use std::path::Path;

/// A language with a grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Go,
    CSharp,
    Python,
    Swift,
    Html,
    JavaScript,
    TypeScript,
    Tsx,
    Json,
    Yaml,
    Toml,
    Css,
    C,
    Cpp,
    Java,
    Kotlin,
    Markdown,
    Bash,
    Zig,
}

/// Each language's file extensions, in lowercase.
const EXTENSIONS: [(&str, Language); 45] = [
    ("rs", Language::Rust),
    ("go", Language::Go),
    ("cs", Language::CSharp),
    ("py", Language::Python),
    ("pyi", Language::Python),
    ("swift", Language::Swift),
    ("html", Language::Html),
    ("htm", Language::Html),
    ("js", Language::JavaScript),
    ("mjs", Language::JavaScript),
    ("cjs", Language::JavaScript),
    ("jsx", Language::JavaScript),
    ("ts", Language::TypeScript),
    ("mts", Language::TypeScript),
    ("cts", Language::TypeScript),
    ("tsx", Language::Tsx),
    ("json", Language::Json),
    ("jsonc", Language::Json),
    ("yaml", Language::Yaml),
    ("yml", Language::Yaml),
    ("toml", Language::Toml),
    ("css", Language::Css),
    ("c", Language::C),
    // C++'s grammar parses C headers too, so `.h` takes C++, as in Zed.
    ("cc", Language::Cpp),
    ("cpp", Language::Cpp),
    ("cxx", Language::Cpp),
    ("c++", Language::Cpp),
    ("h", Language::Cpp),
    ("hh", Language::Cpp),
    ("hpp", Language::Cpp),
    ("hxx", Language::Cpp),
    ("h++", Language::Cpp),
    ("inl", Language::Cpp),
    ("ipp", Language::Cpp),
    ("java", Language::Java),
    ("kt", Language::Kotlin),
    ("kts", Language::Kotlin),
    ("md", Language::Markdown),
    ("markdown", Language::Markdown),
    // zsh scripts parse as Bash, as in Zed.
    ("sh", Language::Bash),
    ("bash", Language::Bash),
    ("zsh", Language::Bash),
    ("bats", Language::Bash),
    ("ebuild", Language::Bash),
    ("zig", Language::Zig),
];

/// Files whose whole name gives their language, in their own case: shell startup files and
/// package build scripts, as in Zed.
const FILE_NAMES: [(&str, Language); 14] = [
    (".bashrc", Language::Bash),
    (".bash_profile", Language::Bash),
    (".bash_aliases", Language::Bash),
    (".bash_login", Language::Bash),
    (".bash_logout", Language::Bash),
    (".profile", Language::Bash),
    (".zshrc", Language::Bash),
    (".zshenv", Language::Bash),
    (".zprofile", Language::Bash),
    (".zlogin", Language::Bash),
    (".zlogout", Language::Bash),
    (".envrc", Language::Bash),
    ("PKGBUILD", Language::Bash),
    ("APKBUILD", Language::Bash),
];

/// The bytes that open a script's interpreter line.
const SHEBANG: &str = "#!";
/// The program that finds the interpreter on `PATH`: `#!/usr/bin/env bash`.
const ENV: &str = "env";

/// Each interpreter a script's first line can name, without its version, and its language.
/// zsh, dash and ksh scripts parse as Bash, as `.zsh` files do.
const INTERPRETERS: [(&str, Language); 8] = [
    ("sh", Language::Bash),
    ("bash", Language::Bash),
    ("zsh", Language::Bash),
    ("dash", Language::Bash),
    ("ksh", Language::Bash),
    ("python", Language::Python),
    ("node", Language::JavaScript),
    ("swift", Language::Swift),
];

/// The last component of a `/`-separated path.
fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

impl Language {
    /// Every language, in declaration order.
    pub const ALL: [Self; 20] = [
        Self::Rust,
        Self::Go,
        Self::CSharp,
        Self::Python,
        Self::Swift,
        Self::Html,
        Self::JavaScript,
        Self::TypeScript,
        Self::Tsx,
        Self::Json,
        Self::Yaml,
        Self::Toml,
        Self::Css,
        Self::C,
        Self::Cpp,
        Self::Java,
        Self::Kotlin,
        Self::Markdown,
        Self::Bash,
        Self::Zig,
    ];

    /// The language's tree-sitter grammar.
    #[must_use]
    pub fn grammar(self) -> tree_sitter::Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE,
            Self::Go => tree_sitter_go::LANGUAGE,
            Self::CSharp => tree_sitter_c_sharp::LANGUAGE,
            Self::Python => tree_sitter_python::LANGUAGE,
            Self::Swift => tree_sitter_swift::LANGUAGE,
            Self::Html => tree_sitter_html::LANGUAGE,
            Self::JavaScript => tree_sitter_javascript::LANGUAGE,
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX,
            Self::Json => tree_sitter_json::LANGUAGE,
            Self::Yaml => tree_sitter_yaml::LANGUAGE,
            Self::Toml => tree_sitter_toml_ng::LANGUAGE,
            Self::Css => tree_sitter_css::LANGUAGE,
            Self::C => tree_sitter_c::LANGUAGE,
            Self::Cpp => tree_sitter_cpp::LANGUAGE,
            Self::Java => tree_sitter_java::LANGUAGE,
            Self::Kotlin => tree_sitter_kotlin_sg::LANGUAGE,
            Self::Markdown => tree_sitter_md::LANGUAGE,
            Self::Bash => tree_sitter_bash::LANGUAGE,
            Self::Zig => tree_sitter_zig::LANGUAGE,
        }
        .into()
    }

    /// The highlight query triad gives this language's grammar where the editors' bundled
    /// query is missing or weaker; none to keep the bundled one. C++ takes C's query first,
    /// since `tree-sitter-cpp`'s query holds only the lines C++ adds.
    #[must_use]
    pub fn highlight_query(self) -> Option<Cow<'static, str>> {
        match self {
            Self::CSharp => Some(Cow::Borrowed(tree_sitter_c_sharp::HIGHLIGHTS_QUERY)),
            Self::Swift => Some(Cow::Borrowed(tree_sitter_swift::HIGHLIGHTS_QUERY)),
            Self::Kotlin => Some(Cow::Borrowed(tree_sitter_kotlin_sg::HIGHLIGHTS_QUERY)),
            Self::Cpp => Some(Cow::Owned(
                [tree_sitter_c::HIGHLIGHT_QUERY, tree_sitter_cpp::HIGHLIGHT_QUERY].concat(),
            )),
            Self::Rust
            | Self::Go
            | Self::Python
            | Self::Html
            | Self::JavaScript
            | Self::TypeScript
            | Self::Tsx
            | Self::Json
            | Self::Yaml
            | Self::Toml
            | Self::Css
            | Self::C
            | Self::Java
            | Self::Markdown
            | Self::Bash
            | Self::Zig => None,
        }
    }

    /// The language of a file named `path`: by its whole name, else by its extension in any
    /// case.
    #[must_use]
    pub fn of(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_str()?;
        if let Some(&(_, language)) = FILE_NAMES.iter().find(|(known, _)| *known == name) {
            return Some(language);
        }
        let extension = path.extension()?.to_str()?;
        EXTENSIONS
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(extension))
            .map(|&(_, language)| language)
    }

    /// The language a script's first line names: `#!`, then the interpreter's path, or
    /// `env` and the interpreter, after `env`'s options and assignments. A version after
    /// the interpreter's name counts for nothing: `python3.12` is `python`.
    #[must_use]
    pub fn of_shebang(text: &str) -> Option<Self> {
        let line = text.lines().next()?.strip_prefix(SHEBANG)?;
        let mut words = line.split_whitespace();
        let mut program = file_name(words.next()?);
        if program == ENV {
            program = words.find(|word| !word.starts_with('-') && !word.contains('='))?;
            program = file_name(program);
        }
        let name = program.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
        INTERPRETERS.iter().find(|(known, _)| *known == name).map(|&(_, language)| language)
    }

    /// The language of the first of `texts` whose first line names one.
    pub fn of_scripts<'a>(texts: impl IntoIterator<Item = &'a str>) -> Option<Self> {
        texts.into_iter().find_map(Self::of_shebang)
    }

    /// The language of the first of `names` that has one.
    pub fn detect<'a>(names: impl IntoIterator<Item = &'a Path>) -> Option<Self> {
        names.into_iter().find_map(Self::of)
    }

    /// The language of a run's file names (spec §4.3): the inputs', then the output's, then
    /// the title's. Both the merge and the colours take it.
    pub fn of_run<'a>(
        inputs: impl IntoIterator<Item = &'a Path>,
        output: Option<&'a Path>,
        title: Option<&'a str>,
    ) -> Option<Self> {
        Self::detect(inputs.into_iter().chain(output).chain(title.map(Path::new)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(name: &str) -> Option<Language> {
        Language::of(Path::new(name))
    }

    #[test]
    fn each_extension_names_its_language() {
        assert_eq!(of("src/a.rs"), Some(Language::Rust));
        assert_eq!(of("main.go"), Some(Language::Go));
        assert_eq!(of("Program.cs"), Some(Language::CSharp));
        assert_eq!(of("main.py"), Some(Language::Python));
        assert_eq!(of("stubs.pyi"), Some(Language::Python));
        assert_eq!(of("App.swift"), Some(Language::Swift));
        assert_eq!(of("index.html"), Some(Language::Html));
        assert_eq!(of("old.htm"), Some(Language::Html));
        assert_eq!(of("app.js"), Some(Language::JavaScript));
        assert_eq!(of("mod.mjs"), Some(Language::JavaScript));
        assert_eq!(of("conf.cjs"), Some(Language::JavaScript));
        assert_eq!(of("View.jsx"), Some(Language::JavaScript));
        assert_eq!(of("app.ts"), Some(Language::TypeScript));
        assert_eq!(of("mod.mts"), Some(Language::TypeScript));
        assert_eq!(of("conf.cts"), Some(Language::TypeScript));
        assert_eq!(of("index.d.ts"), Some(Language::TypeScript));
        assert_eq!(of("View.tsx"), Some(Language::Tsx));
        assert_eq!(of("package.json"), Some(Language::Json));
        assert_eq!(of("tsconfig.jsonc"), Some(Language::Json));
        assert_eq!(of("ci.yaml"), Some(Language::Yaml));
        assert_eq!(of(".github/workflows/triad.yml"), Some(Language::Yaml));
        assert_eq!(of("Cargo.toml"), Some(Language::Toml));
        assert_eq!(of("rust-toolchain.toml"), Some(Language::Toml));
        assert_eq!(of("style.css"), Some(Language::Css));
        assert_eq!(of("src/main.c"), Some(Language::C));
        assert_eq!(of("src/Main.java"), Some(Language::Java));
        assert_eq!(of("App.kt"), Some(Language::Kotlin));
        assert_eq!(of("build.gradle.kts"), Some(Language::Kotlin));
        assert_eq!(of("README.md"), Some(Language::Markdown));
        assert_eq!(of("notes.markdown"), Some(Language::Markdown));
        assert_eq!(of("script/build.sh"), Some(Language::Bash));
        assert_eq!(of("install.bash"), Some(Language::Bash));
        assert_eq!(of("prompt.zsh"), Some(Language::Bash));
        assert_eq!(of("test/cli.bats"), Some(Language::Bash));
        assert_eq!(of("src/main.zig"), Some(Language::Zig));
        for name in [
            "a.cc", "a.cpp", "a.cxx", "a.c++", "a.h", "a.hh", "a.hpp", "a.hxx", "a.h++", "a.inl",
            "a.ipp",
        ] {
            assert_eq!(of(name), Some(Language::Cpp), "{name}");
        }
    }

    #[test]
    fn a_shell_startup_file_or_build_script_is_bash_by_its_name() {
        for name in [
            ".bashrc",
            "home/.bash_profile",
            ".bash_aliases",
            ".bash_login",
            ".bash_logout",
            ".profile",
            "dotfiles/.zshrc",
            ".zshenv",
            ".zprofile",
            ".zlogin",
            ".zlogout",
            ".envrc",
            "PKGBUILD",
            "aports/main/zlib/APKBUILD",
            "dev-libs/zlib-1.3.ebuild",
            "A.EBUILD",
        ] {
            assert_eq!(of(name), Some(Language::Bash), "{name}");
        }
    }

    #[test]
    fn a_file_name_matches_only_whole_and_in_its_own_case() {
        for name in [
            "bashrc",
            ".bashrc.orig",
            ".bashrc_BASE_1234",
            "a.bashrc",
            ".BASHRC",
            "profile",
            ".profiles",
            "pkgbuild",
            "PKGBUILD.orig",
            ".PKGBUILD",
            "PKGBUILD/a",
        ] {
            assert_eq!(of(name), None, "{name}");
        }
    }

    #[test]
    fn gits_temporary_inputs_of_a_dotfile_take_the_outputs_name() {
        let inputs = [".bashrc_LOCAL_1234", ".bashrc_BASE_1234", ".bashrc_REMOTE_1234"];
        let language = Language::of_run(inputs.map(Path::new), Some(Path::new(".bashrc")), None);
        assert_eq!(language, Some(Language::Bash));
    }

    #[test]
    fn the_extension_matches_in_any_case() {
        assert_eq!(of("A.RS"), Some(Language::Rust));
        assert_eq!(of("x.Go"), Some(Language::Go));
        assert_eq!(of("P.CS"), Some(Language::CSharp));
        assert_eq!(of("A.PY"), Some(Language::Python));
        assert_eq!(of("A.Swift"), Some(Language::Swift));
        assert_eq!(of("INDEX.HTML"), Some(Language::Html));
        assert_eq!(of("APP.JS"), Some(Language::JavaScript));
        assert_eq!(of("App.Ts"), Some(Language::TypeScript));
        assert_eq!(of("VIEW.TSX"), Some(Language::Tsx));
        assert_eq!(of("A.JSON"), Some(Language::Json));
        assert_eq!(of("a.JsonC"), Some(Language::Json));
        assert_eq!(of("A.YAML"), Some(Language::Yaml));
        assert_eq!(of("b.Yml"), Some(Language::Yaml));
        assert_eq!(of("CARGO.TOML"), Some(Language::Toml));
        assert_eq!(of("STYLE.CSS"), Some(Language::Css));
        assert_eq!(of("MAIN.C"), Some(Language::C));
        assert_eq!(of("A.CPP"), Some(Language::Cpp));
        assert_eq!(of("A.H"), Some(Language::Cpp));
        assert_eq!(of("MAIN.JAVA"), Some(Language::Java));
        assert_eq!(of("App.KT"), Some(Language::Kotlin));
        assert_eq!(of("b.Kts"), Some(Language::Kotlin));
        assert_eq!(of("README.MD"), Some(Language::Markdown));
        assert_eq!(of("a.MarkDown"), Some(Language::Markdown));
        assert_eq!(of("A.SH"), Some(Language::Bash));
        assert_eq!(of("a.Bash"), Some(Language::Bash));
        assert_eq!(of("A.ZSH"), Some(Language::Bash));
        assert_eq!(of("a.BATS"), Some(Language::Bash));
        assert_eq!(of("MAIN.ZIG"), Some(Language::Zig));
    }

    #[test]
    fn a_temporary_input_keeps_its_extension() {
        assert_eq!(of("BASE_1234.rs"), Some(Language::Rust));
        assert_eq!(of("/tmp/jj-resolve-x/a.go"), Some(Language::Go));
    }

    #[test]
    fn other_names_are_plain() {
        for name in [
            "a.txt",
            "Makefile",
            "a",
            ".rs",
            "a.rs.orig",
            "a.rsx",
            "a.",
            "",
            "rs",
            "a.pyc",
            "a.xhtml",
            "a.swiftmodule",
            "a.js.map",
            "a.tsbuildinfo",
            "a.json5",
            "a.jsonl",
            "a.json.orig",
            "a.yaml.orig",
            "a.ymlx",
            "Cargo.lock",
            "a.toml.orig",
            "a.tml",
            "a.scss",
            "a.less",
            "a.css.map",
            "a.c.orig",
            "a.cl",
            "a.cp",
            "a.hpp.orig",
            "a.c+",
            "a.class",
            "a.jar",
            "a.jav",
            "a.java.orig",
            "a.k",
            "a.ktm",
            "build.gradle",
            "a.mdx",
            "a.m",
            "a.md.orig",
            "a.mkdn",
            "a.csh",
            "a.fish",
            "a.ksh",
            "a.ps1",
            "a.sh.orig",
            "a.shx",
            "a.zi",
            "a.zigx",
            "a.zig.orig",
            "a.zo",
            "a.z",
            // The Zig grammar parses declarations only, so a ZON manifest stays plain.
            "build.zig.zon",
        ] {
            assert_eq!(of(name), None, "{name}");
        }
    }

    #[test]
    fn the_first_name_with_a_language_wins() {
        let detect = |names: &[&str]| Language::detect(names.iter().map(Path::new));
        assert_eq!(detect(&["BASE_1234", "src/a.go"]), Some(Language::Go));
        assert_eq!(detect(&["a.txt", "b.cs", "c.rs"]), Some(Language::CSharp));
        assert_eq!(detect(&["a.rs", "b.go"]), Some(Language::Rust));
        assert_eq!(detect(&["a", "b.txt"]), None);
        assert_eq!(detect(&[]), None);
    }

    #[test]
    fn a_run_takes_the_inputs_then_the_output_then_the_title() {
        let inputs = [Path::new("BASE_1"), Path::new("LEFT_1")];
        let run = |inputs: &[&'static Path], output: Option<&'static str>, title| {
            Language::of_run(inputs.iter().copied(), output.map(Path::new), title)
        };
        assert_eq!(
            run(&[Path::new("a.go"), Path::new("b")], Some("o.rs"), Some("t.cs")),
            Some(Language::Go)
        );
        assert_eq!(run(&inputs, Some("o.rs"), Some("t.cs")), Some(Language::Rust));
        assert_eq!(run(&inputs, Some("out"), Some("t.cs")), Some(Language::CSharp));
        assert_eq!(run(&inputs, None, None), None);
    }

    #[test]
    fn a_shebang_names_its_interpreters_language() {
        for (line, language) in [
            ("#!/bin/sh\n", Language::Bash),
            ("#!/bin/bash\necho a\n", Language::Bash),
            ("#!/usr/bin/zsh\n", Language::Bash),
            ("#!/bin/dash\n", Language::Bash),
            ("#!/bin/ksh\n", Language::Bash),
            ("#! /bin/sh\n", Language::Bash),
            ("#!/bin/sh -eu\n", Language::Bash),
            ("#!/bin/sh\r\necho a\r\n", Language::Bash),
            ("#!/bin/sh", Language::Bash),
            ("#!/usr/bin/env bash\n", Language::Bash),
            ("#!/usr/bin/env\tbash\n", Language::Bash),
            ("#!/usr/bin/env -S bash -e\n", Language::Bash),
            ("#!/usr/bin/env -i LANG=C sh\n", Language::Bash),
            ("#!/usr/bin/python\n", Language::Python),
            ("#!/usr/bin/env python3\n", Language::Python),
            ("#!/usr/local/bin/python3.12 -u\n", Language::Python),
            ("#!/usr/bin/env node\n", Language::JavaScript),
            ("#!/usr/bin/swift\n", Language::Swift),
        ] {
            assert_eq!(Language::of_shebang(line), Some(language), "{line:?}");
        }
    }

    #[test]
    fn other_first_lines_name_no_language() {
        for line in [
            "",
            "\n",
            "#!",
            "#!\n/bin/sh\n",
            "#!/usr/bin/env\n",
            "#!/usr/bin/env -S\n",
            "#!/usr/bin/env LANG=C\n",
            "# /bin/sh\n",
            "#/bin/sh\n",
            " #!/bin/sh\n",
            "echo a\n#!/bin/sh\n",
            "#!/usr/bin/perl\n",
            "#!/usr/bin/fish\n",
            "#!/usr/bin/env shellcheck\n",
            "#!/bin/bashful\n",
            "#!/usr/bin/python3x\n",
            "#!/usr/bin/env 3\n",
            "#!/bin/\n",
            "#!/usr/bin/env sh.d\n",
        ] {
            assert_eq!(Language::of_shebang(line), None, "{line:?}");
        }
    }

    #[test]
    fn the_first_script_with_a_shebang_wins() {
        let of = |texts: &[&str]| Language::of_scripts(texts.iter().copied());
        assert_eq!(of(&["a\n", "#!/bin/sh\n", "#!/usr/bin/env python3\n"]), Some(Language::Bash));
        assert_eq!(of(&["#!/usr/bin/perl\n", "#!/usr/bin/env node\n"]), Some(Language::JavaScript));
        assert_eq!(of(&["a\n", "", "b\n"]), None);
        assert_eq!(of(&[]), None);
    }

    /// Each language's sample, and the kind of its tree's root.
    const SAMPLES: [(Language, &str, &str); 20] = [
        (Language::Rust, "fn a() -> u8 { 1 }\n", "source_file"),
        (Language::Go, "package a\nfunc b() {}\n", "source_file"),
        (Language::CSharp, "class A { void B() {} }\n", "compilation_unit"),
        (Language::Python, "def a():\n    return 1\n", "module"),
        (Language::Swift, "func a() -> Int { 1 }\n", "source_file"),
        (Language::Html, "<p>a</p>\n", "document"),
        (Language::JavaScript, "const a = () => 1;\n", "program"),
        (Language::TypeScript, "const a: number = 1;\n", "program"),
        (Language::Tsx, "const a = <div>{b as number}</div>;\n", "program"),
        (Language::Json, "// c\n{\"a\": [1, true, null]}\n", "document"),
        (Language::Yaml, "# c\na: [1, true]\nb:\n  - c\n", "stream"),
        (Language::Toml, "# c\n[a]\nb = [1, true]\n", "document"),
        (Language::Css, "/* c */\na > b { color: #fff; }\n", "stylesheet"),
        (Language::C, "// c\nstatic int a(void) { return 1; }\n", "translation_unit"),
        (Language::Cpp, "// c\nclass A { int b() const; };\n", "translation_unit"),
        (Language::Java, "// c\nclass A { int b() { return 1; } }\n", "program"),
        (Language::Kotlin, "// c\nfun a(): Int = 1\n", "source_file"),
        (Language::Markdown, "# A\n\n- b\n\n```rust\nc\n```\n", "document"),
        (Language::Bash, "# c\nf() { echo \"$1\" | wc -l; }\n", "program"),
        (Language::Zig, "// c\nfn a() u8 {\n    return 1;\n}\n", "source_file"),
    ];

    fn root_kind(language: Language, source: &str) -> (String, bool) {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language.grammar()).expect("the grammar loads");
        let tree = parser.parse(source, None).expect("the parse runs");
        let root = tree.root_node();
        (root.kind().to_owned(), root.has_error())
    }

    /// The text and capture name of each capture `language`'s own query makes in `source`,
    /// in document order.
    fn captures(language: Language, source: &str) -> Vec<(String, String)> {
        use tree_sitter::StreamingIterator as _;
        let query_text = language.highlight_query().expect("triad gives a query");
        let grammar = language.grammar();
        let query = tree_sitter::Query::new(&grammar, &query_text).expect("the query compiles");
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&grammar).expect("the grammar loads");
        let tree = parser.parse(source, None).expect("the parse runs");
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut found = Vec::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        while let Some(m) = matches.next() {
            for capture in m.captures {
                let name = query.capture_names()[capture.index as usize].to_owned();
                found.push((source[capture.node.byte_range()].to_owned(), name));
            }
        }
        found
    }

    #[test]
    fn each_given_query_compiles_and_colours_its_sample() {
        let first_line = |source: &'static str| source.lines().next().expect("a first line");
        for (language, source) in [
            (Language::CSharp, include_str!("../../../fixtures/syntax/sample.cs")),
            (Language::Swift, include_str!("../../../fixtures/syntax/sample.swift")),
            (Language::Kotlin, include_str!("../../../fixtures/syntax/sample.kt")),
            (Language::Cpp, include_str!("../../../fixtures/syntax/sample.cpp")),
        ] {
            let found = captures(language, source);
            let comment = (first_line(source).to_owned(), "comment".to_owned());
            assert!(found.contains(&comment), "{language:?}: the opening comment is a comment");
        }
    }

    #[test]
    fn the_cpp_query_holds_c_s_lines() {
        let found = captures(Language::Cpp, "#include <string>\n");
        assert!(
            found.contains(&("#include".to_owned(), "keyword".to_owned())),
            "C's query colours the include: {found:?}"
        );
    }

    #[test]
    fn all_lists_each_language_once() {
        assert_eq!(SAMPLES.map(|(language, ..)| language), Language::ALL);
    }

    #[test]
    fn every_language_parses_its_own_sample_without_error() {
        for (language, source, root) in SAMPLES {
            assert_eq!(root_kind(language, source), (root.to_owned(), false), "{language:?}");
        }
    }

    #[test]
    fn typescript_and_tsx_are_different_grammars() {
        let jsx = "const a = <div>{b as number}</div>;\n";
        assert!(root_kind(Language::TypeScript, jsx).1, "TypeScript has no JSX");
        assert!(!root_kind(Language::Tsx, jsx).1);
        let cast = "const a = <number>b;\n";
        assert!(!root_kind(Language::TypeScript, cast).1);
        assert!(root_kind(Language::Tsx, cast).1, "TSX has no angle-bracket casts");
    }
}
