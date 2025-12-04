use json::{self, object};
// use once_cell::sync::OnceCell;
use tree_sitter;
use tree_sitter_bpftrace;

use once_cell::sync::Lazy;
use std::{
    collections::HashMap,
    io::{self, Read, Write},
    process::Command,
    sync::{mpsc, Arc, RwLock},
    thread,
    time::{Duration, Instant},
};

const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const PKG_NAME: &str = env!("CARGO_PKG_NAME");

pub const JSON_RPC_VERSION: &str = "2.0";

// #[derive(Debug)]
pub struct TextDocument {
    text: String,
    version: u64,
    syntax_tree: Option<tree_sitter::Tree>,
}

pub struct DocumentsData {
    map: HashMap<String, Arc<TextDocument>>,
    parser: tree_sitter::Parser,
}

impl DocumentsData {
    fn new() -> Self {
        let map = HashMap::new();
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_bpftrace::LANGUAGE.into())
            .expect("Error loading bpftrace grammar"); //TODO
        Self { map, parser }
    }
}

pub struct DocumentsState(Lazy<RwLock<DocumentsData>>);

pub static DOCUMENTS_STATE: DocumentsState =
    DocumentsState(Lazy::new(|| RwLock::new(DocumentsData::new())));

impl DocumentsState {
    fn get(&self, uri: &str) -> Option<Arc<TextDocument>> {
        let read_guard = self.0.read().unwrap();
        read_guard.map.get(uri).cloned()
    }

    fn set(&self, uri: String, text: String, version: u64) {
        let mut write_guard = self.0.write().unwrap();

        let syntax_tree = write_guard.parser.parse(text.as_bytes(), None);

        let text_doc = Arc::new(TextDocument {
            text,
            version,
            syntax_tree,
        });
        write_guard.map.insert(uri, text_doc);
    }
}

pub mod btf_mod;
mod completion;
pub mod parser;

#[macro_use]
pub mod log_mod;

use log_mod::{DIAGN, NOTIF, PROTO};

#[derive(Debug)]
enum LspMessageType {
    Request,
    Response,
    Notification,
}

enum NotificationAction {
    None,
    Exit,
    SendDiagnostics(String),
}

struct LspClientMessage {
    msg_type: LspMessageType,
    id: u64,
    method: String,
    content: json::JsonValue,
    start_time: Instant,
}

struct DiagnosticsResutls {
    uri: String,
    version: u64,
    diagnostics: json::JsonValue,
}

struct DiagnosticsRequest {
    uri: String,
    version: u64,
}

enum MpscMessage {
    ClientMessage(LspClientMessage),
    Diagnostics(DiagnosticsResutls),
}

enum DiagnosticsCommand {
    DiagRequest(DiagnosticsRequest),
    Exit,
}

