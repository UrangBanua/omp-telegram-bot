//! Utilitas untuk konversi Markdown ke Telegram HTML, chunking pesan, dan formatting tampilan.

/// Memecah pesan panjang menjadi beberapa chunk yang aman untuk batas limit Telegram (4096 karakter).
/// Mengutamakan pemotongan pada batas baris baru (`\n`) agar format teks tetap rapi.
pub fn chunk_message(text: &str, max_len: usize) -> Vec<String> {
    if text.is_empty() {
        return vec!["(Pesan kosong)".to_string()];
    }

    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }

        let slice = &remaining[..max_len];
        let split_pos = if let Some(last_newline) = slice.rfind('\n') {
            if last_newline > 0 {
                last_newline + 1
            } else {
                max_len
            }
        } else if let Some(last_space) = slice.rfind(' ') {
            if last_space > 0 {
                last_space + 1
            } else {
                max_len
            }
        } else {
            max_len
        };

        let (chunk, rest) = remaining.split_at(split_pos);
        chunks.push(chunk.to_string());
        remaining = rest;
    }

    chunks
}

/// Melakukan escape karakter khusus HTML untuk mode parse Telegram HTML.
pub fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Format indikator status eksekusi tool OMP dengan blockquote yang rapi.
pub fn format_tool_status(tool_name: &str, intent: Option<&str>) -> String {
    let mut out = format!("<blockquote>🛠️ <b>Tool:</b> <code>{}</code>", escape_html(tool_name));
    if let Some(i) = intent {
        if !i.is_empty() {
            out.push_str(&format!("\n<i>{}</i>", escape_html(i)));
        }
    }
    out.push_str("</blockquote>");
    out
}

/// Mengonversi teks Markdown standar (dari LLM / OMP) menjadi format HTML resmi yang didukung Telegram Bot API.
/// Menangani:
/// - Code blocks: ```lang ... ``` -> <pre><code class="language-lang">...</code></pre>
/// - Inline code: `code` -> <code>code</code>
/// - Headings: #, ##, ### -> <b>Header</b>
/// - Bold: **bold** atau __bold__ -> <b>bold</b>
/// - Italic: *italic* atau _italic_ -> <i>italic</i>
/// - Lists: * item atau - item -> • item
/// - Tables: | a | b | -> <pre>a | b\n...</pre> (Monospace table)
/// - Separator: --- -> ───────────────
/// - Auto-closing tags untuk streaming yang belum selesai.
pub fn markdown_to_telegram_html(input: &str) -> String {
    let mut output = String::with_capacity(input.len() + 64);
    let mut lines = input.lines().peekable();

    let mut in_code_block = false;
    let mut code_block_lang = String::new();
    let mut code_block_content = String::new();

    let mut in_table = false;
    let mut table_rows: Vec<String> = Vec::new();

    while let Some(line) = lines.next() {
        // 1. Penanganan Code Blocks (```lang ... ```)
        if line.trim_start().starts_with("```") {
            if !in_code_block {
                in_code_block = true;
                let lang = line.trim_start().trim_start_matches('`').trim();
                code_block_lang = lang.to_string();
                code_block_content.clear();
            } else {
                in_code_block = false;
                if !code_block_lang.is_empty() {
                    output.push_str(&format!(
                        "<pre><code class=\"language-{}\">{}</code></pre>\n",
                        escape_html(&code_block_lang),
                        escape_html(&code_block_content)
                    ));
                } else {
                    output.push_str(&format!(
                        "<pre>{}</pre>\n",
                        escape_html(&code_block_content)
                    ));
                }
            }
            continue;
        }

        if in_code_block {
            if !code_block_content.is_empty() {
                code_block_content.push('\n');
            }
            code_block_content.push_str(line);
            continue;
        }

        // 2. Penanganan Tabel Markdown (| col1 | col2 |)
        let trimmed_line = line.trim();
        if trimmed_line.starts_with('|') && trimmed_line.ends_with('|') {
            in_table = true;
            table_rows.push(line.to_string());
            continue;
        } else if in_table {
            in_table = false;
            format_table_into_output(&table_rows, &mut output);
            table_rows.clear();
        }

        // 3. Penanganan Separator horizontal line (--- / ***)
        if trimmed_line == "---" || trimmed_line == "***" || trimmed_line == "___" {
            output.push_str("───────────────\n");
            continue;
        }

        // 4. Penanganan Headings (# Header, ## Header, ### Header)
        if let Some(heading_text) = parse_heading(trimmed_line) {
            output.push_str(&format!("<b>{}</b>\n\n", parse_inline_formatting(&heading_text)));
            continue;
        }

        // 5. Penanganan Bullet Lists (* item, - item)
        if let Some(list_item) = parse_list_item(trimmed_line) {
            output.push_str(&format!("• {}\n", parse_inline_formatting(&list_item)));
            continue;
        }

        // 6. Penanganan Blockquote (> quote)
        if let Some(quote_text) = parse_blockquote(trimmed_line) {
            output.push_str(&format!("<blockquote>{}</blockquote>\n", parse_inline_formatting(&quote_text)));
            continue;
        }

        // 7. Paragraf teks biasa
        output.push_str(&parse_inline_formatting(line));
        output.push('\n');
    }

    // Flush sisa tabel jika berada di akhir teks
    if in_table && !table_rows.is_empty() {
        format_table_into_output(&table_rows, &mut output);
    }

    // Auto-close code block jika streaming masih berjalan di tengah code block
    if in_code_block {
        if !code_block_lang.is_empty() {
            output.push_str(&format!(
                "<pre><code class=\"language-{}\">{}</code></pre>",
                escape_html(&code_block_lang),
                escape_html(&code_block_content)
            ));
        } else {
            output.push_str(&format!(
                "<pre>{}</pre>",
                escape_html(&code_block_content)
            ));
        }
    }

    output.trim_end().to_string()
}

