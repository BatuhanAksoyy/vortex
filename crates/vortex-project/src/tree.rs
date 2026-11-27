use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::files::FileList;

/// A file tree node. Children are sorted directories first, then by name,
/// case-insensitively.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub children: Vec<TreeNode>,
}

#[derive(Default)]
struct Builder {
    is_dir: bool,
    children: BTreeMap<String, Builder>,
}

#[must_use]
pub fn build_tree(list: &FileList) -> Vec<TreeNode> {
    let mut root = Builder::default();
    for entry in &list.entries {
        let mut node = &mut root;
        let components: Vec<String> = entry
            .path
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let last_index = components.len().saturating_sub(1);
        for (index, name) in components.into_iter().enumerate() {
            node = node.children.entry(name).or_default();
            node.is_dir |= index < last_index || entry.is_dir;
        }
    }
    finish(root.children, Path::new(""))
}

fn finish(children: BTreeMap<String, Builder>, parent: &Path) -> Vec<TreeNode> {
    let mut nodes: Vec<TreeNode> = children
        .into_iter()
        .map(|(name, builder)| {
            let path = parent.join(&name);
            TreeNode {
                children: finish(builder.children, &path),
                is_dir: builder.is_dir,
                name,
                path,
            }
        })
        .collect();
    nodes.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    nodes
}

#[cfg(test)]
mod tests {
    use super::build_tree;
    use crate::files::{FileEntry, FileList};
    use std::path::PathBuf;

    fn list(entries: &[(&str, bool)]) -> FileList {
        FileList {
            entries: entries
                .iter()
                .map(|(path, is_dir)| FileEntry {
                    path: PathBuf::from(path),
                    is_dir: *is_dir,
                })
                .collect(),
            truncated: false,
        }
    }

    #[test]
    fn nests_and_sorts_directories_first() {
        let tree = build_tree(&list(&[
            ("README.md", false),
            ("src", true),
            ("src/main.rs", false),
            ("src/app", true),
            ("src/app/view.rs", false),
            ("Cargo.toml", false),
            ("assets", true),
        ]));
        let names: Vec<&str> = tree.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["assets", "src", "Cargo.toml", "README.md"]);
        let src = &tree[1];
        assert!(src.is_dir);
        let src_children: Vec<&str> = src.children.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(src_children, ["app", "main.rs"]);
        assert_eq!(
            src.children[0].children[0].path,
            PathBuf::from("src/app/view.rs")
        );
    }

    #[test]
    fn infers_parent_directories_missing_from_list() {
        let tree = build_tree(&list(&[("a/b/c.txt", false)]));
        assert!(tree[0].is_dir);
        assert!(tree[0].children[0].is_dir);
        assert!(!tree[0].children[0].children[0].is_dir);
    }
}
