use json::{self, object};
// use once_cell::sync::OnceCell;
use std::{
    collections::HashMap,
    io::{self, Read, Write},
    process::Command,
    thread,
    time::Instant,
};

const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const PKG_NAME: &str = env!("CARGO_PKG_NAME");

pub const JSON_RPC_VERSION: &str = "2.0";
pub type State = HashMap<String, String>;

mod completion;

#[macro_use]
pub mod log_mod;
pub mod btf_mod;

use log_mod::{DIAGN, NOTIF, PROTO, VERBOSE_DEBUG};

#[derive(Debug)]
enum MessageType {
    Request,
    Response,
    Notification,
}

enum NotificationAction {
    None,
    Exit,
    SendDiagnostics,
}

fn handle_notification(
    state: &mut State,
    method: String,
    content: json::JsonValue,
) -> NotificationAction {
    match &method[..] {
        "textDocument/didOpen" => {
            let text_document = &content["params"]["textDocument"];
            let uri = text_document["uri"].to_string();
            let text = text_document["text"].to_string();

            state.insert(uri, text);

            log_dbg!(NOTIF, "Open: textDocument: {}", text_document);
            return NotificationAction::SendDiagnostics;
        }
        "textDocument/didChange" => {
            let text_document = &content["params"]["textDocument"];
            let uri = text_document["uri"].to_string();

            let changes = &content["params"]["contentChanges"];
            let text = changes[0]["text"].to_string();

            state.insert(uri, text.to_string());

            log_dbg!(NOTIF, "Change: textDocument: {}", text_document);
            return NotificationAction::SendDiagnostics;
        }
        "textDocument/didSave" => {
            return NotificationAction::SendDiagnostics;
        }
        "exit" => {
            return NotificationAction::Exit;
        }
        _ => log_dbg!(
            NOTIF,
            "Unhandled {} notification with content {}",
            method,
            content
        ),
    }

    NotificationAction::None
}

fn encode_initalize_result() -> json::JsonValue {
    let capabilities = object! {
        "textDocumentSync": 1,
        "hoverProvider": true,
        "definitionProvider": true,
        // "codeActionProvider": true,
        "completionProvider": {
            "triggerCharacters": [":", ".", ">"],
            // TODO "resolveProvider": true,

        },
    };

    let server_info = object! {
        name: PKG_NAME,
        version: PKG_VERSION,
    };

    let data = object! {
        "result": {
            "capabilities": capabilities,
            "serverInfo": server_info,
        },
    };

    data
}

fn encode_shutdown() -> json::JsonValue {
    let data = object! {
        "result": null,
    };

    data
}

fn encode_definition(_state: &State, content: json::JsonValue) -> json::JsonValue {
    log_err!("Received definition with data {}", content);
    let uri = &content["params"]["textDocument"]["uri"].to_string();

    let position = &content["params"]["position"];
    let line_nr = position["line"].as_usize().unwrap();
    let char_nr = position["character"].as_usize().unwrap();

    let new_line_nr = if line_nr > 0 { line_nr - 1 } else { line_nr };

    let data = object! {
        "result": {
            "uri": uri.to_string(),
            "range": {
                "start": { "line": new_line_nr, "character": char_nr + 8,},
                "end": {"line": new_line_nr, "character": char_nr + 10, },
            },
        },
    };

    data
}

// TODO implement correct codeAction and enable codeActionProvider
fn encode_code_action(_state: &State, content: json::JsonValue) -> json::JsonValue {
    log_err!("Received codeAction with data {}", content);
    let uri = &content["params"]["textDocument"]["uri"].to_string();

    let range = &content["params"]["range"];
    let start = &range["start"];
    let end = &range["end"];

    let (start_line, _start_char) = (
        start["line"].as_u64().unwrap(),
        start["character"].as_u64().unwrap(),
    );

    let (end_line, _end_char) = (
        end["line"].as_u64().unwrap(),
        end["character"].as_u64().unwrap(),
    );

    let text_edit = object! {
        "range": {
            "start": { "line": start_line, "character": 0,},
            "end": { "line": end_line, "character": 0, }
        },

         "newText": format!("{}: ", start_line),
    };

    let code_action = object! {
        "title": "Add line number at the beginning\r\n",
        "edit": {
            "changes": {
                [uri]: [text_edit],
            },
        }
    };

    let data = object! {
        "result": [code_action],
    };

    data
}

