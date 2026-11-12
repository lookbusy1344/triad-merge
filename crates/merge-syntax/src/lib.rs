//! Languages and their tree-sitter grammars: a text's syntax colours, and the syntax
//! boundaries a merge keeps its cuts out of (`docs/DESIGN.md` §B5.8, §F). No UI.

pub mod boundaries;
pub mod language;
pub mod syntax;
