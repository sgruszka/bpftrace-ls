use json::{self, object};
use std::collections::HashMap;
use std::str::Lines;
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::Instant;
use tree_sitter::Node;

use crate::btf_mod::{
    btf_iterate_over_names_chain, btf_resolve_func, btf_setup_module, ResolvedBtfItem,
    ResolvedVariable,
};
use crate::cmd_mod::bpftrace_command;
use crate::gen::completion::{bpftrace_probe_providers, bpftrace_stdlib_functions};
use crate::log_mod::{self, COMPL, HOVER};
use crate::parser::{self, SyntaxLocation};
use crate::DOCUMENTS_STATE;
use crate::{log_dbg, log_err, log_vdbg};
use btf_rs::Btf;

static PROBES_ARGS_MAP: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static MODULE_BTF_MAP: LazyLock<Mutex<HashMap<String, Btf>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static AVAILABE_TRACES: OnceLock<Option<String>> = OnceLock::new();

static FENTRY_KFUNC_NAME: OnceLock<&'static str> = OnceLock::new();

fn btf_item_to_str(item: &ResolvedBtfItem, with_name: bool) -> String {
    let mut s = item.type_vec.join(" ").to_string();
    if with_name {
        s.push_str(" ");
        s.push_str(&item.name);
    }
    s
}

fn children_to_vec_str(resolved: &ResolvedBtfItem) -> Vec<String> {
    let mut results: Vec<String> = Vec::new();
    for child in resolved.children_vec.iter() {
        results.push(btf_item_to_str(child, true));
    }

    results
}

fn resolve_args_name_chain(
    module: String,
    resolved_func: ResolvedBtfItem,
    this_argument: &str,
) -> Option<ResolvedVariable> {
    // log_dbg!(COMPL, "MODULE {}", module);
    // log_dbg!(COMPL, "RESOLVED FUNC {:?}", resolved_func);
    // log_dbg!(COMPL, "THIS_ARGUMENT {}", this_argument);
    // let mut names_chain: Vec<&str> = args_str_to_tokens(this_argument);
    // names_chain.remove(0); // skip "args"
    // names_chain.remove(0); // skip "."
    //
    // log_dbg!(COMPL, "NAMES CHAIN {:?}", names_chain);

    // log_dbg!(
    //     COMPL,
    //     "Looking for next item for name chain {:?}",
    //     names_chain
    // );

    let module_btf_map = MODULE_BTF_MAP.lock().unwrap();

    if let Some(resolved_var) = module_btf_map
        .get(&module)
        .and_then(|btf| btf_iterate_over_names_chain(btf, &resolved_func, this_argument))
    {
        return Some(resolved_var);
    }

    None
}

fn is_fentry_probe(probe: &str) -> bool {
    probe.starts_with("fentry") || probe.starts_with("kfunc")
}

fn is_fexit_probe(probe: &str) -> bool {
    probe.starts_with("fexit") || probe.starts_with("kretfunc")
}

fn is_btf_probe(probe: &str) -> bool {
    is_fentry_probe(probe) || is_fexit_probe(probe)
}

fn kprobe_to_kfunc(probe: &str) -> String {
    let mut v: Vec<&str> = probe.split(":").collect();
    if v[0] == "kprobe" {
        v[0] = "kfunc";
    } else if v[0] == "kretprobe" {
        v[0] = "kretfunc";
    }
    let kfunc = v[..].join(":").to_string();

    kfunc
}

fn find_probe_args_by_command(probe: &str) -> String {
    if probe.is_empty() {
        return "".to_string();
    }

    // Use kfunc for getting arguments, kprobe/kretprobe does not work
    let probe = kprobe_to_kfunc(probe);

    let mut probes_args_map = PROBES_ARGS_MAP.lock().unwrap();

    let mut probe_args = "".to_string();
    if let Some(args) = probes_args_map.get(&probe) {
        probe_args = args.to_string();
    } else if let Ok(output) = bpftrace_command(&["-l", "-v", &probe]) {
        if let Ok(stdout_probe_args) = String::from_utf8(output.stdout) {
            probe_args = stdout_probe_args.clone();
        }
        if let Ok(stderr_probe_args) = String::from_utf8(output.stderr) {
            probe_args.push_str(&stderr_probe_args);
        }

        if probe_args.is_empty() {
            log_err!("No arguments for probe {}", probe);
        } else {
            probes_args_map.insert(probe.clone(), probe_args.clone());
            log_dbg!(COMPL, "Found arguments using command line\n{}", probe_args);
        }
    }

    probe_args
}

