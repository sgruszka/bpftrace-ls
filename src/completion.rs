use json::{self, object};
use once_cell::sync::{Lazy, OnceCell};
use std::collections::HashMap;
use std::process::Command;
use std::str::Lines;
use std::sync::Mutex;

use crate::btf_mod::{
    btf_iterate_over_names_chain, btf_resolve_func, btf_setup_module, ResolvedBtfItem,
};
use crate::log_mod::{self, COMPL, HOVER};
// use crate::DocumentsState;
use crate::DOCUMENTS_STATE;
use crate::{log_dbg, log_vdbg};
use btf_rs::Btf;

static PROBES_ARGS_MAP: Lazy<Mutex<HashMap<String, String>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

static MODULE_BTF_MAP: Lazy<Mutex<HashMap<String, Btf>>> = Lazy::new(|| Mutex::new(HashMap::new()));

fn get_text(uri: &str) -> String {
    if let Some(text_doc) = DOCUMENTS_STATE.get(uri) {
        return text_doc.text.to_string();
    };

    "".to_string()
}

fn get_line(uri: &str, line_nr: usize) -> String {
    let mut from_line = String::new();
    if let Some(text_doc) = DOCUMENTS_STATE.get(uri) {
        for (i, line) in text_doc.text.lines().enumerate() {
            if i == line_nr {
                from_line = line.to_string();
            }
        }
    }

    from_line
}

