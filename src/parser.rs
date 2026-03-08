use tree_sitter::{Node, Point, Query, QueryCursor, StreamingIterator, Tree};

use crate::log_mod::{self, PARSE};
use crate::{log_dbg, log_err};

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

    let mut ret = (SyntaxLocation::SourceFile, tree.root_node());

    let query = match Query::new(&tree_sitter_bpftrace::LANGUAGE.into(), query_str) {
        Ok(q) => q,
        Err(e) => {
            log_err!("Tree-sitter error: {}", e);
            return ret;
        }
    };

    let mut query_cursor = QueryCursor::new();
    let mut matches = query_cursor.matches(&query, tree.root_node(), text.as_bytes());

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

    let query = match Query::new(&tree_sitter_bpftrace::LANGUAGE.into(), query_str) {
        Ok(q) => q,
        Err(e) => {
            log_err!("Tree-sitter error: {}", e);
            return None;
        }
    };

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

    let query = match Query::new(&tree_sitter_bpftrace::LANGUAGE.into(), query_str) {
        Ok(q) => q,
        Err(e) => {
            log_err!("Tree-sitter error: {}", e);
            return Vec::new();
        }
    };

    let mut query_cursor = QueryCursor::new();
    let mut matches = query_cursor.matches(&query, *root_node, text.as_bytes());

    let mut results: Vec<Node> = vec![];

    while let Some(m) = matches.next() {
        for cap in m.captures {
            let node = cap.node;
            results.push(node);
        }
    }

    results
}

pub fn find_all_map_variables<'t>(text: &str, root_node: &Node<'t>) -> Vec<Node<'t>> {
    let query_str = r#"
        (assignment_statement
          left: (map_variable) @map.lhs)
    "#;

    let query = match Query::new(&tree_sitter_bpftrace::LANGUAGE.into(), query_str) {
        Ok(q) => q,
        Err(e) => {
            log_err!("Tree-sitter error: {}", e);
            return Vec::new();
        }
    };

    let mut query_cursor = QueryCursor::new();
    let mut matches = query_cursor.matches(&query, *root_node, text.as_bytes());

    let mut results: Vec<Node> = vec![];

    while let Some(m) = matches.next() {
        for cap in m.captures {
            let node = cap.node;
            results.push(node);
        }
    }

    results
}

fn probes_list_to_vec(probes_list: &Node, text: &str) -> Vec<String> {
    let mut probes_vec: Vec<String> = Vec::with_capacity(probes_list.child_count());
    for i in 0..probes_list.child_count() {
        let probe = probes_list.child(i).unwrap();
        if probe.kind() != "probe" {
            continue;
        }

        let probe_text = probe.utf8_text(text.as_bytes());

        probes_vec.push(probe_text.unwrap().to_string());
    }

    probes_vec
}

pub fn find_probes_for_action(action: &Node, text: &str) -> Vec<String> {
    assert_eq!(action.kind(), "action");

    let Some(action_block) = action.parent() else {
        return Vec::new();
    };
    // TODO: handle broken tree - remove assertions
    assert_eq!(action_block.kind(), "action_block");

    let probes_list = action_block.child(0).unwrap();
    assert_eq!(probes_list.kind(), "probes_list");

    probes_list_to_vec(&probes_list, text)
}

fn add_scratch_variables_for_node(
    node: &Node,
    text: &str,
    results: &mut Vec<String>,
    child_nr: usize,
) {
    if let Some(var) = node.child(child_nr).and_then(|var| {
        if var.kind() == "scratch_variable" {
            Some(var)
        } else {
            None
        }
    }) {
        if let Ok(variable_name) = var.utf8_text(text.as_bytes()) {
            results.push(variable_name.to_owned());
        }
    }
}

fn add_map_variables_for_node(node: &Node, text: &str, results: &mut Vec<String>, child_nr: usize) {
    if let Some(map_node) = node.child(child_nr).and_then(|node| {
        if node.kind() == "map_variable" {
            Some(node)
        } else {
            None
        }
    }) {
        let Ok(map_str) = map_node.utf8_text(text.as_bytes()) else {
            return;
        };

        let comma_count: usize = if let Some(indexes_list) = map_node.child(0) {
            let mut index_count: usize = 0;

            let mut cursor = indexes_list.walk();
            for idx_node in indexes_list.named_children(&mut cursor) {
                if idx_node.is_extra() {
                    continue;
                }
                index_count += 1;
            }
            index_count.saturating_sub(1)
        } else {
            0
        };

        let mut map_var = map_str.to_owned();
        if let Some((before, rest)) = map_str.split_once('[') {
            if let Some((_inside, after)) = rest.rsplit_once(']') {
                let replacement = ",".repeat(comma_count);
                map_var = format!("{}[{}]{}", before, replacement, after);
            }
        }

        if !results.contains(&map_var) {
            results.push(map_var);
        }
    }
}