fn find_kfunc_args_by_btf(kfunc: &str, need_retval: bool) -> Option<(String, ResolvedBtfItem)> {
    let kfunc_vec: Vec<&str> = kfunc.split(":").collect();
    log_dbg!(COMPL, "kfunc_vec {:?}", kfunc_vec);

    if kfunc_vec.len() != 3 {
        return None;
    }

    let module = kfunc_vec[1];
    if module.is_empty() {
        return None;
    }

    let mut module_btf_map = MODULE_BTF_MAP.lock().unwrap();

    let this_btf;
    if let Some(btf) = module_btf_map.get(module) {
        this_btf = btf;
    } else {
        log_dbg!(COMPL, "Looking for btf for module: {}", module);
        if let Some(btf) = btf_setup_module(module) {
            module_btf_map.insert(module.to_string(), btf);
            this_btf = module_btf_map.get(module).unwrap();
        } else {
            return None;
        }
    }

    if let Some(ret) = btf_resolve_func(this_btf, kfunc_vec[2], need_retval) {
        return Some((module.to_string(), ret));
    }

    None
}

fn items_from_resolved_btf(btf_item: &ResolvedBtfItem) -> json::JsonValue {
    let mut items = json::JsonValue::new_array();

    for child in btf_item.children_vec.iter() {
        let completion = object! {
            "label": child.name.clone(),
            "kind" : 5,
            "detail" : btf_item_to_str(child, false),
            // TODO
            // "documentation" : field_type,
        };
        let _ = items.push(completion);
    }
    items
}

fn items_from_probe_args(probe_args_iter: Lines) -> json::JsonValue {
    let mut items = json::JsonValue::new_array();

    for arg in probe_args_iter {
        let tokens: Vec<&str> = arg.split(" ").collect();
        if tokens.len() <= 1 {
            continue;
        }
        let end = tokens.len() - 1;

        // let field = format!("args.{}", tokens[end]);
        let field = tokens[end];
        let _field_type = tokens[0..end].join(" ").to_string();
        let completion = object! {
            "label": field,
            "kind" : 5,
            "detail" : arg, //field_type.clone(),
            // TODO
            // "documentation" : field_type,
        };
        let _ = items.push(completion);
    }
    items
}

// Complete args. i.e. kfunc:xe:__fini_dbm { printf("%s\n", str(args.drm->driver->name)) }
fn encode_completion_for_args_keyword(
    probe: &str,
    args_with_fields: &str,
) -> Option<json::JsonValue> {
    log_dbg!(COMPL, "Complete for argument: {}", args_with_fields);

    let mut is_kfunc = false;
    let mut need_retval = false;

    if probe.starts_with("kprobe:") || probe.starts_with("fentry") || probe.starts_with("kfunc:") {
        is_kfunc = true;
    }

    if probe.starts_with("kretprobe:")
        || probe.starts_with("kretfunc:")
        || probe.starts_with("fexit")
    {
        is_kfunc = true;
        need_retval = true;
    }

    let mut btf_probe_args = None;
    if is_kfunc {
        let kfunc = kprobe_to_kfunc(probe);
        btf_probe_args = find_kfunc_args_by_btf(&kfunc, need_retval);
    }

    let items = if args_with_fields.ends_with("args.") && !is_kfunc {
        let probe_args = find_probe_args_by_command(probe);
        let mut probe_args_iter = probe_args.lines();

        // On first line of probe args is kfunc module and name
        probe_args_iter.next();
        items_from_probe_args(probe_args_iter)
    } else if let Some(next_items) = btf_probe_args
        .and_then(|(module, resolved_btf)| {
            resolve_args_name_chain(module, resolved_btf, args_with_fields)
        })
        .and_then(|item| item.var_type)
    {
        // For debug:
        // let args = children_to_vec_str(&next_items);
        // let args_as_string.push_str(&args.join("\n"));
        // log_dbg!(COMPL, "Found arguments using btf:\n{}", args_as_string);

        items_from_resolved_btf(&next_items)
    } else {
        json::JsonValue::new_array()
    };

    let is_incomplete = false; // Currently we provide complete list
    let data = object! {
        "result": {
            "isIncomplete": is_incomplete,
            "items": items,
        }
    };

    Some(data)
}

