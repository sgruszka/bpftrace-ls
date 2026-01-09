use tree_sitter::{Node, Point, Query, QueryCursor, StreamingIterator, Tree};

use crate::log_dbg;
use crate::log_mod::{self, PARSE};

// Syntax tree nodes we are interested in in context of completion
#[derive(Debug, PartialEq)]
pub enum SyntaxLocation {
    SourceFile,
    Comment,
    ProbesList,
    Predicate,
    Action,
}

#[derive(PartialEq)]
enum Position {
    Before,
    Within,
    After,
}

fn postition_relative_to_node(node: &Node, line_nr: usize, char_nr: usize) -> Position {
    let start = node.start_position();
    let end = node.end_position();

    if line_nr < start.row {
        return Position::Before;
    }
    if line_nr > end.row {
        return Position::After;
    }
    if line_nr == start.row && char_nr < start.column {
        return Position::Before;
    }
    if line_nr == end.row && char_nr >= end.column {
        return Position::After;
    }

    Position::Within
}

fn postion_before_next_sibling(node: &Node, line_nr: usize, char_nr: usize) -> bool {
    if let Some(next_sibling) = node.next_sibling() {
        if postition_relative_to_node(&next_sibling, line_nr, char_nr) == Position::Before {
            return true;
        }
    }

    false
}

fn node_to_syntax_location(node: &Node) -> SyntaxLocation {
    match node.kind() {
        "block_comment" => SyntaxLocation::Comment,
        "line_comment" => SyntaxLocation::Comment,
        "probes_list" => SyntaxLocation::ProbesList,
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
        (probes_list) @probe_list
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

    'matches_loop: while let Some(m) = matches.next() {
        for cap in m.captures {
            let node = cap.node;

            let pos = postition_relative_to_node(&node, line_nr, char_nr);

            if pos == Position::Within {
                ret = (node_to_syntax_location(&node), node);
            } else if pos == Position::Before {
                break 'matches_loop;
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

pub fn find_error_location<'t>(
    text: &str,
    root_node: &Node<'t>,
    line_nr: usize,
    char_nr: usize,
) -> Option<Node<'t>> {
    let query_str = r#"
    [
        (ERROR) @ERROR
    ]
    "#;

    let query = Query::new(&tree_sitter_bpftrace::LANGUAGE.into(), query_str)
        .expect("Error creating query"); // TODO

    let mut query_cursor = QueryCursor::new();
    let mut matches = query_cursor.matches(&query, *root_node, text.as_bytes());

    'matches_loop: while let Some(m) = matches.next() {
        for cap in m.captures {
            let node = cap.node;

            let pos = postition_relative_to_node(&node, line_nr, char_nr);

            if pos == Position::Within {
                return Some(node);
            } else if pos == Position::Before {
                break 'matches_loop;
            }
        }
    }

    None
}

pub fn find_errors<'t>(text: &str, root_node: &Node<'t>) -> Vec<Node<'t>> {
    let query_str = r#"
    [
        (ERROR) @ERROR
        (MISSING) @MISSING
    ]
    "#;

    let query = Query::new(&tree_sitter_bpftrace::LANGUAGE.into(), query_str)
        .expect("Error creating query"); // TODO

    let mut query_cursor = QueryCursor::new();
    let mut matches = query_cursor.matches(&query, *root_node, text.as_bytes());

    let mut results: Vec<Node> = vec![];

    let mut last_end: isize = -1;
    while let Some(m) = matches.next() {
        for cap in m.captures {
            let node = cap.node;

            let end = node.end_position();
            let line_nr = end.row as isize;

            if line_nr > last_end {
                results.push(node);
                last_end = line_nr;
            }
        }
    }

    results
}

pub fn find_probe_for_action(action: &Node, text: &str) -> String {
    assert_eq!(action.kind(), "action");

    // TODO remove unwrap and add checks in case of broken tree
    let action_block = action.parent().unwrap();
    assert_eq!(action_block.kind(), "action_block");

    let probes_list = action_block.child(0).unwrap();
    assert_eq!(probes_list.kind(), "probes_list");

    // TODO handle probe list and wildcard's
    let probe = probes_list.child(0).unwrap();

    let probe_text = probe.utf8_text(text.as_bytes());

    //"".to_string()
    probe_text.unwrap().to_string()
}

pub fn find_probe_for_error(error_node: &Node, text: &str) -> String {
    assert_eq!(error_node.kind(), "ERROR");
    let mut probe_str = "".to_string();

    let mut cursor = error_node.walk();
    for child_node in error_node.children(&mut cursor) {
        let probe;
        if child_node.kind() == "probes_list" {
            probe = child_node.child(0).unwrap();
        } else if child_node.kind() == "probe" {
            probe = child_node;
        } else {
            continue;
        }

        let probe_text = probe.utf8_text(text.as_bytes());
        probe_str = probe_text.unwrap().to_string();
    }

    probe_str
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
            "probes_list" => SyntaxLocation::ProbesList,
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

pub fn is_argument(line_str: &str, char_nr: usize) -> Option<String> {
    if let Some(last_word) = line_str
        .get(0..=char_nr)
        .and_then(|line_upto_char| line_upto_char.rsplit([' ', '{', '(', ',']).next())
    {
        if last_word.starts_with("args.") {
            return Some(last_word.to_string());
        }
    }

    None
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

        let probes_list = action_block.child(0).unwrap();
        let action = action_block.child(1).unwrap();
        assert_eq!(probes_list.kind(), "probes_list");
        assert_eq!(action.kind(), "action");

        let probe = probes_list.child(0).unwrap();
        assert_eq!(probe.kind(), "probe");
        assert_eq!(probe.child_count(), 3);
        assert_eq!(probe.field_name_for_child(0).unwrap(), "provider");
        assert_eq!(probe.field_name_for_child(2).unwrap(), "function");
    }

    #[test]
    fn test_find_location() {
        let tree = setup_syntax_tree("kprobe:tcp_reset { }\n /* this is block comment */\n");

        let ret = find_location(&tree, 0, 18);
        assert_eq!(ret, SyntaxLocation::Action);

        let ret = find_location(&tree, 0, 0);
        assert_eq!(ret, SyntaxLocation::ProbesList);

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
        assert_eq!(ret.0, SyntaxLocation::ProbesList);
        let ret = find_syntax_location(text, &tree, 2, 0);
        assert_eq!(ret.0, SyntaxLocation::ProbesList);

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