fn is_action_block(text: &str, line_nr: usize, char_nr: usize) -> bool {
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

fn is_argument(line_str: &str, char_nr: usize, args: &mut String) -> bool {
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

fn btf_item_to_str(item: &ResolvedBtfItem) -> String {
    let mut s = item.type_vec.join(" ").to_string();
    s.push_str(" ");
    s.push_str(&item.name);
    s
}

fn children_to_vec_str(resolved: &ResolvedBtfItem) -> Vec<String> {
    let mut results: Vec<String> = Vec::new();
    for child in &resolved.children_vec[..] {
        let mut res_str = String::new();
        for t in child.type_vec.iter() {
            res_str.push_str(t);
            res_str.push_str(" ");
        }
        res_str.push_str(&child.name);
        results.push(res_str);
    }

    results
}

fn argument_next_item(
    module: String,
    resolved_func: ResolvedBtfItem,
    this_argument: &str,
) -> ResolvedBtfItem {
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

    if let Some(btf) = module_btf_map.get(&module) {
        if let Some(resolved) = btf_iterate_over_names_chain(&btf, &resolved_func, this_argument) {
            return resolved;
        }
    }

    ResolvedBtfItem::default()
}

fn find_probe_for_action(text: &str, line_nr: usize) -> String {
    if let Some(line) = text.lines().nth(line_nr) {
        if let Some(char_nr) = line.find("{") {
            let trimed = line[0..char_nr].trim();
            if trimed.len() > 0 {
                return trimed.to_string();
            } else {
                let prev_line_nr = line_nr - 1;
                if let Some(line_prev) = text.lines().nth(prev_line_nr) {
                    return line_prev.trim().to_string();
                }
            }
        }
    }
    "".to_string()
}

fn kprobe_to_kfunc(probe: &str) -> String {
    let mut v: Vec<&str> = probe.split(":").collect();
    if v[0] == "kprobe" || v[0] == "kretprobe" {
        v[0] = "kfunc";
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

    let mut this_probe_args = "".to_string();
    if let Some(args) = probes_args_map.get(&probe) {
        this_probe_args = args.to_string();
    } else if let Ok(output) = Command::new("sudo")
        .arg("bpftrace")
        .arg("-l")
        .arg("-v")
        .arg(probe.clone())
        .output()
    {
        if let Ok(probe_args) = String::from_utf8(output.stdout) {
            this_probe_args = probe_args.clone();
            if let Ok(stderr_probe_args) = String::from_utf8(output.stderr) {
                this_probe_args.push_str(&stderr_probe_args);
            }
            probes_args_map.insert(probe.clone(), probe_args.clone());
            log_dbg!(
                COMPL,
                "Found arguments using command line\n{}",
                this_probe_args
            );
        }
    }

    this_probe_args
}

fn find_kfunc_args_by_btf(kfunc: &str) -> Option<(String, ResolvedBtfItem)> {
    let kfunc_vec: Vec<&str> = kfunc.split(":").collect();
    if kfunc_vec.len() < 3 {
        return None;
    }

    log_dbg!(COMPL, "kfunc_vec {:?}", kfunc_vec);

    let module = kfunc_vec[1];
    assert!(!module.is_empty());

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

    if let Some(ret) = btf_resolve_func(&this_btf, kfunc_vec[2]) {
        return Some((module.to_string(), ret));
    }

    None
}

fn encode_completion_for_action(
    text: &str,
    line_str: &str,
    line_nr: usize,
    char_nr: usize,
) -> Option<json::JsonValue> {
    if !is_action_block(text, line_nr, char_nr) {
        return None;
    }
    log_dbg!(COMPL, "Complete for action block");

    let mut items = json::JsonValue::new_array();
    let is_incomplete = false; // Currently we provide complete list

    let probe = find_probe_for_action(text, line_nr);
    if !probe.is_empty() {
        log_dbg!(COMPL, "Found probe {}", probe);
    }

    let mut this_argument = String::new();
    if is_argument(line_str, char_nr, &mut this_argument) {
        log_dbg!(COMPL, "Complete for argument: {}", this_argument);

        let mut is_kfunc = false;
        if probe.starts_with("kprobe:")
            || probe.starts_with("kretprobe:")
            || probe.starts_with("kfunc:")
            || probe.starts_with("kretfunc:")
        {
            is_kfunc = true;
        }

        let mut btf_probe_args = None;
        if is_kfunc {
            let kfunc = kprobe_to_kfunc(&probe);
            btf_probe_args = find_kfunc_args_by_btf(&kfunc);
        }

        let mut args_as_string = String::new();
        let mut probe_args_iter: Lines = "".lines();
        let probe_args;

        if this_argument.ends_with("args.") && !is_kfunc {
            probe_args = find_probe_args_by_command(&probe);
            probe_args_iter = probe_args.lines();
            // On first line of probe args is kfunc module and name
            probe_args_iter.next();
        } else if let Some((module, resolved_btf)) = btf_probe_args {
            // Complete args. i.e. kfunc:xe:__fini_dbm { printf("%s\n", str(args.drm->driver->name)) }
            let arg_btf = argument_next_item(module, resolved_btf, &this_argument);
            let args = children_to_vec_str(&arg_btf);

            args_as_string.push_str(&args.join("\n"));
            probe_args_iter = args_as_string.lines();

            log_dbg!(COMPL, "Found arguments using btf:\n{}", args_as_string);
        }

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
    } else {
        // TODO preload btf module
        // TODO provide complete list
        let completion_printf = object! {
            "label": "printf",
            "kind" : 3,
            "detail" : "TODO",
            "documentation" : "need documentation",
        };
        let _ = items.push(completion_printf);

        let completion_str = object! {
            "label": "str",
            "kind" : 3,
            "detail" : "TODO",
            "documentation" : "need documentation",
        };
        let _ = items.push(completion_str);

        let completion_args = object! {
            "label": "args",
            "kind" : 5,
            "detail" : "TODO",
            "documentation" : "need documentation",
        };
        let _ = items.push(completion_args);
    }

    let data = object! {
        "result": {
            "isIncomplete": is_incomplete,
            "items": items,
        }
    };

    Some(data)
}

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

// TODO: convert to std::sync::OnceLock
static AVAILABE_TRACES: OnceCell<String> = OnceCell::new();

pub fn init_available_traces() {
    // TODO is OneCell thread safe
    if let Some(_traces) = AVAILABE_TRACES.get() {
        return;
    } else {
        if let Ok(output) = Command::new("sudo").arg("bpftrace").arg("-l").output() {
            if let Ok(traces) = String::from_utf8(output.stdout) {
                let _ = AVAILABE_TRACES.set(traces);
                //available_traces = AVAILABE_TRACES.get().unwrap();
                log_dbg!(COMPL, "Initalized available traces");
            }
        }
    }
}

fn encode_completion_for_line(prefix: &str, line_str: &str) -> Option<json::JsonValue> {
    if !line_str.trim().starts_with(&prefix) {
        return None;
    }

    log_dbg!(COMPL, "Check completion for prefix {}", prefix);

    // TOOD separate traces for each type i.e. kprobe, tracepoint
    // TODO add kretprobe, kretfunc support
    let available_traces;
    if let Some(traces) = AVAILABE_TRACES.get() {
        available_traces = traces;
    } else if let Ok(output) = Command::new("sudo").arg("bpftrace").arg("-l").output() {
        if let Ok(traces) = String::from_utf8(output.stdout) {
            let _ = AVAILABE_TRACES.set(traces);
            available_traces = AVAILABE_TRACES.get().unwrap();
            log_vdbg!(COMPL, "List of available traces: \n{available_traces}\n");
        } else {
            return None;
        }
    } else {
        return None;
    }

    let mut items = json::JsonValue::new_array();
    let mut is_incomplete = false;

    let max_count = 200;
    let mut count = max_count as i32;
    let mut duplicates: HashMap<String, u32> = HashMap::new();

    let mut line_tokens: Vec<&str> = line_str.split(":").collect();
    if line_str.trim().starts_with("kretfunc") || line_str.trim().starts_with("kretprobe") {
        line_tokens[0] = "kfunc";
    }
    let search_line = line_tokens.join(":");

    for trace_line in available_traces.lines() {
        if trace_line.trim().starts_with(search_line.trim()) {
            //TODO: save matched tokens ans skip duplicate lines here

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
                    3 // Function
                } else {
                    9 // Module
                };

                let mut item = object! {
                    "label": label,
                    "kind": kind,
                    // "detail": "TODO",
                    // "documentation": "need better documentation",
                };

                if trace_tokens[0] == "kfunc" && kind == 3 {
                    if let Some((_module, resolved_btf)) = find_kfunc_args_by_btf(&trace_line) {
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

fn encode_completion_for_empty_line(prefixes: &[&str]) -> json::JsonValue {
    let mut items = json::JsonValue::new_array();

    for prefix in prefixes.iter() {
        let prefix_item = object! {
            "label": prefix.to_string(),
            "kind": 8,
            "detail": "TODO",
            "documentation": "need better documentation"
        };
        let _ = items.push(prefix_item);
    }

    let data = object! {
        "result": {
            "isIncomplete": false,
            "items": items,
        }
    };

    data
}

pub fn encode_completion(content: json::JsonValue) -> json::JsonValue {
    let uri = &content["params"]["textDocument"]["uri"].to_string();

    let position = &content["params"]["position"];
    let line_nr = position["line"].as_usize().unwrap();
    let char_nr = position["character"].as_usize().unwrap();

    let text = get_text(&uri);
    let line_str = get_line(&uri, line_nr);

    log_dbg!(COMPL, "Complete for line: '{}'", line_str);

    if let Some(data) = encode_completion_for_action(&text, &line_str, line_nr, char_nr) {
        return data;
    }

    let prefixes = [
        "iter",
        "hardware",
        "tracepoint:",
        "kprobe",
        "kretprobe",
        "software:",
        "rawtracepoint",
        "kfunc",
        "kretfunc",
    ];
    for prefix in prefixes.iter() {
        if let Some(data) = encode_completion_for_line(prefix, &line_str) {
            return data;
        }
    }

    let data = encode_completion_for_empty_line(&prefixes[..]);
    data
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
        if found == "" && l < r {
            found = &line[l..r];
        }
    }

    found.to_string()
}

pub fn encode_hover(content: json::JsonValue) -> json::JsonValue {
    log_dbg!(HOVER, "Received hover with data {}", content);

    let position = &content["params"]["position"];
    let line_nr = position["line"].as_usize().unwrap();
    let char_nr = position["character"].as_usize().unwrap();

    let uri = &content["params"]["textDocument"]["uri"].to_string();

    let mut data = object! {};

    let option = DOCUMENTS_STATE.get(uri);
    if option.is_none() {
        return data;
    }

    let text_doc = option.unwrap();
    let text = &text_doc.text;

    log_vdbg!(HOVER, "This is the text:\n'{}'", text);

    let mut from_line: &str = text;
    for (i, line) in text.lines().enumerate() {
        if i == line_nr {
            from_line = line
        }
    }
    log_dbg!(HOVER, "Hover for line {}", from_line);

    let found = find_hover_str(
        from_line,
        char_nr,
        |c| c.is_whitespace(),
        |c| c.is_whitespace(),
    );
    log_dbg!(HOVER, "Found hover item: {}", found);

    if found.starts_with("kfunc:") {
        let args_by_btf = find_kfunc_args_by_btf(&found);
        if let Some((_module, resolved_btf)) = args_by_btf {
            data = object! {
                  "result": {
                      "contents": func_proto_str(&resolved_btf),
                  },
            };
        }
    } else if is_action_block(&text, line_nr, char_nr) {
        let probe = find_probe_for_action(&text, line_nr);
        let probe_args = find_probe_args_by_command(&probe);
        log_dbg!(HOVER, "Probe {} with args:\n{}", probe, probe_args);

        let lterm = |c: char| -> bool { c.is_whitespace() || c == '{' || c == '(' };
        let rterm =
            |c: char| -> bool { c.is_whitespace() || c == '}' || c == ')' || c == '.' || c == '-' };
        let mut found = find_hover_str(from_line, char_nr, lterm, rterm);
        log_dbg!(HOVER, "Hover found args string {}", found);

        if found == "args" {
            found.push('.');
        }
        let btf_probe_args = find_kfunc_args_by_btf(&probe);
        if let Some((module, resolved_btf)) = btf_probe_args {
            // log_dbg!(HOVER, "Resolved BTF {:?}", resolved_btf);
            let arg_btf = argument_next_item(module, resolved_btf, &found);
            // log_dbg!(HOVER, "ARG BTF {:?}", arg_btf);
            let mut hover = btf_item_to_str(&arg_btf);
            let args = children_to_vec_str(&arg_btf);

            hover.push_str("\n");
            hover.push_str("\n");
            hover.push_str(&args.join("\n"));

            log_dbg!(HOVER, "Hover:\n{:?}", hover);

            data = object! {
                  "result": {
                      "contents": hover,
                  },
            };
        }
    }

    data
}

#[cfg(test)]
mod tests {
    use super::*;
    fn compare_btf_and_cmd(s: &str) {
        let args_by_cmd = find_probe_args_by_command(s);
        let args_by_btf = find_kfunc_args_by_btf(s);

        let resolved_btf = if let Some((_module, resolved_btf)) = args_by_btf {
            resolved_btf
        } else {
            assert!(false);
            ResolvedBtfItem::default()
        };

        // for (i, c) in resolved_btf.children_vec.iter().enumerate() {
        //     println!("{i}: '{}'", btf_item_to_str(c).trim());
        // }

        let mut n = 0;
        for (i, arg) in args_by_cmd.lines().enumerate() {
            if i == 0 {
                assert!(arg == s);
                continue;
            }

            assert!(resolved_btf.children_vec.len() > i - 1);

            let btf_item = &resolved_btf.children_vec[i - 1];
            assert!(arg.trim() == btf_item_to_str(btf_item));
            n += 1;
        }
        assert!(resolved_btf.children_vec.len() == n);
    }

    #[test]
    fn test_find_probe_args() {
        compare_btf_and_cmd("kfunc:vmlinux:posixtimer_free_timer");
        compare_btf_and_cmd("kfunc:vmlinux:acpi_unregister_gsi");
        compare_btf_and_cmd("kfunc:vmlinux:acpi_register_gsi");
        compare_btf_and_cmd("kfunc:vmlinux:vfs_open");
    }
}