fn encode_completion_for_action(
    _text: &str,
    _node: &Node,
    _line_str: &str,
    _char_nr: usize,
) -> Option<json::JsonValue> {
    log_dbg!(COMPL, "Complete for action block");

    // TODO preload btf module
    let mut items = json::JsonValue::new_array();

    bpftrace_stdlib_functions(&mut items);

    let completion_args = object! {
        "label": "args",
        "kind" : 5,
        "detail" : "args",
        "documentation" : r#"
This keyword represents the struct of all arguments of the traced function.
You can print the entire structure via `print(args)` or access particular fields using the dot syntax, e.g., `$x = str(args.filename);`. "#,
    };
    let _ = items.push(completion_args);
    let is_incomplete = false; // Currently we provide complete list
    let data = object! {
        "result": {
            "isIncomplete": is_incomplete,
            "items": items,
        }
    };

    Some(data)
}

#[allow(clippy::needless_range_loop)]
fn func_proto_str(item: &ResolvedBtfItem) -> String {
    let mut s = String::new();
    let params = &item.children_vec;

    let mut l = params.len();

    if l > 0 && params[l - 1].name == "retval" {
        s.push_str(&params[l - 1].type_vec.join(" ").to_string());
        l -= 1;
    } else {
        s.push_str("void");
    }

    s.push_str(" ");
    s.push_str(&item.name);

    s.push_str("(");
    for i in 0..l {
        let p = &params[i];

        s.push_str(&p.type_vec.join(" "));
        if !s.ends_with("*") {
            s.push_str(" ");
        }
        s.push_str(&p.name);
        if i < l - 1 {
            s.push_str(", ")
        }
    }
    s.push_str(");");

    s
}

fn bpftrace_get_traces_list() -> Option<String> {
    let start = Instant::now();

    let Ok(output) = bpftrace_command(&["-l"]) else {
        log_err!("Failed to get output from bpftrace command");
        return None;
    };

    let Ok(traces) = String::from_utf8(output.stdout) else {
        log_err!("Failed to convert stdout to string");
        return None;
    };

    log_dbg!(
        COMPL,
        "Got list of available traces after {:?}",
        start.elapsed()
    );

    log_vdbg!(COMPL, "List of available traces: \n{traces}\n");
    Some(traces)
}

pub fn init_available_traces() {
    let _ = AVAILABE_TRACES.get_or_init(bpftrace_get_traces_list);
}