fn node_with_block<'t>(node: &Node<'t>) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    let block = node
        .children(&mut cursor)
        .find(|&child| child.kind() == "block");
    block
}

fn add_variables_for_block(
    main_node: &Node,
    text: &str,
    line_nr: usize,
    char_nr: usize,
    results: &mut Vec<String>,
) {
    assert!(main_node.kind() == "block" || main_node.kind() == "action");

    let mut cursor = main_node.walk();
    for node in main_node.children(&mut cursor) {
        let pos = postition_relative_to_node(&node, line_nr, char_nr);
        if pos == Position::Before {
            break;
        }

        if let Some(block) = node_with_block(&node) {
            if postition_relative_to_node(&block, line_nr, char_nr) == Position::Within {
                add_variables_for_block(&block, text, line_nr, char_nr, results);
            }
        }

        let child_idx = match node.kind() {
            "assignment_statement" => 0,
            "declaration_statement" => 0,
            "for_statement" => 1,
            _ => continue,
        };

        add_scratch_variables_for_node(&node, text, results, child_idx);
        add_map_variables_for_node(&node, text, results, child_idx);
    }
}

pub fn find_variables_for_action(
    action: &Node,
    text: &str,
    line_nr: usize,
    char_nr: usize,
) -> Vec<String> {
    assert_eq!(action.kind(), "action");

    let mut results = Vec::new();

    add_variables_for_block(action, text, line_nr, char_nr, &mut results);

    results
}

fn node_to_source_file(n: Node) -> Option<Node> {
    let mut node = n;

    let source_file = loop {
        let parent = node.parent()?;

        if parent.kind() == "source_file" {
            break parent;
        }

        node = parent;
    };

    Some(source_file)
}

pub fn find_source_file_macros_for_action(action: &Node, text: &str) -> Vec<String> {
    assert_eq!(action.kind(), "action");

    let mut macros = Vec::new();

    let Some(source_file) = node_to_source_file(*action) else {
        return macros;
    };
    assert_eq!(source_file.kind(), "source_file");

    let mut cursor = source_file.walk();
    for node in source_file.named_children(&mut cursor) {
        if node.kind() == "macro_definition" {
            let Some(name_node) = node.child_by_field_name("name") else {
                continue;
            };
            if let Ok(macro_name) = name_node.utf8_text(text.as_bytes()) {
                macros.push(macro_name.to_owned());
            }
        }
    }

    macros
}

pub fn find_probes_vec_for_error(error_node: &Node, text: &str) -> Vec<String> {
    assert_eq!(error_node.kind(), "ERROR");
    let mut probes_vec: Vec<String> = Vec::new();

    let mut cursor = error_node.walk();
    for child_node in error_node.children(&mut cursor) {
        let probe;
        if child_node.kind() == "probes_list" {
            return probes_list_to_vec(&child_node, text);
        } else if child_node.kind() == "probe" {
            probe = child_node;
        } else {
            continue;
        }

        let probe_text = probe.utf8_text(text.as_bytes());
        let probe_str = probe_text.unwrap().to_string();
        probes_vec.push(probe_str);
    }

    probes_vec
}

pub fn find_probe_in_probes_list<'t>(
    probes_list: &Node<'t>,
    line_nr: usize,
    char_nr: usize,
) -> Option<Node<'t>> {
    assert_eq!(probes_list.kind(), "probes_list");

    let mut cursor = probes_list.walk();
    let ret = probes_list
        .children(&mut cursor)
        .find(|&probe| postition_relative_to_node(&probe, line_nr, char_nr) == Position::Within);
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