// Parse single line errors:
// stdin:6:60-69: ERROR: str() expects an integer or a pointer type as first argument (struct _tracepoint_syscalls_sys_exit_bpf provided)
fn handle_single_line_error(mut line_nr: usize, tokens: &Vec<&str>) -> json::JsonValue {
    if line_nr > 1 {
        line_nr -= 1;
    }

    let chars: Vec<&str> = tokens[2].split("-").collect();
    let start: usize = chars[0].parse().unwrap();
    let end: usize = chars[1].parse().unwrap();

    let to_severity = |e: &str| -> u32 {
        match e.trim() {
            "ERROR" => 1,
            _ => 2,
        }
    };

    let tail = if tokens.len() > 4 {
        tokens[4..].join(":")
    } else {
        "".to_string()
    };

    let diag = object! {
        "range": { "start": { "line": line_nr, "character": start}, "end": {"line": line_nr, "character": end, }, },
        "severity": to_severity(tokens[3]),
        // "source": "bpftrace -d",
        "message": format!("{}:{}", tokens[3], tail),
    };

    diag
}

// Parse errors with lines range like this:
// stdin:2-4: ERROR: Invalid probe type: kkprobe
fn handle_multi_line_error(tokens: &Vec<&str>) -> json::JsonValue {
    let start_end: Vec<&str> = tokens[1].split("-").collect();
    // position error on last line
    let mut line_nr: usize = start_end[1].parse().unwrap();
    if line_nr > 1 {
        line_nr -= 1;
    }

    let to_severity = |e: &str| -> u32 {
        match e.trim() {
            "ERROR" => 1,
            _ => 2,
        }
    };

    let tail = if tokens.len() > 3 {
        tokens[3..].join(":")
    } else {
        "".to_string()
    };

    let diag = object! {
        "range": { "start": { "line": line_nr, "character": 0}, "end": {"line": line_nr, "character": 0, }, },
        "severity": to_severity(tokens[2]),
        // "source": "bpftrace -d",
        "message": format!("{}:{}", tokens[2], tail),
    };

    diag
}

fn publish_diagnostics(state: &State) -> String {
    let entry;

    // TOOD: need support for all edited files
    match state.into_iter().nth(0) {
        Some(x) => entry = x,
        None => return "".to_string(),
    }

    let (uri, text) = entry;
    log_dbg!(DIAGN, "Check diagnostics for uri: {}", uri);
    log_vdbg!(DIAGN, "Text: \n{}\n", text);

    let mut diagnostics = json::JsonValue::new_array();

    if let Ok(output) = Command::new("sudo")
        .arg("bpftrace")
        .arg("-d")
        .arg("-e")
        .arg(text)
        .output()
    {
        let s = String::from_utf8(output.stderr).expect("Need some text"); // TODO remove expect
        log_vdbg!(DIAGN, "Output from bpftrace -d -e:\n{s}\n");

        for line in s.lines() {
            let tokens: Vec<&str> = line.split(":").collect();
            log_dbg!(DIAGN, "Parsing error line: {}", line);
            if tokens[0] == "stdin" && tokens.len() >= 3 {
                if let Ok(line_nr) = tokens[1].parse::<usize>() {
                    let diag = handle_single_line_error(line_nr, &tokens);
                    let _ = diagnostics.push(diag);
                } else {
                    let diag = handle_multi_line_error(&tokens);
                    let _ = diagnostics.push(diag);
                }
            }
        }
    }

    let params = object! {
        "uri": uri.to_string(),
        "diagnostics": diagnostics,
    };

    let data = object! {
        "jasonrpc": JSON_RPC_VERSION,
        "method": "textDocument/publishDiagnostics",
        "params": params,
    };

    let resp = data.dump();
    format!("Content-Length: {}\r\n\r\n{}\r\n", resp.len(), resp)
}

fn encode_message(state: &State, id: u64, method: &str, content: json::JsonValue) -> String {
    let mut data = match &method[..] {
        "initialize" => encode_initalize_result(),
        "shutdown" => encode_shutdown(),
        "textDocument/hover" => completion::encode_hover(state, content),
        "textDocument/definition" => encode_definition(state, content),
        "textDocument/codeAction" => encode_code_action(state, content),
        "textDocument/completion" => completion::encode_completion(state, content),
        "completionItem/resolve" => completion::encode_completion_resolve(state, content),
        unhandled_method => {
            log_dbg!(PROTO, "No handler for method: {}", unhandled_method);
            object! {}
        }
    };

    data["id"] = id.into();
    data["jasonrpc"] = JSON_RPC_VERSION.into();

    let resp = data.dump();
    format!("Content-Length: {}\r\n\r\n{}\n", resp.len(), resp)
}