fn encode_completion_for_line(
    prefix: &str,
    line_str: &str,
    short_prefix: Option<&str>,
) -> Option<json::JsonValue> {
    log_dbg!(
        COMPL,
        "Check completion for prefix '{}' with short name {:?}",
        prefix,
        short_prefix
    );

    // TOOD separate traces for each type i.e. kprobe, tracepoint
    // TODO add kretprobe, kretfunc support
    let Some(available_traces) = AVAILABE_TRACES.get_or_init(bpftrace_get_traces_list) else {
        return Some(encode_no_completion());
    };

    let mut items = json::JsonValue::new_array();
    let mut is_incomplete = false;

    let max_count = 200;
    let mut count = max_count;
    let mut duplicates: HashMap<String, u32> = HashMap::new();

    let mut line_tokens: Vec<&str> = line_str.split(":").collect();

    if let Some(short_name) = short_prefix {
        assert!(line_str.trim().starts_with(short_name));
        line_tokens[0] = prefix;
    }

    if line_tokens[0] == "kfunc"
        || line_tokens[0] == "kretfunc"
        || line_tokens[0] == "fentry"
        || line_tokens[0] == "fexit"
    {
        let fentry_kfunc_name = FENTRY_KFUNC_NAME.get_or_init(||
            // TOOD: rewrite this to have separte lines for each probe type
            if available_traces.contains("\nfentry") {
                "fentry"
            } else {
                "kfunc"
            }
        );
        line_tokens[0] = fentry_kfunc_name;
    } else if line_tokens[0] == "kretprobe" {
        line_tokens[0] = "kprobe";
    }

    let search_line = line_tokens.join(":");
    log_dbg!(COMPL, "Searching for line '{}'", search_line);

    for trace_line in available_traces.lines() {
        if trace_line.trim().starts_with(search_line.trim()) {
            //TODO: save matched tokens ans skip duplicate lines here
            // log_vdbg!(COMPL, "Found available traces line '{}'", trace_line);

            let trace_tokens: Vec<&str> = trace_line.split(":").collect();

            let mut match_tokens = 0;
            for i in 0..std::cmp::min(trace_tokens.len(), line_tokens.len()) {
                if trace_tokens[i] != line_tokens[i] {
                    break;
                }
                match_tokens += 1;
            }

            if trace_tokens.len() > match_tokens {
                let label = trace_tokens[match_tokens];

                match duplicates.get(label) {
                    None => duplicates.insert(label.to_string(), 1),
                    Some(_) => continue,
                };

                let kind = if match_tokens == trace_tokens.len() - 1 {
                    10 // Property
                } else {
                    9 // Module
                };

                let mut item = object! {
                    "label": label,
                    "kind": kind,
                    // "detail": "TODO",
                    // "documentation": "need better documentation",
                };

                if (trace_tokens[0] == "kfunc" || trace_tokens[0] == "fentry") && kind == 10 {
                    if let Some((_module, resolved_btf)) = find_kfunc_args_by_btf(trace_line, true)
                    {
                        item["detail"] = func_proto_str(&resolved_btf).into();
                    }
                }

                log_vdbg!(
                    COMPL,
                    "Adding complete item: {} : {}",
                    label,
                    item["detail"].to_string()
                );

                let _ = items.push(item);
                count -= 1;
                if count < 0 {
                    is_incomplete = true;
                    break;
                }
            }
        }
    }

    let data = object! {
        "result": {
            "isIncomplete": is_incomplete,
            "items": items,
        }
    };

    Some(data)
}

fn encode_completion_for_empty_line() -> json::JsonValue {
    let mut items = json::JsonValue::new_array();

    bpftrace_probe_providers(&mut items);

    let data = object! {
        "result": {
            "isIncomplete": false,
            "items": items,
        }
    };

    data
}

fn encode_no_completion() -> json::JsonValue {
    let items = json::JsonValue::new_array();
    let empty_data = object! {
        "result": {
            "isIncomplete": false,
            "items": items,
        }
    };

    empty_data
}

fn encode_completion_for_probes(line_str: &str) -> json::JsonValue {
    let prefixes = [
        ("begin", None),
        ("end", None),
        ("test", None),
        ("bench", None),
        ("self", None),
        ("interval", Some("i")),
        ("profile", Some("p")),
        ("iter", Some("it")),
        ("hardware", Some("h")),
        ("software", Some("s")),
        ("rawtracepoint", Some("rt")),
        ("tracepoint", Some("t")),
        ("kprobe", Some("k")),
        ("kretprobe", Some("kr")),
        ("kfunc", None),
        ("kretfunc", None),
        ("fentry", Some("f")),
        ("fexit", Some("fr")),
    ];

    if !line_str.is_empty() {
        for prefix in prefixes.iter() {
            if !line_str.trim().starts_with(prefix.0) {
                continue;
            }
            if let Some(data) = encode_completion_for_line(prefix.0, line_str, None) {
                return data;
            }
        }

        for prefix in prefixes.iter() {
            let Some(short_prefix) = prefix.1 else {
                continue;
            };

            if line_str.trim().len() <= short_prefix.len()
                || line_str.trim().chars().nth(short_prefix.len()) != Some(':')
            {
                continue;
            }

            if !line_str.trim().starts_with(short_prefix) {
                continue;
            }

            if let Some(data) = encode_completion_for_line(prefix.0, line_str, prefix.1) {
                return data;
            }
        }
    }

    encode_completion_for_empty_line()
}

