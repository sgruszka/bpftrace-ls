use std::fs;

#[derive(Default, Debug)]
struct DocItem {
    label: String,
    first_line: String,
    variants: String,
    short_name: String,
    full_info: String,
}

fn add_json_obj(text: &mut String, doc_item: &DocItem, kind: usize) {
    let label = &doc_item.label;
    let detail = if doc_item.variants.is_empty() {
        &doc_item.label
    } else {
        &doc_item.variants
    };

    let mut docs = if doc_item.short_name.is_empty() {
        doc_item.first_line.to_string()
    } else {
        format!(
            "{}, short name {}",
            doc_item.first_line, &doc_item.short_name
        )
    };

    docs.push_str("\n\n");
    docs.push_str(&doc_item.full_info);

    let obj = format!(
        r##"
    let json_obj = object! {{
        "label": r#"{label}"#,
        "kind" : {kind},
        "detail": r#"{detail}"#,
        "documentation" : {{
            "kind": "markdown",
            "value": r#"{docs}"#,
        }},
    }};
    let _ = items.push(json_obj);
    "##
    );

    text.push_str(&obj);
}

fn gen_completion_probes(language_md: &str) -> String {
    let mut probes_section = false;
    let mut collected_probe = false;
    let mut in_full_docs = false;

    let mut after_label = false;
    let mut after_variants = false;
    let mut after_short_name = false;

    let mut doc_item = DocItem::default();

    let mut text = r#"
pub fn bpftrace_probe_providers(items: &mut json::JsonValue) {
    "#
    .to_string();

    for line in language_md.lines() {
        if line.trim().starts_with("## ") && probes_section {
            probes_section = false;
        }

        if line.trim().starts_with("## Probes") {
            probes_section = true;
        }
        if !probes_section {
            continue;
        }

        if line.trim().starts_with("### ") {
            if collected_probe {
                add_json_obj(&mut text, &doc_item, 8);
            }

            doc_item = DocItem::default();
            doc_item.label = line[4..].to_string();
            collected_probe = true;

            after_label = true;
            after_short_name = false;
            after_variants = false;
            in_full_docs = false;
            continue;
        }

        if line.trim().is_empty() && !in_full_docs {
            continue;
        }
        if after_label {
            doc_item.first_line = line.trim().to_string();
            after_label = false;
            continue;
        }

        if line.trim().starts_with("**variants**") {
            after_variants = true;
            continue;
        }

        if after_variants {
            if line.trim().starts_with("- ") || line.trim().starts_with("* ") {
                doc_item.variants.push_str(&line[3..line.len() - 1]);
                doc_item.variants.push('\n');
            } else if !line.trim().is_empty() {
                after_variants = false;
            }
        }

        if line.trim().starts_with("**short names**") || line.trim().starts_with("**short name**") {
            after_short_name = true;
            continue;
        }

        if after_short_name {
            if line.trim().starts_with("- ") || line.trim().starts_with("* ") {
                doc_item.short_name.push_str(&line[2..]);
            } else if !line.trim().is_empty() {
                after_short_name = false;
                in_full_docs = true;
            }
        }

        if in_full_docs {
            doc_item.full_info.push_str(line);
            doc_item.full_info.push('\n');
        }
    }

    text.push_str("}");

    text
}

fn gen_completion_stdlib(stdlib_md: &str) -> String {
    let mut doc_item = DocItem::default();
    let mut collected_item = false;
    let mut after_label = false;
    let mut in_full_docs = false;

    let mut text = r#"
pub fn bpftrace_stdlib_functions(items: &mut json::JsonValue) {
    "#
    .to_string();

    for line in stdlib_md.lines() {
        if line.trim().starts_with("### ") {
            if collected_item {
                add_json_obj(&mut text, &doc_item, 3);
            }

            doc_item = DocItem::default();
            doc_item.label = line[4..].to_string();
            collected_item = true;

            after_label = true;
            in_full_docs = false;
            continue;
        }

        if after_label {
            if line.trim().starts_with("- ") || line.trim().starts_with("* ") {
                doc_item.variants.push_str(&line[3..line.len() - 1]);
                doc_item.variants.push('\n');
            } else if !line.trim().is_empty() {
                after_label = false;
                in_full_docs = true;
            }
        }

        if in_full_docs {
            doc_item.full_info.push_str(line);
            doc_item.full_info.push('\n');
        }
    }

    text.push_str("}");

    text
}
