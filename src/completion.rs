use json::{self, object};
use once_cell::sync::{Lazy, OnceCell};
use std::collections::HashMap;
use std::process::Command;
use std::sync::Mutex;

use crate::log_mod::{self, COMPL, VERBOSE_DEBUG};
use crate::{log_dbg, log_vdbg};
use crate::{State, JSON_RPC_VERSION};

static PROBES_ARGS_MAP: Lazy<Mutex<HashMap<String, String>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn get_text(state: &State, uri: &str) -> String {
    if let Some(text) = state.get(uri) {
        return text.to_string();
    };

    "".to_string()
}

fn get_line(state: &State, uri: &str, line_nr: usize) -> String {
    let mut from_line = String::new();
    if let Some(text) = state.get(uri) {
        for (i, line) in text.lines().enumerate() {
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

fn is_args(line_str: &str, char_nr: usize) -> bool {
    let mut res = false;
    if let Some(line_upto_char) = line_str.get(0..char_nr) {
        res = line_upto_char.ends_with("args.");
    }
    res
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

fn encode_completion_for_action(
    id: u64,
    text: &str,
    line_str: &str,
    line_nr: usize,
    char_nr: usize,
) -> Option<json::JsonValue> {
    if !is_action_block(text, line_nr, char_nr) {
        return None;
    }
    log_dbg!(COMPL, "Complete for action block");

    let mut is_incomplete = true;
    let mut items = json::JsonValue::new_array();

    let probe = find_probe_for_action(text, line_nr);

    let mut this_probe_args = "".to_string();
    if !probe.is_empty() {
        log_dbg!(COMPL, "Completing for probe: {}", probe);

        // Use kfunc for getting arguments, kprobe/kretprobe does not work
        let mut v: Vec<&str> = probe.split(":").collect();
        if v[0] == "kprobe" || v[0] == "kretprobe" {
            v[0] = "kfunc";
        }
        let probe = v[..].join(":").to_string();

        let mut probes_args_map = PROBES_ARGS_MAP.lock().unwrap();

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
                probes_args_map.insert(probe.clone(), probe_args.clone());
                this_probe_args = probe_args.clone();
            }
        }
    }

    if this_probe_args.is_empty() {
        log_dbg!(COMPL, "No arguments for probe {}", probe);
    } else {
        log_dbg!(
            COMPL,
            "Founds probe {} arguments: {}",
            probe,
            this_probe_args
        );
    }

    if is_args(line_str, char_nr) && !this_probe_args.is_empty() {
        let mut probe_args_iter = this_probe_args.lines();
        let _ = probe_args_iter.nth(0); // skip first line
        for arg in probe_args_iter {
            let tokens: Vec<&str> = arg.split(" ").collect();
            if tokens.len() <= 1 {
                continue;
            }
            let end = tokens.len() - 1;

            // let field = format!("args.{}", tokens[end]);
            let field = tokens[end];
            let field_type = tokens[0..end - 1].join(" ").to_string();
            let completion = object! {
                "label": field,
                "kind" : 5,
                "detail" : field_type.clone(),
                "documentation" : field_type,
            };
            let _ = items.push(completion);
        }
        is_incomplete = false;
    } else {
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
        "id" : id,
        "jasonrpc": JSON_RPC_VERSION,
        "result": {
            "isIncomplete": is_incomplete,
            "items": items,
        }
    };

    Some(data)
}

static AVAILABE_TRACES: OnceCell<String> = OnceCell::new();

fn encode_completion_for_line(id: u64, prefix: &str, line_str: &str) -> Option<json::JsonValue> {
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

    for trace_line in available_traces.lines() {
        if trace_line.trim().starts_with(line_str.trim()) {
            //TODO: save matched tokens ans skip duplicate lines here

            let trace_tokens: Vec<&str> = trace_line.split(":").collect();
            let line_tokens: Vec<&str> = line_str.split(":").collect();

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
                    5 // Field
                } else {
                    9 // Module
                };

                log_vdbg!(COMPL, "Adding complete item: {label}");

                let item = object! {
                    "label": label,
                    "kind": kind,
                    "detail": "TODO",
                    "documentation": "need better documentation",
                };
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
        "id" : id,
        "jasonrpc": JSON_RPC_VERSION,
        "result": {
            "isIncomplete": is_incomplete,
            "items": items,
        }
    };

    Some(data)
}

fn encode_completion_for_empty_line(id: u64) -> json::JsonValue {
    // TODO provide complete list, code this compactly
    let completion_iter = object! {
        "label": "iter:",
        "kind" : 8,
        "detail" : "TODO",
        "documentation" : "need documentation",
    };

    let completion_kfunc = object! {
        "label": "kfunc:",
        "kind" : 8,
        "detail" : "TODO",
        "documentation" : "need documentation",
    };

    let completion_kprobe = object! {
        "label": "kprobe:",
        "kind" : 8,
        "detail" : "TODO",
        "documentation" : "need documentation",
    };

    let completion_rawtracepoint = object! {
        "label": "rawtracepoint:",
        "kind" : 8,
        "detail" : "TODO",
        "documentation" : "need documentation",
    };

    let completion_software = object! {
        "label": "software:",
        "kind" : 8,
        "detail" : "TODO",
        "documentation" : "need documentation",
    };

    let completion_tracepoint = object! {
        "label": "tracepoint:",
        "kind" : 8,
        "detail" : "TODO",
        "documentation" : "need documentation",
    };

    let completion_hardware = object! {
        "label": "hardware:",
        "kind": 8,
        "detail": "TODO",
        "documentation": "need better documentation",
    };

    let data = object! {
        "id" : id,
        "jasonrpc": JSON_RPC_VERSION,
        "result": {
            "isIncomplete": false,
            "items": [
              completion_iter,
              completion_hardware,
              completion_tracepoint,
              completion_kprobe,
              completion_software,
              completion_rawtracepoint,
              completion_kfunc,
            ],
        }
    };

    data
}

pub fn encode_completion(state: &State, id: u64, content: json::JsonValue) -> String {
    let uri = &content["params"]["textDocument"]["uri"].to_string();

    let position = &content["params"]["position"];
    let line_nr = position["line"].as_usize().unwrap();
    let char_nr = position["character"].as_usize().unwrap();

    let text = get_text(state, &uri);
    let line_str = get_line(state, &uri, line_nr);

    log_dbg!(COMPL, "Complete for line: '{}'", line_str);

    if let Some(data) = encode_completion_for_action(id, &text, &line_str, line_nr, char_nr) {
        return data.dump();
    }
    // TODO handle kretprobe kretfunc
    let prefixes = [
        "iter",
        "hardware",
        "tracepoint:",
        "kprobe",
        "software:",
        "rawtracepoint",
        "kfunc",
    ];
    for prefix in prefixes.iter() {
        if let Some(data) = encode_completion_for_line(id, prefix, &line_str) {
            return data.dump();
        }
    }

    let data = encode_completion_for_empty_line(id);
    data.dump()
}

pub fn encode_completion_resolve(_state: &State, id: u64, content: json::JsonValue) -> String {
    log_vdbg!(COMPL, "Copletion resolve for: {}", content);

    let mut params = content["params"].clone();
    params["documentation"] = "Do this MARKUP".into();
    log_dbg!(COMPL, "documentation {}", params["documentation"]);

    let data = object! {
        "id" : id,
        "jasonrpc": JSON_RPC_VERSION,
        "result": params,
    };

    data.dump()
}
