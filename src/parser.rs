use tree_sitter::Tree;

use crate::log_dbg;
use crate::log_mod::{self, PROTO};

pub fn ts_parse(tree: &Tree) {
    let root_node = tree.root_node();
    assert_eq!(root_node.kind(), "source_file");

    log_dbg!(PROTO, "Syntax tree\n {}", root_node.to_sexp());
}

pub fn is_action_block(text: &str, line_nr: usize, char_nr: usize) -> bool {
    let mut brace_count = 0;
    for (i, line) in text.lines().enumerate() {
        if i == line_nr {
            let last_line = line.to_string();
            for (i, c) in last_line.chars().enumerate() {
                if i >= char_nr {
                    break;
                }
                if c == '{' {
                    brace_count += 1;
                }
                if c == '}' {
                    brace_count -= 1;
                }
            }
        } else {
            brace_count += line.matches("{").count();
            brace_count -= line.matches("}").count();
        }
    }

    brace_count > 0
}

/*
fn is_args(line_str: &str, char_nr: usize) -> bool {
    let mut res = false;
    if let Some(line_upto_char) = line_str.get(0..char_nr) {
        res = line_upto_char.ends_with("args.");
    }
    res
}
*/

pub fn is_argument(line_str: &str, char_nr: usize, args: &mut String) -> bool {
    let mut res = false;
    if let Some(line_upto_char) = line_str.get(0..char_nr) {
        if let Some(last_word) = line_upto_char
            .rsplit(|c| c == ' ' || c == '{' || c == '(')
            .nth(0)
        {
            if last_word.starts_with("args.") {
                args.push_str(last_word);
                res = true;
            }
        }
    }

    res
}
