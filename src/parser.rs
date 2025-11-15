use tree_sitter::Parser;
use tree_sitter_bpftrace;

use crate::log_mod::{self, PROTO};
use crate::log_vdbg;

pub fn ts_parse(source_code: &str) {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_bpftrace::LANGUAGE.into())
        .expect("Error loading bptfrace grammar"); // TODO

    let tree = parser.parse(source_code.as_bytes(), None).unwrap();
    let root_node = tree.root_node();
    assert_eq!(root_node.kind(), "source_file");

    log_vdbg!(PROTO, "Syntax tree\n {}", root_node.to_sexp());
}
