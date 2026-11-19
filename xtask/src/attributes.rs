//! Lint-suppression facts read from the syntax tree, so attribute-like text in
//! strings and comments is never mistaken for a real attribute.

use syn::visit::{self, Visit};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Facts {
    /// Lines of inner (`#![…]`) `allow`/`expect` attributes.
    pub inner_suppressions: Vec<usize>,
    /// Lines of `#[ignore]` without a reason.
    pub bare_ignores: Vec<usize>,
    /// Lines of attributes that suppress `unsafe_code`.
    pub unsafe_suppressions: Vec<usize>,
    /// Number of outer `#[expect(…)]` attributes.
    pub expects: usize,
}

pub fn collect(file: &syn::File) -> Facts {
    let mut facts = Facts::default();
    facts.visit_file(file);
    facts
}

impl<'ast> Visit<'ast> for Facts {
    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        let line = attr.pound_token.span.start().line;
        let suppresses = attr.path().is_ident("allow") || attr.path().is_ident("expect");
        if suppresses && matches!(attr.style, syn::AttrStyle::Inner(_)) {
            self.inner_suppressions.push(line);
        }
        if attr.path().is_ident("expect") && matches!(attr.style, syn::AttrStyle::Outer) {
            self.expects += 1;
        }
        if suppresses && mentions_unsafe_code(attr) {
            self.unsafe_suppressions.push(line);
        }
        if attr.path().is_ident("ignore") && matches!(attr.meta, syn::Meta::Path(_)) {
            self.bare_ignores.push(line);
        }
        visit::visit_attribute(self, attr);
    }
}

fn mentions_unsafe_code(attr: &syn::Attribute) -> bool {
    let syn::Meta::List(list) = &attr.meta else {
        return false;
    };
    list.tokens
        .to_string()
        .split([',', ' '])
        .any(|token| token == "unsafe_code")
}

#[cfg(test)]
mod tests {
    use super::{Facts, collect};

    fn facts(source: &str) -> Facts {
        collect(&syn::parse_file(source).unwrap())
    }

    #[test]
    fn finds_suppressions_and_bare_ignores() {
        let found = facts(
            "#![allow(dead_code)]\n\
             #[expect(unsafe_code, reason = \"ffi\")]\nfn a() {}\n\
             #[ignore]\nfn b() {}\n\
             #[ignore = \"slow\"]\nfn c() {}\n",
        );
        assert_eq!(
            found,
            Facts {
                inner_suppressions: vec![1],
                bare_ignores: vec![4],
                unsafe_suppressions: vec![2],
                expects: 1,
            }
        );
    }

    #[test]
    fn ignores_attribute_text_in_strings() {
        assert_eq!(
            facts("fn f() { let _ = \"#![allow(x)] #[expect(y)]\"; }"),
            Facts::default()
        );
    }
}
