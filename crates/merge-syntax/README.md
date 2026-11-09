# merge-syntax

Languages, grammars and syntax. `language::Language` comes from file names; `syntax` parses a text in chunks into highlight spans per line; `boundaries::SyntaxBoundaries` implements `merge_core::boundaries::Boundaries`, so syntax-aware merging works without the view model. No UI.
