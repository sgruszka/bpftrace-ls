use tree_sitter::{Node, Point, Query, QueryCursor, StreamingIterator, Tree};

use crate::log_dbg;
use crate::log_mod::{self, PARSE};

// Syntax tree nodes we are interested in in context of completion
#[derive(Debug, PartialEq)]
pub enum SyntaxLocation {
    SourceFile,
    Comment,
    Probes,
    Predicate,
    Action,
    ArgsItem,
}

#[derive(PartialEq)]
enum Position {
    BEFORE,
    WITHIN,
    AFTER,
}

fn postition_relative_to_node(node: &Node, line_nr: usize, char_nr: usize) -> Position {
    let start = node.start_position();
    let end = node.end_position();

    if line_nr < start.row {
        return Position::BEFORE;
    }
    if line_nr > end.row {
        return Position::AFTER;
    }
    if line_nr == start.row && char_nr < start.column {
        return Position::BEFORE;
    }
    if line_nr == end.row && char_nr >= end.column {
        return Position::AFTER;
    }

    Position::WITHIN
}

pub fn find_syntax_location(
    text: &str,
    tree: &Tree,
    line_nr: usize,
    char_nr: usize,
) -> SyntaxLocation {
    let query_str = r#"
    [
        (probes) @probes
        (predicate) @predicate
        (action) @action
        (block_comment) @block_comment
        (line_comment) @line_comment
        (args_item) @args_item
    ]
    "#;

    let query = Query::new(&tree_sitter_bpftrace::LANGUAGE.into(), query_str)
        .expect("Error creating query"); // TODO

    let mut query_cursor = QueryCursor::new();
    let mut matches = query_cursor.matches(&query, tree.root_node(), text.as_bytes());

    let mut ret = SyntaxLocation::SourceFile;

    while let Some(m) = matches.next() {
        for cap in m.captures {
            let node = cap.node;
            println!("{}", node.kind());

            let start_point = node.start_position();
            let end_point = node.end_position();
            println!(
                "    Start: Line {}, Column {}",
                start_point.row + 1,
                start_point.column + 1
            );
            println!(
                "    End:   Line {}, Column {}",
                end_point.row + 1,
                end_point.column + 1
            );

            let pos = postition_relative_to_node(&node, line_nr, char_nr);

            if pos == Position::WITHIN {
                ret = match node.kind() {
                    "block_comment" => SyntaxLocation::Comment,
                    "line_comment" => SyntaxLocation::Comment,
                    "probes" => SyntaxLocation::Probes,
                    "predicate" => SyntaxLocation::Predicate,
                    "action" => SyntaxLocation::Action,
                    "args_item" => SyntaxLocation::ArgsItem,
                    _ => SyntaxLocation::SourceFile,
                };
                break;
            }
        }
    }

    ret
}

pub fn find_location(tree: &Tree, line_nr: usize, char_nr: usize) -> SyntaxLocation {
    let root_node = tree.root_node();
    log_dbg!(PARSE, "Syntax tree\n {}", root_node.to_sexp());

    let pos = Point::new(line_nr, char_nr);
    let mut node = if let Some(n) = root_node.descendant_for_point_range(pos, pos) {
        n
    } else {
        return SyntaxLocation::SourceFile;
    };

    // TODO! This might not work correctly when there are errors in syntax tree

    loop {
        let loc = match node.kind() {
            "source_file" => SyntaxLocation::SourceFile,
            "block_comment" => SyntaxLocation::Comment,
            "line_comment" => SyntaxLocation::Comment,
            "probes" => SyntaxLocation::Probes,
            "predicate" => SyntaxLocation::Predicate,
            "action" => SyntaxLocation::Action,
            "args_item" => SyntaxLocation::ArgsItem,
            _ => SyntaxLocation::SourceFile,
        };

        if loc != SyntaxLocation::SourceFile {
            return loc;
        }

        node = if let Some(n) = node.parent() {
            n
        } else {
            break;
        };
    }

    SyntaxLocation::SourceFile
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

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::{Parser, Tree};

    fn setup_syntax_tree(source_code: &str) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_bpftrace::LANGUAGE.into())
            .expect("Error loading bpftrace grammar");
        let tree = parser.parse(source_code, None).unwrap();
        tree
    }

    #[test]
    fn test_tree_sitter() {
        let tree = setup_syntax_tree("kprobe:tcp_reset { }");

        let root_node = tree.root_node();
        assert_eq!(root_node.kind(), "source_file");

        let action_block = root_node.child(0).unwrap();
        assert_eq!(action_block.kind(), "action_block");

        let probes = action_block.child(0).unwrap();
        let action = action_block.child(1).unwrap();
        assert_eq!(probes.kind(), "probes");
        assert_eq!(action.kind(), "action");

        let probe = probes.child(0).unwrap();
        assert_eq!(probe.kind(), "probe");
        assert_eq!(probe.child_count(), 3);
        assert_eq!(probe.field_name_for_child(0).unwrap(), "provider");
        assert_eq!(probe.field_name_for_child(2).unwrap(), "event");
    }

    #[test]
    fn test_find_location() {
        let tree = setup_syntax_tree("kprobe:tcp_reset { }\n /* this is block comment */\n");

        let ret = find_location(&tree, 0, 18);
        assert_eq!(ret, SyntaxLocation::Action);

        let ret = find_location(&tree, 0, 0);
        assert_eq!(ret, SyntaxLocation::Probes);

        let ret = find_location(&tree, 1, 5);
        assert_eq!(ret, SyntaxLocation::Comment);
    }

    #[test]
    fn test_block_comment_syntax_find() {
        let text = "kprobe:tcp_reset { }\n /* this is block comment */\n";
        let tree = setup_syntax_tree(text);

        let ret = find_syntax_location(text, &tree, 1, 5);
        assert_eq!(ret, SyntaxLocation::Comment);
    }

    #[test]
    fn test_action_block_syntax_find() {
        let text = r#"
tracepoint:syscalls:sys_enter_open,
tracepoint:syscalls:sys_enter_openat {
  printf("%-6d %-16s %s\n", pid, comm, str(args.filename));
}
        "#;
        let tree = setup_syntax_tree(text);

        let ret = find_syntax_location(text, &tree, 1, 0);
        assert_eq!(ret, SyntaxLocation::Probes);
        let ret = find_syntax_location(text, &tree, 2, 0);
        assert_eq!(ret, SyntaxLocation::Probes);

        let ret = find_syntax_location(text, &tree, 3, 0);
        assert_eq!(ret, SyntaxLocation::Action);

        let ret = find_syntax_location(text, &tree, 3, 55);
        assert_eq!(ret, SyntaxLocation::ArgsItem);
    }
}
