//! The language a pane's text is highlighted and merged as, from file names
//! (`docs/PLAN.md` §F5), and its grammar.
//!
//! git and jj hand a merge tool temporary inputs such as `BASE_1234.rs`, and jj's diff tool
//! files in a temporary directory, so the caller lists every name it knows, best first: the
//! inputs, then the output, then the `--title` path. The first name whose extension triad
//! knows gives the language. None is plain text.

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
}

/// Each language's file extensions, in lowercase.
const EXTENSIONS: [(&str, Language); 16] = [
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
];

impl Language {
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
        }
        .into()
    }

    /// The language of a file named `path`, by its extension in any case.
    #[must_use]
    pub fn of(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        EXTENSIONS
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(extension))
            .map(|&(_, language)| language)
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
            "a.json",
            "a.js.map",
            "a.tsbuildinfo",
            "a.jsonc",
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

    /// Each language's sample, and the kind of its tree's root.
    const SAMPLES: [(Language, &str, &str); 9] = [
        (Language::Rust, "fn a() -> u8 { 1 }\n", "source_file"),
        (Language::Go, "package a\nfunc b() {}\n", "source_file"),
        (Language::CSharp, "class A { void B() {} }\n", "compilation_unit"),
        (Language::Python, "def a():\n    return 1\n", "module"),
        (Language::Swift, "func a() -> Int { 1 }\n", "source_file"),
        (Language::Html, "<p>a</p>\n", "document"),
        (Language::JavaScript, "const a = () => 1;\n", "program"),
        (Language::TypeScript, "const a: number = 1;\n", "program"),
        (Language::Tsx, "const a = <div>{b as number}</div>;\n", "program"),
    ];

    fn root_kind(language: Language, source: &str) -> (String, bool) {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language.grammar()).expect("the grammar loads");
        let tree = parser.parse(source, None).expect("the parse runs");
        let root = tree.root_node();
        (root.kind().to_owned(), root.has_error())
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