/// Helper untuk merender tabel markdown ke dalam blok monospace <pre> yang rapi.
fn format_table_into_output(rows: &[String], output: &mut String) {
    if rows.is_empty() {
        return;
    }

    output.push_str("<pre>\n");
    for row in rows {
        // Jangan render garis separator markdown |--|--| agar tabel lebih bersih
        let trimmed = row.trim();
        if trimmed.contains("---") || trimmed.contains(":---") || trimmed.contains("---:") {
            continue;
        }
        output.push_str(&escape_html(trimmed));
        output.push('\n');
    }
    output.push_str("</pre>\n");
}

/// Cek heading (# ... , ## ... , ### ...)
fn parse_heading(line: &str) -> Option<String> {
    if line.starts_with("### ") {
        Some(line[4..].trim().to_string())
    } else if line.starts_with("## ") {
        Some(line[3..].trim().to_string())
    } else if line.starts_with("# ") {
        Some(line[2..].trim().to_string())
    } else {
        None
    }
}

/// Cek list item (* item, - item)
fn parse_list_item(line: &str) -> Option<String> {
    if (line.starts_with("* ") || line.starts_with("- ")) && line.len() > 2 {
        Some(line[2..].trim().to_string())
    } else {
        None
    }
}

/// Cek blockquote (> text)
fn parse_blockquote(line: &str) -> Option<String> {
    if line.starts_with("> ") && line.len() > 2 {
        Some(line[2..].trim().to_string())
    } else if line.starts_with('>') && line.len() > 1 {
        Some(line[1..].trim().to_string())
    } else {
        None
    }
}

/// Mengonversi pemformatan inline (bold, italic, inline code).
fn parse_inline_formatting(line: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let len = chars.len();

    while i < len {
        // 1. Inline Code (`code`)
        if chars[i] == '`' {
            if let Some(close_idx) = find_next_char(&chars, i + 1, '`') {
                let code_content: String = chars[i + 1..close_idx].iter().collect();
                result.push_str(&format!("<code>{}</code>", escape_html(&code_content)));
                i = close_idx + 1;
                continue;
            }
        }

        // 2. Bold (**bold** atau __bold__)
        if (chars[i] == '*' && i + 1 < len && chars[i + 1] == '*')
            || (chars[i] == '_' && i + 1 < len && chars[i + 1] == '_')
        {
            let marker = chars[i];
            if let Some(close_idx) = find_next_double_char(&chars, i + 2, marker) {
                let bold_content: String = chars[i + 2..close_idx].iter().collect();
                result.push_str(&format!("<b>{}</b>", escape_html(&bold_content)));
                i = close_idx + 2;
                continue;
            }
        }

        // 3. Italic (*italic* atau _italic_)
        if (chars[i] == '*' || chars[i] == '_') && (i == 0 || chars[i - 1].is_whitespace()) {
            let marker = chars[i];
            if let Some(close_idx) = find_next_char(&chars, i + 1, marker) {
                // Pastikan bukan penutup double
                if close_idx + 1 == len || chars[close_idx + 1] != marker {
                    let italic_content: String = chars[i + 1..close_idx].iter().collect();
                    if !italic_content.trim().is_empty() {
                        result.push_str(&format!("<i>{}</i>", escape_html(&italic_content)));
                        i = close_idx + 1;
                        continue;
                    }
                }
            }
        }

        // Karakter biasa
        match chars[i] {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            c => result.push(c),
        }
        i += 1;
    }

    result
}

fn find_next_char(chars: &[char], start: usize, target: char) -> Option<usize> {
    for idx in start..chars.len() {
        if chars[idx] == target {
            return Some(idx);
        }
    }
    None
}

fn find_next_double_char(chars: &[char], start: usize, target: char) -> Option<usize> {
    let mut idx = start;
    while idx + 1 < chars.len() {
        if chars[idx] == target && chars[idx + 1] == target {
            return Some(idx);
        }
        idx += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_headings_and_bold() {
        let md = "### 1. Sistem Keamanan\n* **TTSR:** Pemantauan live stream.";
        let html = markdown_to_telegram_html(md);
        assert!(html.contains("<b>1. Sistem Keamanan</b>"));
        assert!(html.contains("• <b>TTSR:</b> Pemantauan live stream."));
    }

    #[test]
    fn test_code_block() {
        let md = "```rust\nfn main() {\n    println!(\"hello\");\n}\n```";
        let html = markdown_to_telegram_html(md);
        assert!(html.contains("<pre><code class=\"language-rust\">fn main() {\n    println!(&quot;hello&quot;);"));
    }

    #[test]
    fn test_table_conversion() {
        let md = "| Kategori | Command |\n| :--- | :--- |\n| Prompting | `prompt` |";
        let html = markdown_to_telegram_html(md);
        assert!(html.contains("<pre>"));
        assert!(html.contains("| Kategori | Command |"));
        assert!(html.contains("| Prompting | `prompt` |"));
    }
}
