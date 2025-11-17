use tree_sitter::Tree;

use crate::log_dbg;
use crate::log_mod::{self, PROTO};

pub fn ts_parse(tree: &Tree) {
    let root_node = tree.root_node();
    assert_eq!(root_node.kind(), "source_file");

    log_dbg!(PROTO, "Syntax tree\n {}", root_node.to_sexp());
}