fn handle_notification(method: String, content: json::JsonValue) -> NotificationAction {
    match &method[..] {
        "textDocument/didOpen" => {
            let text_document = &content["params"]["textDocument"];
            let uri = text_document["uri"].to_string();
            let text = text_document["text"].to_string();
            let version = text_document["version"].as_u64().unwrap_or_default();

            DOCUMENTS_STATE.set(uri.clone(), text, version);

            log_dbg!(NOTIF, "Open: textDocument: {}", text_document);
            return NotificationAction::SendDiagnostics(uri);
        }
        "textDocument/didChange" => {
            let text_document = &content["params"]["textDocument"];
            let uri = text_document["uri"].to_string();
            let version = text_document["version"].as_u64().unwrap_or_default();

            let changes = &content["params"]["contentChanges"];
            let text = changes[0]["text"].to_string();

            // let text_doc = Arc::new(TextDocument { text, version });
            DOCUMENTS_STATE.set(uri.clone(), text, version);

            log_dbg!(NOTIF, "Change: textDocument: {}", text_document);
            return NotificationAction::SendDiagnostics(uri);
        }
        "textDocument/didSave" => {
            let text_document = &content["params"]["textDocument"];
            let uri = text_document["uri"].to_string();
            return NotificationAction::SendDiagnostics(uri);
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

fn encode_definition(content: json::JsonValue) -> json::JsonValue {
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
fn encode_code_action(content: json::JsonValue) -> json::JsonValue {
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

// Parse definitions errors:
// definitions.h:10:18: error: expected ';' at end of declaration list
fn handle_definitions_error(tokens: &Vec<&str>) -> json::JsonValue {
    let mut line_nr = tokens[1].parse::<usize>().unwrap();
    if line_nr > 1 {
        line_nr -= 1;
    }

    let end_char_nr = tokens[2].parse::<usize>().unwrap();
    let start_char_nr = if end_char_nr > 0 {
        end_char_nr - 1
    } else {
        end_char_nr
    };

    let msg = if tokens.len() > 4 {
        tokens[4..].join(":")
    } else {
        "".to_string()
    };

    let diag = object! {
        "range": { "start": { "line": line_nr, "character": start_char_nr}, "end": {"line": line_nr, "character": end_char_nr, }, },
        "severity": 1,
        // "source": "bpftrace -d",
        "message": format!("ERROR:{}", msg),
    };

    diag
}

fn do_diagnotics(text: &str) -> json::JsonValue {
    let mut diagnostics = json::JsonValue::new_array();

    let output = if let Ok(out) = Command::new("sudo")
        .arg("bpftrace")
        .arg("-d")
        .arg("-e")
        .arg(text)
        .output()
    {
        out
    } else {
        return diagnostics;
    };

    let output = if let Ok(out) = String::from_utf8(output.stderr) {
        out
    } else {
        return diagnostics;
    };

    log_vdbg!(DIAGN, "Output from bpftrace -d -e:\n{output}\n");

    for line in output.lines() {
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
        } else if tokens[0] == "definitions.h" && tokens.len() >= 3 {
            let diag = handle_definitions_error(&tokens);
            let _ = diagnostics.push(diag);
        }
    }

    diagnostics
}

fn send_diag_command(uri: String, diag_tx: &mpsc::Sender<DiagnosticsCommand>) {
    let Some(text_doc) = DOCUMENTS_STATE.get(&uri) else {
        log_dbg!(DIAGN, "No text document for {}", uri);
        return;
    };
    let version = text_doc.version;

    log_dbg!(
        DIAGN,
        "Send diagnostics command for uri {} version {}",
        uri,
        version,
    );

    let diag_req = DiagnosticsRequest { uri, version };

    let _ = diag_tx.send(DiagnosticsCommand::DiagRequest(diag_req));
}

fn send_diag_exit(diag_tx: &mpsc::Sender<DiagnosticsCommand>) {
    let _ = diag_tx.send(DiagnosticsCommand::Exit);
}

fn publish_diagnostics(diag_results: DiagnosticsResutls) -> Option<String> {
    let uri = &diag_results.uri;
    log_dbg!(
        DIAGN,
        "Got diagnostics results for uri: {} version {}",
        uri,
        diag_results.version
    );

    let text_doc = DOCUMENTS_STATE.get(uri)?;

    if text_doc.version != diag_results.version {
        log_dbg!(
            DIAGN,
            "Text document versions do not match: {} vs {}",
            text_doc.version,
            diag_results.version
        );
        return None;
    }

    log_vdbg!(DIAGN, "Text: \n{}\n", &text_doc.text);

    let params = object! {
        "uri": uri.to_string(),
        "version": text_doc.version,
        "diagnostics": diag_results.diagnostics,
    };

    let data = object! {
        "jasonrpc": JSON_RPC_VERSION,
        "method": "textDocument/publishDiagnostics",
        "params": params,
    };

    let resp = data.dump();
    Some(format!(
        "Content-Length: {}\r\n\r\n{}\r\n",
        resp.len(),
        resp
    ))
}

fn encode_message(id: u64, method: &str, content: json::JsonValue) -> String {
    let mut data = match &method[..] {
        "initialize" => encode_initalize_result(),
        "shutdown" => encode_shutdown(),
        "textDocument/hover" => completion::encode_hover(content),
        "textDocument/definition" => encode_definition(content),
        "textDocument/codeAction" => encode_code_action(content),
        "textDocument/completion" => completion::encode_completion(content),
        "completionItem/resolve" => completion::encode_completion_resolve(content),
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

fn decode_message(msg: String) -> (LspMessageType, u64, String, json::JsonValue) {
    // TODO remove unwrap() and handle errors
    let content = json::parse(&msg).unwrap();

    let method = &content["method"];
    //let client_info = &content["params"]["clientInfo"];
    //log_dbg!(PROTO, "client Info {}", client_info);

    let mut id = 0;
    if let Some(num) = content["id"].as_u64() {
        id = num;
    }

    let mut msg_type = LspMessageType::Notification;
    if id != 0 {
        if !content["result"].is_null() || !content["error"].is_null() {
            msg_type = LspMessageType::Response;
        } else {
            msg_type = LspMessageType::Request;
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

fn thread_input(mpsc_tx: mpsc::Sender<MpscMessage>) {
    let mut error_count = 0;

    loop {
        match recv_message() {
            Ok(msg) => {
                let start_time = Instant::now();
                let (msg_type, id, method, content) = decode_message(msg);

                let exit: bool = match &msg_type {
                    LspMessageType::Notification => {
                        if method == "exit" {
                            true
                        } else {
                            false
                        }
                    }
                    _ => false,
                };

                let lsp_client_msg = LspClientMessage {
                    msg_type,
                    id,
                    method,
                    content,
                    start_time,
                };

                let res = mpsc_tx.send(MpscMessage::ClientMessage(lsp_client_msg));
                if let Err(err) = res {
                    log_err!("MPSC send error {}", err);
                    break;
                }

                if exit {
                    log_dbg!(PROTO, "Received exit notification");
                    break;
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
}

fn thread_diagnostics(
    mpsc_tx: mpsc::Sender<MpscMessage>,
    diag_rx: mpsc::Receiver<DiagnosticsCommand>,
) {
    loop {
        match diag_rx.recv() {
            Ok(diag_msg) => match diag_msg {
                DiagnosticsCommand::DiagRequest(diag_req) => {
                    let uri = diag_req.uri;
                    let version = diag_req.version;

                    // Skip diagnostics if file is edited
                    // TODO: check if 300ms is more or less good heuristics
                    thread::sleep(Duration::from_millis(300));

                    let option = DOCUMENTS_STATE.get(&uri);
                    if option.is_none() {
                        log_err!("Can not find document for {uri}");
                        continue;
                    }
                    let text_doc = option.unwrap();

                    if text_doc.version != diag_req.version {
                        log_dbg!(
                            DIAGN,
                            "Skip diagnostics for old version {}, version is {}",
                            diag_req.version,
                            text_doc.version
                        );
                        continue;
                    }

                    let diagnostics = do_diagnotics(&text_doc.text);

                    let diag_msg = DiagnosticsResutls {
                        uri,
                        version,
                        diagnostics,
                    };
                    let _res = mpsc_tx.send(MpscMessage::Diagnostics(diag_msg));
                }
                DiagnosticsCommand::Exit => {
                    log_dbg!(DIAGN, "Exit diagnostics thread");
                    break;
                }
            },
            Err(e) => {
                log_err!("Diagnostics MPSC error {}", e);
                break;
            }
        }
    }
}

fn handle_client_msg(
    lsp_client_msg: LspClientMessage,
    diag_tx: &mpsc::Sender<DiagnosticsCommand>,
) -> bool {
    let LspClientMessage {
        msg_type,
        id,
        method,
        content,
        start_time,
    } = lsp_client_msg;

    match msg_type {
        LspMessageType::Request => {
            let s = encode_message(id, &method, content);
            let time_diff = start_time.elapsed();
            log_dbg!(PROTO, "Response time {:?}", time_diff);
            log_vdbg!(PROTO, "Answer:\n{}", s);
            send_message(s);
            // TOOD response with InvalidRequest after shutdown
            // if method == "shutdown" {
            //     break;
            // }
            //
        }
        LspMessageType::Response => (),
        LspMessageType::Notification => {
            let notif_action = handle_notification(method, content);
            // TODO consider moving this to handle notification
            match notif_action {
                NotificationAction::SendDiagnostics(uri) => {
                    send_diag_command(uri, diag_tx);
                }
                NotificationAction::Exit => {
                    log_dbg!(PROTO, "Exiting");
                    send_diag_exit(diag_tx);
                    return true;
                }
                NotificationAction::None => {}
            }
        }
    }

    false /* No exit */
}

fn main() {
    if let Err(e) = log_mod::create_logger("log.txt") {
        println!("Failed to create logger, error {e}");
    }

    log_dbg!(PROTO, "{} {} started", PKG_NAME, PKG_VERSION);

    let _completion_init = thread::spawn(completion::init_available_traces);

    let (mpsc_tx, mpsc_rx) = mpsc::channel::<MpscMessage>();
    let diag_mpsc_tx = mpsc_tx.clone();
    thread::spawn(move || thread_input(mpsc_tx));

    let (diag_tx, diag_rx) = mpsc::channel::<DiagnosticsCommand>();
    thread::spawn(move || thread_diagnostics(diag_mpsc_tx, diag_rx));

    loop {
        match mpsc_rx.recv() {
            Ok(mpsc_msg) => {
                match mpsc_msg {
                    MpscMessage::ClientMessage(client_msg) => {
                        let do_exit = handle_client_msg(client_msg, &diag_tx);
                        if do_exit {
                            break;
                        }
                    }
                    MpscMessage::Diagnostics(diag_results) => {
                        if let Some(s) = publish_diagnostics(diag_results) {
                            log_dbg!(DIAGN, "Send diagnostics: {}", s);
                            send_message(s);
                        }
                    }
                };
            }
            Err(err) => {
                log_err!("Subthread error {}", err);
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_decode_message() {
        let msg = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{"general":{"positionEncodings":["utf-16"]}}}}"#;
        let (msg_type, id, method, _content) = decode_message(msg.to_string());
        match msg_type {
            LspMessageType::Request => assert!(true),
            _ => assert!(false),
        }
        assert!(id == 1);
        assert!(method == "initialize");
    }
}
