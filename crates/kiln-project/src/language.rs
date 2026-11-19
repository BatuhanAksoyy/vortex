use std::path::Path;

/// Returns the syntax-highlighting language name understood by gpui-kit's
/// highlighter, or `None` for plain text.
#[must_use]
pub fn language_for_path(path: &Path) -> Option<&'static str> {
    let file_name = path.file_name()?.to_str()?;
    lookup(BY_FILE_NAME, file_name).or_else(|| {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        lookup(BY_EXTENSION, &extension)
    })
}

const BY_FILE_NAME: &[(&[&str], &str)] = &[
    (&["Makefile", "makefile", "GNUmakefile"], "make"),
    (&["CMakeLists.txt"], "cmake"),
    (&["Cargo.lock", "uv.lock", "poetry.lock"], "toml"),
    (&[".bashrc", ".zshrc", ".profile", ".envrc"], "bash"),
];

const BY_EXTENSION: &[(&[&str], &str)] = &[
    (&["rs"], "rust"),
    (&["ts", "mts", "cts"], "typescript"),
    (&["tsx"], "tsx"),
    (&["js", "mjs", "cjs", "jsx"], "javascript"),
    (&["json", "jsonc", "json5"], "json"),
    (&["py", "pyi"], "python"),
    (&["go"], "go"),
    (&["c", "h"], "c"),
    (&["cc", "cpp", "cxx", "hpp", "hh", "hxx"], "cpp"),
    (&["cs"], "csharp"),
    (&["java"], "java"),
    (&["kt", "kts"], "kotlin"),
    (&["swift"], "swift"),
    (&["rb"], "ruby"),
    (&["php"], "php"),
    (&["lua"], "lua"),
    (&["zig"], "zig"),
    (&["scala"], "scala"),
    (&["ex", "exs"], "elixir"),
    (&["sh", "bash", "zsh"], "bash"),
    (&["toml"], "toml"),
    (&["yaml", "yml"], "yaml"),
    (&["md", "mdx", "markdown"], "markdown"),
    (&["html", "htm"], "html"),
    (&["css", "scss"], "css"),
    (&["sql"], "sql"),
    (&["graphql", "gql"], "graphql"),
    (&["proto"], "proto"),
    (&["svelte"], "svelte"),
    (&["astro"], "astro"),
    (&["diff", "patch"], "diff"),
    (&["cmake"], "cmake"),
    (&["mk"], "make"),
    (&["ejs"], "ejs"),
    (&["erb"], "erb"),
];

fn lookup(table: &[(&[&str], &'static str)], key: &str) -> Option<&'static str> {
    table
        .iter()
        .find(|(keys, _)| keys.contains(&key))
        .map(|(_, language)| *language)
}

#[cfg(test)]
mod tests {
    use super::language_for_path;
    use std::path::Path;

    #[test]
    fn detects_by_extension_case_insensitively() {
        assert_eq!(language_for_path(Path::new("src/main.rs")), Some("rust"));
        assert_eq!(language_for_path(Path::new("App.TSX")), Some("tsx"));
        assert_eq!(language_for_path(Path::new("a/b.yml")), Some("yaml"));
    }

    #[test]
    fn detects_by_file_name_before_extension() {
        assert_eq!(language_for_path(Path::new("Makefile")), Some("make"));
        assert_eq!(
            language_for_path(Path::new("x/CMakeLists.txt")),
            Some("cmake")
        );
    }

    #[test]
    fn unknown_is_plain_text() {
        assert_eq!(language_for_path(Path::new("LICENSE")), None);
        assert_eq!(language_for_path(Path::new("data.bin")), None);
    }
}