fn unpack_text_document_info(content: json::JsonValue) -> (String, usize, usize) {
    let uri = content["params"]["textDocument"]["uri"].to_string();

    let position = &content["params"]["position"];

    let line_nr = position["line"].as_usize().unwrap_or_default();
    let char_nr = position["character"]
        .as_usize()
        .unwrap_or_default()
        .saturating_sub(1);

    (uri, line_nr, char_nr)
}

macro_rules! get_document_state {
    ($text_doc:ident, $line_nr:ident, $char_nr:ident, $none:expr, $log:ident) => {{
        let Some(tree) = &$text_doc.syntax_tree else {
            return $none;
        };

        let text = &$text_doc.text;

        let log_str = match $log {
            COMPL => "Completion",
            HOVER => "Hover",
            _ => "",
        };

        let line_str = text.lines().nth($line_nr).unwrap_or_default();
        log_dbg!(
            $log,
            "{} for line {}: '{}', char at {}: '{}'",
            log_str,
            $line_nr,
            line_str,
            $char_nr,
            line_str.chars().nth($char_nr).unwrap_or_default()
        );

        let (loc, node) = parser::find_syntax_location(text, tree, $line_nr, $char_nr);
        log_dbg!($log, "{}: found syntax location: {:?}", log_str, loc);

        (text, loc, node, line_str)
    }};
}

#[allow(clippy::collapsible_else_if)]
pub fn encode_completion(content: json::JsonValue) -> json::JsonValue {
    let (uri, line_nr, char_nr) = unpack_text_document_info(content);

    let Some(text_doc) = DOCUMENTS_STATE.get(&uri) else {
        return encode_no_completion();
    };

    let (text, loc, node, line_str) =
        get_document_state!(text_doc, line_nr, char_nr, encode_no_completion(), COMPL);

    if loc == SyntaxLocation::Action {
        if let Some(args) = parser::is_argument(line_str, char_nr) {
            let probe = parser::find_probe_for_action(&node, text);
            if !probe.is_empty() {
                log_dbg!(COMPL, "Found probe {}", probe);

                if let Some(data) = encode_completion_for_args_keyword(&probe, &args) {
                    return data;
                }
            }
        } else {
            if let Some(data) = encode_completion_for_action(text, &node, line_str, char_nr) {
                return data;
            }
        }
    }

    if loc == SyntaxLocation::SourceFile && node.has_error() {
        if let Some(args) = parser::is_argument(line_str, char_nr) {
            if let Some(error_node) = parser::find_error_location(text, &node, line_nr, char_nr) {
                let probe = parser::find_probe_for_error(&error_node, text);
                if !probe.is_empty() {
                    log_dbg!(COMPL, "Found probe {}", probe);

                    if let Some(data) = encode_completion_for_args_keyword(&probe, &args) {
                        return data;
                    }
                }
            }
        }
    }

    if loc != SyntaxLocation::Comment {
        let up_to_char = char_nr.saturating_add(1);
        let line_head = if let Some(splited_line) = line_str.split_at_checked(up_to_char) {
            let (head, _tail) = splited_line;
            head
        } else {
            line_str
        };

        log_dbg!(COMPL, "Complete for line head: '{line_head}'");
        return encode_completion_for_probes(line_head);
    }

    encode_no_completion()
}

pub fn encode_completion_resolve(content: json::JsonValue) -> json::JsonValue {
    // TODO
    log_dbg!(COMPL, "Completion resolve for: {}", content);

    let params = content["params"].clone();
    // TOOD: use clangd to get documentation ?
    // params["documentation"] = "Do this MARKUP".into();
    log_dbg!(COMPL, "documentation {}", params["documentation"]);

    let data = object! {
        "result": params,
    };

    data
}

fn find_hover_str<LF, RF>(line: &str, char_nr: usize, lcond: LF, rcond: RF) -> String
where
    LF: Fn(char) -> bool,
    RF: Fn(char) -> bool,
{
    let mut found = "";

    if line.len() > char_nr {
        let mut l = 0;
        let mut r = line.len();
        for (i, c) in line.chars().enumerate() {
            if i == char_nr && lcond(c) {
                return "".to_string();
            }

            if lcond(c) && i <= char_nr {
                l = i + 1;
            }

            if rcond(c) && i > char_nr {
                r = i;
                break;
            }
        }
        if found.is_empty() && l < r {
            found = &line[l..r];
        }
    }

    found.to_string()
}

