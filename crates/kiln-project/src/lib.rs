//! Project file model: gitignore-aware listing, tree building, language
//! detection, fuzzy path search and changing files. No UI dependencies.

mod files;
mod fuzzy;
mod language;
mod operations;
mod tree;

pub use files::{FileList, ListOptions, list_files};
pub use fuzzy::{PathMatch, fuzzy_match};
pub use language::language_for_path;
pub use operations::{create_file, create_folder, rename, trash};
pub use tree::{TreeNode, build_tree};
