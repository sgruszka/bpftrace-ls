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

fn postion_before_next_sibling(node: &Node, line_nr: usize, char_nr: usize) -> bool {
    if let Some(next_sibling) = node.next_sibling() {
        if postition_relative_to_node(&next_sibling, line_nr, char_nr) == Position::BEFORE {
            return true;
        }
    }

    false
}

fn node_to_syntax_location(node: &Node) -> SyntaxLocation {
    match node.kind() {
        "block_comment" => SyntaxLocation::Comment,
        "line_comment" => SyntaxLocation::Comment,
        "probes" => SyntaxLocation::Probes,
        "predicate" => SyntaxLocation::Predicate,
        "action" => SyntaxLocation::Action,
        _ => SyntaxLocation::SourceFile,
    }
}

pub fn find_syntax_location<'t>(
    text: &str,
    tree: &'t Tree,
    line_nr: usize,
    char_nr: usize,
) -> (SyntaxLocation, Node<'t>) {
    let query_str = r#"
    [
        (probes) @probes
        (predicate) @predicate
        (action) @action
        (block_comment) @block_comment
        (line_comment) @line_comment
    ]
    "#;

    let query = Query::new(&tree_sitter_bpftrace::LANGUAGE.into(), query_str)
        .expect("Error creating query"); // TODO

    let mut query_cursor = QueryCursor::new();
    let mut matches = query_cursor.matches(&query, tree.root_node(), text.as_bytes());

    let mut ret = (SyntaxLocation::SourceFile, tree.root_node());
    let mut current_node: Option<Node> = None;

    while let Some(m) = matches.next() {
        for cap in m.captures {
            let node = cap.node;

            let pos = postition_relative_to_node(&node, line_nr, char_nr);

            if pos == Position::WITHIN {
                ret = (node_to_syntax_location(&node), node);
            } else if pos == Position::BEFORE {
                break;
            }

            current_node = Some(node);
        }
    }

    if ret.0 != SyntaxLocation::SourceFile || current_node.is_none() {
        return ret;
    }

    let node = current_node.unwrap();
    if (node.next_sibling().is_none() || postion_before_next_sibling(&node, line_nr, char_nr))
        && node.has_error()
    {
        if let Some(right_child) = node.child(node.child_count() - 1) {
            if right_child.is_missing() {
                return (node_to_syntax_location(&node), node);
            }
        }
    }

    (SyntaxLocation::SourceFile, tree.root_node())
}

pub fn find_probe_for_action(action: &Node, text: &str) -> String {
    assert_eq!(action.kind(), "action");

    // TODO remove unwrap and add checks in case of broken tree
    let action_block = action.parent().unwrap();
    assert_eq!(action_block.kind(), "action_block");

    let probes = action_block.child(0).unwrap();
    assert_eq!(probes.kind(), "probes");

    // TODO handle probe list and wildcard's
    let probe = probes.child(0).unwrap();

    let probe_text = probe.utf8_text(text.as_bytes());

    //"".to_string()
    probe_text.unwrap().to_string()
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

// TODO we should not count for braces in comments :-)
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
            break;
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
        assert_eq!(ret.0, SyntaxLocation::Comment);
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
        assert_eq!(ret.0, SyntaxLocation::Probes);
        let ret = find_syntax_location(text, &tree, 2, 0);
        assert_eq!(ret.0, SyntaxLocation::Probes);

        let ret = find_syntax_location(text, &tree, 3, 0);
        assert_eq!(ret.0, SyntaxLocation::Action);
    }

    #[test]
    fn test_unfinished_action_syntax_find() {
        let text = "kprobe:tcp_reset {  ";
        let tree = setup_syntax_tree(text);

        let ret = find_syntax_location(text, &tree, 0, text.len() - 1);
        assert_eq!(ret.0, SyntaxLocation::Action);
    }

    #[test]
    fn test_block_comment_in_action_syntax_find() {
        // TODO:
        // let text = r#"kfunc:vmlinux:posix_timer_fn { printf("%d\n", args.timer->"#;

        let text = r#"kfunc:vmlinux:posix_timer_fn { printf("%d\n", /* args.timer->*/}"#;
        let tree = setup_syntax_tree(text);

        let ret = find_syntax_location(text, &tree, 0, text.len() - 5);
        assert_eq!(ret.0, SyntaxLocation::Comment);
    }

    #[test]
    fn test_line_comment_in_action_syntax_find() {
        let text = "kfunc:vmlinux:posix_timer_fn {\n// Line comment\n}";
        let tree = setup_syntax_tree(text);

        let ret = find_syntax_location(text, &tree, 1, 0);
        assert_eq!(ret.0, SyntaxLocation::Comment);
    }

    #[test]
    fn test_oneline_probe_for_action() {
        let text = r#"kretfunc:mac80211:ieee80211_deauth { print(args) }"#;
        let tree = setup_syntax_tree(text);

        let (loc, action) = find_syntax_location(text, &tree, 0, text.len() - 2);
        assert_eq!(loc, SyntaxLocation::Action);
        assert_eq!(action.kind(), "action");

        let probe = find_probe_for_action(&action, text);
        assert_eq!(probe, "kretfunc:mac80211:ieee80211_deauth");
    }

    #[test]
    fn test_multiline_probe_for_action() {
        let text = r#"
kfunc:vmlinux:posix_timer_fn {
    printf("%d \n", args.timer->base->cpu_base->in_hrtirq);
    // Commented line
    printf("%p\n", args.timer->is_hard);
}
        "#;
        let tree = setup_syntax_tree(text);

        let (loc, action) = find_syntax_location(text, &tree, 3, 0);
        assert_eq!(loc, SyntaxLocation::Action);
        assert_eq!(action.kind(), "action");

        let probe = find_probe_for_action(&action, text);
        assert_eq!(probe, "kfunc:vmlinux:posix_timer_fn");
    }
}