pub fn encode_hover(content: json::JsonValue) -> json::JsonValue {
    log_dbg!(HOVER, "Received hover with data {}", content);
    let (uri, line_nr, char_nr) = unpack_text_document_info(content);

    let empty_data = object! {};
    let mut data = object! {};

    let Some(text_doc) = DOCUMENTS_STATE.get(&uri) else {
        return empty_data;
    };

    let (text, loc, node, line_str) = get_document_state!(text_doc, line_nr, char_nr, data, HOVER);

    if loc == SyntaxLocation::ProbesList {
        assert_eq!(node.kind(), "probes_list");

        // TODO extend beyond just single probes
        if node.child_count() < 1 {
            return empty_data;
        }
        let probe_node = node.child(0).unwrap();

        let probe = probe_node.utf8_text(text.as_bytes()).unwrap_or_default();
        log_dbg!(HOVER, "Hover for probe {}", probe);

        if !is_btf_probe(probe) {
            return empty_data;
        }

        let args_by_btf = find_kfunc_args_by_btf(probe, true);
        if let Some((_module, resolved_btf)) = args_by_btf {
            data = object! {
                  "result": {
                      "contents": format!("{}:\n```c\n{}```", probe, func_proto_str(&resolved_btf)),
                  },
            };
        }
    } else if loc == SyntaxLocation::Action {
        let probe = parser::find_probe_for_action(&node, text);
        // TODO: non BTF probes i.e. tracepoints
        // let probe_args = find_probe_args_by_command(&probe);
        // log_dbg!(HOVER, "Probe {} with args:\n{}", probe, probe_args);

        let lterm = |c: char| -> bool { c.is_whitespace() || c == '{' || c == '(' };
        let rterm =
            |c: char| -> bool { c.is_whitespace() || c == '}' || c == ')' || c == '.' || c == '-' };
        let mut found = find_hover_str(line_str, char_nr, lterm, rterm);
        log_dbg!(HOVER, "Hover found args string {}", found);

        if found == "args" {
            found.push('.');
        }
        let btf_probe_args = find_kfunc_args_by_btf(&probe, true);
        let Some((module, resolved_func)) = btf_probe_args else {
            return empty_data;
        };

        // log_dbg!(HOVER, "Resolved BTF {:?}", resolved_btf);
        let Some(resolved_variable) = resolve_args_name_chain(module, resolved_func, &found) else {
            return empty_data;
        };

        let mut hover = btf_item_to_str(&resolved_variable.var, true);
        if let Some(var_type) = resolved_variable.var_type {
            let s = format!(
                ":\n```c\n{}\n{{\n",
                btf_item_to_str(&var_type, true).replace("* ", "")
            );
            hover.push_str(&s);

            // Structure/union members
            let mut max_type_width = 0;
            for child in var_type.children_vec.iter() {
                let width = btf_item_to_str(child, false).len();
                if width > max_type_width {
                    max_type_width = width;
                }
            }

            for child in var_type.children_vec.iter() {
                let s = format!(
                    "        {:<width$} {}; \n",
                    btf_item_to_str(child, false),
                    &child.name,
                    width = max_type_width
                );
                hover.push_str(&s);
            }

            hover.push_str("};```");
        }

        log_vdbg!(HOVER, "Hover:\n{:?}", hover);

        data = object! {
              "result": {
                  "contents": hover,
              },
        };
    }

    data
}