fn decode_message(msg: String) -> (MessageType, u64, String, json::JsonValue) {
    // TODO remove unwrap() and handle errors
    let content = json::parse(&msg).unwrap();

    let method = &content["method"];
    //let client_info = &content["params"]["clientInfo"];
    //log_dbg!(PROTO, "client Info {}", client_info);

    let mut id = 0;
    if let Some(num) = content["id"].as_u64() {
        id = num;
    }

    let mut msg_type = MessageType::Notification;
    if id != 0 {
        if !content["result"].is_null() || !content["error"].is_null() {
            msg_type = MessageType::Response;
        } else {
            msg_type = MessageType::Request;
        }
    }

    log_dbg!(PROTO, "Received {} {:?} with id {}", method, msg_type, id);

    (msg_type, id, method.to_string(), content)
}

fn recv_message() -> Result<String, i32> {
    log_vdbg!(PROTO, "Wait for the next message");
    let mut header = String::new();
    io::stdin()
        .read_line(&mut header)
        .expect("Failed to read header");

    let start_idx = "Content-Length: ".len();
    if header.len() < start_idx {
        log_err!("Not enough input, got header: '{}'\n", header);
        return Err(-1);
    }

    let parse_result = header[start_idx..].trim().parse::<usize>();
    let len: usize;
    match parse_result {
        Ok(val) => len = val,
        Err(_) => {
            log_err!("Failed to parse length");
            return Err(-2);
        }
    }
    // let mut buf: Vec<u8> = Vec::with_capacity(len);
    let mut buf: Vec<u8> = vec![0; len];
    let mut n_read = 0;
    let mut idx = 0;
    let mut count = 0;

    // Skip empty line
    io::stdin()
        .read_line(&mut header)
        .expect("Failed to eat empty line");

    loop {
        match io::stdin().read(&mut buf[idx..]) {
            Ok(n) => {
                log_dbg!(PROTO, "Read n bytes {} buf.len() {}", n, buf.len());
                n_read += n;
                count += 0;
                if count > 9 {
                    break;
                }
            }
            Err(e) => log_err!("Read error {}", e),
        }

        // TODO: handle partial messages
        if n_read < len {
            idx = n_read;
            continue;
        }

        break;
    }

    match String::from_utf8(buf) {
        Ok(s) => {
            log_vdbg!(PROTO, "Read message: '{}'", s);
            return Ok(s);
        }
        Err(e) => log_err!("Failed to convert to string: {}", e),
    }

    return Err(-1);
}

fn send_message(s: String) {
    let res = io::stdout().write(s.as_bytes());
    match res {
        Ok(n) => log_dbg!(PROTO, "Send {} bytes out of {}", n, s.len()),
        Err(e) => log_err!("Failed to write to stdout with error {}", e),
    }
}

fn main() {
    if let Err(e) = log_mod::create_logger("log.txt") {
        println!("Failed to create logger, error {e}");
    }

    log_dbg!(PROTO, "{} {} started", PKG_NAME, PKG_VERSION);

    let mut error_count = 0;
    let mut state: HashMap<String, String> = HashMap::new();

    let completion_init = thread::spawn(completion::init_available_traces);

    // TODO handle shutdown
    loop {
        match recv_message() {
            Ok(msg) => {
                let start_time = Instant::now();
                let (msg_type, id, method, content) = decode_message(msg);

                match msg_type {
                    MessageType::Request => {
                        let s = encode_message(&state, id, &method, content);
                        let time_diff = start_time.elapsed();
                        log_dbg!(PROTO, "Response time {:?}", time_diff);
                        log_vdbg!(PROTO, "Answer:\n{}", s);
                        send_message(s);
                        // TOOD response with InvalidRequest after shutdown
                        // if method == "shutdown" {
                        //     break;
                        // }
                        //
                        // TODO make this work
                        // if method == "initialize" && !completion_init_done {
                        //     completion_init_done = true;
                        //     completion_init.join().unwrap();
                        // }
                    }
                    MessageType::Response => (),
                    MessageType::Notification => {
                        let notif_action = handle_notification(&mut state, method, content);
                        match notif_action {
                            NotificationAction::SendDiagnostics => {
                                let s = publish_diagnostics(&state);
                                log_dbg!(DIAGN, "Send diagnostics: {}", s);
                                if s.len() > 0 {
                                    send_message(s);
                                }
                            }
                            NotificationAction::Exit => {
                                log_dbg!(PROTO, "Exiting");
                                break;
                            }
                            NotificationAction::None => {}
                        }
                    }
                }
            }
            Err(e) => {
                log_err!("Read error {}", e);
                error_count += 1;
                if error_count >= 10 {
                    log_err!("To many read errors, exiting ...");
                    break;
                }
            }
        }
    }
    let _ = completion_init.join();
}
