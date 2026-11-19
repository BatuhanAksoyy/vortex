//! Project file model: gitignore-aware listing, tree building, language
//! detection and fuzzy path search. No UI dependencies.

mod files;
mod fuzzy;
mod language;
mod tree;

pub use files::{FileList, ListOptions, list_files};
pub use fuzzy::{PathMatch, fuzzy_match};
pub use language::language_for_path;
pub use tree::{TreeNode, build_tree};