#[allow(clippy::len_zero)]
#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static URI_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn preload_probes_args(probes_vec: &[&str]) {
        let probes_str = probes_vec.join(",");

        let shell_cmd = format!(r#"(sudo bpftrace -l -v "{}") 2>&1"#, probes_str);

        let Ok(output) = Command::new("sh").arg("-c").arg(shell_cmd).output() else {
            return;
        };

        let Ok(all_probes_args) = String::from_utf8(output.stdout) else {
            return;
        };

        if all_probes_args.is_empty() {
            log_err!("No arguments for probe {}", probes_str);
            return;
        }

        let mut probe = String::new();
        let mut probe_args = String::new();

        for line in all_probes_args.lines() {
            if line.starts_with(" ") {
                probe_args.push_str(line);
                probe_args.push('\n');
            } else {
                if !probe.is_empty() {
                    let mut probes_args_map = PROBES_ARGS_MAP.lock().unwrap();
                    probes_args_map.insert(probe, probe_args);
                }
                probe = line.to_string();
                probe_args = line.to_string();
                probe_args.push('\n');
            }
        }

        if !probe.is_empty() {
            let mut probes_args_map = PROBES_ARGS_MAP.lock().unwrap();
            probes_args_map.insert(probe, probe_args);
        }
    }

    fn compare_btf_and_cmd(s: &str) {
        let args_by_cmd = find_probe_args_by_command(s);
        let args_by_btf = find_kfunc_args_by_btf(s, true);

        let resolved_btf = if let Some((_module, resolved_btf)) = args_by_btf {
            resolved_btf
        } else {
            panic!();
        };

        // for (i, c) in resolved_btf.children_vec.iter().enumerate() {
        //     println!("{i}: '{}'", btf_item_to_str(c, true).trim());
        // }

        let mut n = 0;
        for (i, arg) in args_by_cmd.lines().enumerate() {
            if i == 0 {
                // fentry vs kfunc, the rest after colon must be the same
                assert!(arg.split_once(":").map(|x| x.1) == s.split_once(":").map(|x| x.1));
                continue;
            }

            assert!(resolved_btf.children_vec.len() > i - 1);

            let btf_item = &resolved_btf.children_vec[i - 1];
            assert!(arg.trim() == btf_item_to_str(btf_item, true));
            n += 1;
        }
        assert!(resolved_btf.children_vec.len() == n);
    }

    fn document_content_setup(text: &str, line_nr: usize, char_nr: usize) -> json::JsonValue {
        let uri = format!(
            "file:///completion_test{}.bt",
            URI_COUNTER.fetch_add(1, Ordering::Relaxed)
        );

        DOCUMENTS_STATE.set(uri.to_string(), text.to_string(), 1);

        object! {
            "params": {
                "textDocument": {
                    "uri": uri,
                },
                "position": {
                    "line": line_nr,
                    "character": char_nr,
                }
            }
        }
    }

    fn check_completion_resutls(result: json::JsonValue, values: Vec<&str>) {
        let labels: Vec<_> = result["result"]["items"]
            .members()
            .map(|item| item["label"].to_string())
            .collect();

        for val in values.iter() {
            assert!(
                labels.contains(&val.to_string()),
                "'{val}' missed in completion results: {:?}",
                labels
            );
        }
    }

    #[test]
    fn test_find_probe_args() {
        let probes = vec![
            "kfunc:vmlinux:posixtimer_free_timer",
            "kfunc:vmlinux:acpi_unregister_gsi",
            "kfunc:vmlinux:acpi_register_gsi",
            "kfunc:vmlinux:vfs_open",
        ];
        preload_probes_args(&probes);
        for p in probes {
            compare_btf_and_cmd(p);
        }
    }

    #[test]
    fn test_action_completion_for_do_sys_open() {
        let text = "kprobe:do_sys_open { ";
        let json_content = document_content_setup(text, 0, text.len() - 1);

        let result = encode_completion(json_content);
        assert!(result["result"]["items"].len() > 0);

        let functions = vec![
            "printf", "print", "str", "strlen", "assert", "cpu", "curtask", "exit", "is_ptr",
        ];
        check_completion_resutls(result, functions);
    }

    #[test]
    fn test_probes_completion_for_empty_line() {
        let json_content = document_content_setup("", 0, 0);

        let result = encode_completion(json_content);
        assert!(result["result"]["items"].len() > 0);

        let prefixes = vec![
            "iter",
            "hardware",
            "tracepoint",
            "kprobe",
            "kretprobe",
            "software",
            "rawtracepoint",
            "kfunc",
            "kretfunc",
        ];
        check_completion_resutls(result, prefixes);
    }

    #[test]
    fn test_probes_completion_for_modules() {
        for text in vec!["kfunc:", "kretfunc:", "fentry:", "fexit:"].into_iter() {
            let json_content = document_content_setup(text, 0, text.len() - 1);

            let result = encode_completion(json_content);
            assert!(result["result"]["items"].len() > 0);

            // TODO other items than vmlinux? Use 'lsmod' ?
            check_completion_resutls(result, vec!["vmlinux"]);
        }
    }

    #[test]
    fn test_probes_completion_for_vfs_functions() {
        let text = "kfunc:vmlinux:vfs_";
        let json_content = document_content_setup(text, 0, text.len() - 1);

        let result = encode_completion(json_content);
        assert!(result["result"]["items"].len() > 0);

        let functions = vec![
            "vfs_open",
            "vfs_read",
            "vfs_write",
            "vfs_fstatat",
            "vfs_mknod",
            "vfs_llseek",
            "vfs_readv",
            "vfs_writev",
            "vfs_truncate",
            "vfs_unlink",
        ];
        check_completion_resutls(result, functions);
    }

    #[test]
    fn test_args_completion_for_hrtimer_base() {
        let text = r#"kfunc:vmlinux:posix_timer_fn { printf("%d\n", args.timer->base-> ); }"#;
        let json_content = document_content_setup(text, 0, text.len() - 5);

        let result = encode_completion(json_content);
        assert!(result["result"]["items"].len() > 0);

        let fields = vec![
            "cpu_base", "index", "clockid", "seq", "running", "active", "get_time", "offset",
        ];
        check_completion_resutls(result, fields);
    }

    #[test]
    fn test_args_completion_for_posix_cpu_clock_get() {
        let text = r#"fexit:vmlinux:posix_cpu_clock_get { args. }"#;
        let json_content = document_content_setup(text, 0, text.len() - 2);

        let result = encode_completion(json_content);
        assert!(result["result"]["items"].len() > 0);

        let fields = vec!["retval", "tp", "clock"];
        check_completion_resutls(result, fields);
    }

    #[test]
    fn test_modules_completion_for_short_tracepoint() {
        let text = r#"t:"#;
        let json_content = document_content_setup(text, 0, text.len());

        let result = encode_completion(json_content);
        assert!(result["result"]["items"].len() > 0);

        let fields = vec![
            "vmalloc",
            "syscalls",
            "timer",
            "notifier",
            "workqueue",
            "writeback",
            "dma",
        ];
        check_completion_resutls(result, fields);
    }

    #[test]
    fn test_modules_completion_for_short_clk() {
        let text = r#"t:clk:"#;
        let json_content = document_content_setup(text, 0, text.len());

        let result = encode_completion(json_content);
        assert!(result["result"]["items"].len() > 0);

        let fields = vec![
            "clk_disable",
            "clk_disable_complete",
            "clk_enable",
            "clk_enable_complete",
            "clk_prepare",
            "clk_prepare_complete",
            "clk_set_max_rate",
            "clk_set_min_rate",
        ];
        check_completion_resutls(result, fields);
    }

    #[test]
    fn test_missing_right_bracket_action() {
        let text = r#"t:syscalls:sys_enter_bpf { args."#;
        let json_content = document_content_setup(text, 0, text.len());
        let result = encode_completion(json_content);
        assert!(result["result"]["items"].len() > 0);

        let fields = vec!["size", "cmd", "uattr"];
        check_completion_resutls(result, fields);
    }

    #[test]
    fn test_missing_left_bracket_action() {
        let text = r#"t:syscalls:sys_enter_bpf args. }"#;
        let json_content = document_content_setup(text, 0, text.len() - 2);
        let result = encode_completion(json_content);
        assert!(result["result"]["items"].len() > 0);

        let fields = vec!["size", "cmd", "uattr"];
        check_completion_resutls(result, fields);
    }

    #[test]
    fn test_hover_for_kfunc() {
        let text = r"kfunc:vmlinux:posix_timer_fn {}";
        let json_content = document_content_setup(text, 0, text.len() - 10);
        let result = encode_hover(json_content);

        let hover = result["result"]["contents"].as_str().unwrap();
        assert!(hover.contains(r"kfunc:vmlinux:posix_timer_fn"));
        assert!(hover.contains(r"hrtimer_restart posix_timer_fn(struct hrtimer *timer)"));
    }
}
