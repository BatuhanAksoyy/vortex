//! `McCabe` cyclomatic complexity per function, computed from the syntax tree so
//! the result is deterministic and independent of external tools.
//!
//! Each function starts at 1 and gains 1 for every `if` (including `if let`
//! and `else if`), `while`, `for`, `loop`, `let … else`, match guard, `&&`,
//! `||`, and every `match` arm after the first. Closures count toward their
//! enclosing function, so branching cannot be hidden inside callbacks.

use syn::visit::{self, Visit};

#[derive(Debug, PartialEq, Eq)]
pub struct FunctionComplexity {
    pub name: String,
    pub line: usize,
    pub complexity: usize,
}

pub fn analyze(file: &syn::File) -> Vec<FunctionComplexity> {
    let mut collector = Collector::default();
    collector.visit_file(file);
    collector.functions
}

#[derive(Default)]
struct Collector {
    functions: Vec<FunctionComplexity>,
}

impl Collector {
    fn record(&mut self, ident: &syn::Ident, block: &syn::Block) {
        let mut counter = Counter { count: 1 };
        counter.visit_block(block);
        self.functions.push(FunctionComplexity {
            name: ident.to_string(),
            line: ident.span().start().line,
            complexity: counter.count,
        });
    }
}

impl<'ast> Visit<'ast> for Collector {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.record(&node.sig.ident, &node.block);
        visit::visit_item_fn(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.record(&node.sig.ident, &node.block);
        visit::visit_impl_item_fn(self, node);
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        if let Some(block) = &node.default {
            self.record(&node.sig.ident, block);
        }
        visit::visit_trait_item_fn(self, node);
    }
}

struct Counter {
    count: usize,
}

impl<'ast> Visit<'ast> for Counter {
    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        self.count += 1;
        visit::visit_expr_if(self, node);
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        self.count += 1;
        visit::visit_expr_while(self, node);
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        self.count += 1;
        visit::visit_expr_for_loop(self, node);
    }

    fn visit_expr_loop(&mut self, node: &'ast syn::ExprLoop) {
        self.count += 1;
        visit::visit_expr_loop(self, node);
    }

    fn visit_expr_match(&mut self, node: &'ast syn::ExprMatch) {
        self.count += node.arms.len().saturating_sub(1);
        self.count += node.arms.iter().filter(|arm| arm.guard.is_some()).count();
        visit::visit_expr_match(self, node);
    }

    fn visit_local(&mut self, node: &'ast syn::Local) {
        if node
            .init
            .as_ref()
            .is_some_and(|init| init.diverge.is_some())
        {
            self.count += 1;
        }
        visit::visit_local(self, node);
    }

    fn visit_bin_op(&mut self, node: &'ast syn::BinOp) {
        if matches!(node, syn::BinOp::And(_) | syn::BinOp::Or(_)) {
            self.count += 1;
        }
        visit::visit_bin_op(self, node);
    }

    // Nested items are separate functions, analyzed by `Collector`.
    fn visit_item(&mut self, _: &'ast syn::Item) {}
}

#[cfg(test)]
mod tests {
    use super::FunctionComplexity;

    fn analyze(source: &str) -> Vec<FunctionComplexity> {
        super::analyze(&syn::parse_file(source).unwrap())
    }

    fn complexity_of(source: &str) -> usize {
        analyze(source)[0].complexity
    }

    #[test]
    fn straight_line_code_is_one() {
        assert_eq!(complexity_of("fn f() { let x = 1; g(x); }"), 1);
    }

    #[test]
    fn counts_branches_loops_and_boolean_operators() {
        let source = "fn f(a: bool, b: bool) {
            if a && b { } else if a || b { }
            while a { } for _ in 0..3 { } loop { break; }
        }";
        assert_eq!(complexity_of(source), 8);
    }

    #[test]
    fn counts_match_arms_guards_and_let_else() {
        let source = "fn f(x: Option<u8>) {
            match x { Some(v) if v > 1 => {}, Some(_) => {}, None => {} }
            let Some(y) = x else { return; };
        }";
        assert_eq!(complexity_of(source), 5);
    }

    #[test]
    fn closures_count_toward_the_enclosing_function() {
        assert_eq!(
            complexity_of("fn f() { let c = |a: bool| if a { 1 } else { 2 }; }"),
            2
        );
    }

    #[test]
    fn methods_and_nested_functions_are_separate() {
        let source = "impl S { fn m(&self) { if true {} fn inner() { if true {} if true {} } } }";
        let functions = analyze(source);
        let names: Vec<(&str, usize)> = functions
            .iter()
            .map(|f| (f.name.as_str(), f.complexity))
            .collect();
        assert_eq!(names, [("m", 2), ("inner", 3)]);
    }
}