pub fn is_args_or_retval(line_str: &str, char_nr: usize) -> Option<String> {
    let line_upto_char = line_str.get(0..=char_nr)?;

    let mut words = line_upto_char.rsplit([' ', '[', '{', '(', ',']);
    let last_word = words.next()?;

    if last_word.starts_with("args.") {
        return Some(last_word.to_string());
    }

    // Handle:
    // retval.FIELDS and retval().FIELDS
    // retval->FIELDS and retval()->FIELDS
    if last_word.starts_with("retval.") || last_word.starts_with("retval->") {
        return Some(last_word.to_string());
    } else if last_word.starts_with(").") || last_word.starts_with(")->") {
        if let Some(word) = words.next() {
            if word.starts_with("retval") {
                let mut ret = "retval(".to_string();
                ret.push_str(last_word);

                return Some(ret);
            }
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

        let probes = find_probes_for_action(&action, text);
        assert_eq!(probes.len(), 1);
        assert_eq!(probes[0], "kretfunc:mac80211:ieee80211_deauth");
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

        let probes = find_probes_for_action(&action, text);
        assert_eq!(probes.len(), 1);
        assert_eq!(probes[0], "kfunc:vmlinux:posix_timer_fn");
    }

    #[test]
    fn test_find_one_variable_for_action() {
        let text = r#"
begin {
  $x = 10; $y = 1;
  $z =
}
        "#;
        let tree = setup_syntax_tree(text);

        let (loc, action) = find_syntax_location(text, &tree, 3, 4);
        assert_eq!(loc, SyntaxLocation::Action);
        assert_eq!(action.kind(), "action");

        let variables = find_variables_for_action(&action, text, 3, 4);
        assert_eq!(variables.len(), 2);
        assert_eq!(variables[0], "$x");
        assert_eq!(variables[1], "$y");
    }

    #[test]
    fn test_find_scoped_variables_for_action() {
        let text = r#"
begin {
  $x = 10; $y = 20;
  while ($x > 0) {
    $z = 8;
    $x--;
  }
  while ($y > 0) {
    $u = 8;
    $y--

  }
}
    "#;
        let tree = setup_syntax_tree(text);

        let (loc, action) = find_syntax_location(text, &tree, 10, 0);
        assert_eq!(loc, SyntaxLocation::Action);
        assert_eq!(action.kind(), "action");

        let variables = find_variables_for_action(&action, text, 10, 0);
        assert_eq!(variables.len(), 3);
        assert_eq!(variables[0], "$x");
        assert_eq!(variables[1], "$y");
        assert_eq!(variables[2], "$u");
    }

    #[test]
    fn test_find_range_variable() {
        let text = r#"
begin {
  for $i : 0..10 {
    $y =  
  }
}
    "#;
        let tree = setup_syntax_tree(text);

        let (loc, action) = find_syntax_location(text, &tree, 3, 10);
        assert_eq!(loc, SyntaxLocation::Action);
        assert_eq!(action.kind(), "action");

        let variables = find_variables_for_action(&action, text, 3, 10);
        assert_eq!(variables.len(), 1);
        assert_eq!(variables[0], "$i");
    }

    #[test]
    fn test_find_map_variables_simple() {
        let text = r#"
begin {
  @a[1,1] = 1;
  @a[2,1] = 2;
  @b[0] = 3;
  @c = 8;
  @d[/* block comment
        */@a[1,1], 8, 5] = 3;

}
    "#;
        let tree = setup_syntax_tree(text);

        let (loc, action) = find_syntax_location(text, &tree, 8, 0);
        assert_eq!(loc, SyntaxLocation::Action);
        assert_eq!(action.kind(), "action");

        let variables = find_variables_for_action(&action, text, 8, 0);
        assert_eq!(variables.len(), 4);
        assert_eq!(variables[0], "@a[,]");
        assert_eq!(variables[1], "@b[]");
        assert_eq!(variables[2], "@c");
        assert_eq!(variables[3], "@d[,,]");
    }

    #[test]
    fn test_find_all_map_variables() {
        let text = r#"
begin {
  @a[1,1] = 1;
  @a[2,1] = 2;
  @b[0] = 3;
  @c = 8;
  @d[/* block comment
        */@a[1,1], 8, 5] = 3;

}
    "#;
        let tree = setup_syntax_tree(text);

        let variables = find_all_map_variables(text, &tree.root_node());
        assert_eq!(variables.len(), 5);
        for v in variables {
            assert_eq!(v.kind(), "map_variable");
        }
    }

    #[test]
    fn test_find_file_macros() {
        let text = r#"
macro add_one(x) {
  x + 1
}

begin {
  $x = ;
}

macro add_two(y) {
  y + 2
}

    "#;
        let tree = setup_syntax_tree(text);

        let (loc, action) = find_syntax_location(text, &tree, 6, 7);
        assert_eq!(loc, SyntaxLocation::Action);
        assert_eq!(action.kind(), "action");

        let macros = find_source_file_macros_for_action(&action, text);
        assert_eq!(macros.len(), 2);
        assert_eq!(macros[0], "add_one");
        assert_eq!(macros[1], "add_two");
    }
}
