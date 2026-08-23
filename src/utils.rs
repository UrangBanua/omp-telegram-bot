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

/// Format indikator status eksekusi tool/terminal OMP dengan format blockquote yang rapi.
pub fn format_tool_status(tool_name: &str, intent: Option<&str>) -> String {
    let mut out = format!("<blockquote>⚡ <b>Eksekusi Tool:</b> <code>{}</code>", escape_html(tool_name));
    if let Some(i) = intent {
        if !i.is_empty() {
            out.push_str(&format!("\n<i>{}</i>", escape_html(i)));
        }
    }
    out.push_str("</blockquote>");
    out
}

/// Mengonversi teks Markdown standar (dari LLM / OMP) menjadi format HTML resmi Telegram.
/// - Paragraf & Text: Format paragraf biasa yang bersih (bukan block)
/// - Headings (#, ##, ###): Format bold terpisah `<b>...</b>`
/// - Bullet List (*, -): Format bullet Unicode `• ...`
/// - Tabel Markdown (| a | b |): Dikonversi menjadi structured list card yang rapi dan mudah dibaca di mobile/desktop
/// - Code Blocks (```lang ... ```): Dikonversi menjadi `<pre><code class="language-lang">...</code></pre>`
pub fn markdown_to_telegram_html(input: &str) -> String {
    let mut output = String::with_capacity(input.len() + 128);
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;
    let total_lines = lines.len();

    let mut in_code_block = false;
    let mut code_block_lang = String::new();
    let mut code_block_content = String::new();

    while i < total_lines {
        let line = lines[i];
        let trimmed = line.trim();

        // 1. Penanganan Code Blocks (```lang ... ```)
        if trimmed.starts_with("```") {
            if !in_code_block {
                in_code_block = true;
                let lang = trimmed.trim_start_matches('`').trim();
                code_block_lang = lang.to_string();
                code_block_content.clear();
            } else {
                in_code_block = false;
                if !code_block_lang.is_empty() {
                    output.push_str(&format!(
                        "<pre><code class=\"language-{}\">{}</code></pre>\n\n",
                        escape_html(&code_block_lang),
                        escape_html(&code_block_content)
                    ));
                } else {
                    output.push_str(&format!(
                        "<pre>{}</pre>\n\n",
                        escape_html(&code_block_content)
                    ));
                }
            }
            i += 1;
            continue;
        }

        if in_code_block {
            if !code_block_content.is_empty() {
                code_block_content.push('\n');
            }
            code_block_content.push_str(line);
            i += 1;
            continue;
        }

        // 2. Penanganan Tabel Markdown (| col1 | col2 |)
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            let mut table_rows = Vec::new();
            while i < total_lines {
                let cur = lines[i].trim();
                if cur.starts_with('|') && cur.ends_with('|') {
                    table_rows.push(cur);
                    i += 1;
                } else {
                    break;
                }
            }
            format_table_as_clean_cards(&table_rows, &mut output);
            continue;
        }

        // 3. Penanganan Separator Horizontal Line (--- / ***)
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            output.push_str("────────────────────\n\n");
            i += 1;
            continue;
        }

        // 4. Penanganan Headings (# Header, ## Header, ### Header)
        if let Some(heading_text) = parse_heading(trimmed) {
            output.push_str(&format!("<b>{}</b>\n\n", parse_inline_formatting(&heading_text)));
            i += 1;
            continue;
        }

        // 5. Penanganan Bullet Lists (* item, - item)
        if let Some(list_item) = parse_list_item(trimmed) {
            output.push_str(&format!("• {}\n", parse_inline_formatting(&list_item)));
            i += 1;
            continue;
        }

        // 6. Penanganan Numbered Lists (1. item, 2. item)
        if is_numbered_list(trimmed) {
            output.push_str(&format!("{}\n", parse_inline_formatting(trimmed)));
            i += 1;
            continue;
        }

        // 7. Penanganan Blockquote (> quote)
        if let Some(quote_text) = parse_blockquote(trimmed) {
            output.push_str(&format!("<blockquote>{}</blockquote>\n\n", parse_inline_formatting(&quote_text)));
            i += 1;
            continue;
        }

        // 8. Teks Paragraf Biasa
        if !trimmed.is_empty() {
            output.push_str(&parse_inline_formatting(line));
            output.push('\n');
        } else {
            output.push('\n');
        }

        i += 1;
    }

    // Auto-close code block jika streaming terhenti di tengah code block
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

    // Rapikan multiple blank lines yang berlebihan
    clean_excessive_newlines(&output)
}

/// Mengonversi tabel Markdown menjadi format card terstruktur yang sangat rapi dan enak dibaca di Telegram.
fn format_table_as_clean_cards(rows: &[&str], output: &mut String) {
    if rows.is_empty() {
        return;
    }

    let mut headers: Vec<String> = Vec::new();
    let mut data_rows: Vec<Vec<String>> = Vec::new();

    for (idx, row) in rows.iter().enumerate() {
        let cells: Vec<String> = row
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim().to_string())
            .collect();

        // Baris 1: Header
        if idx == 0 {
            headers = cells;
            continue;
        }

        // Baris 2: Separator |--|--|
        if cells.iter().all(|c| c.contains("---") || c.is_empty()) {
            continue;
        }

        // Baris Data
        data_rows.push(cells);
    }

    if data_rows.is_empty() {
        return;
    }

    output.push('\n');
    for row in data_rows {
        output.push_str("📋 ");
        for (col_idx, val) in row.iter().enumerate() {
            if val.is_empty() {
                continue;
            }

            let header_label = headers.get(col_idx).cloned().unwrap_or_default();
            let parsed_val = parse_inline_formatting(val);

            if col_idx == 0 {
                output.push_str(&format!("<b>{}</b>", parsed_val));
            } else if col_idx == 1 && row.len() > 1 && !header_label.is_empty() {
                output.push_str(&format!(" (<code>{}</code>)", parsed_val));
            } else {
                if !header_label.is_empty() {
                    output.push_str(&format!("\n   • <i>{}:</i> {}", escape_html(&header_label), parsed_val));
                } else {
                    output.push_str(&format!("\n   • {}", parsed_val));
                }
            }
        }
        output.push_str("\n\n");
    }
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

/// Cek numbered list (1. item, 2. item)
fn is_numbered_list(line: &str) -> bool {
    let mut parts = line.splitn(2, '.');
    if let (Some(num), Some(rest)) = (parts.next(), parts.next()) {
        if num.chars().all(|c| c.is_ascii_digit()) && rest.starts_with(' ') {
            return true;
        }
    }
    false
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

/// Mengonversi pemformatan inline (bold, italic, inline code) dengan dukungan nested.
pub fn parse_inline_formatting(input: &str) -> String {
    let mut result = String::with_capacity(input.len() + 32);
    let chars: Vec<char> = input.chars().collect();
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
                let inner_raw: String = chars[i + 2..close_idx].iter().collect();
                let inner_formatted = parse_inline_italic(&inner_raw);
                result.push_str(&format!("<b>{}</b>", inner_formatted));
                i = close_idx + 2;
                continue;
            }
        }

        // 3. Italic (*italic* atau _italic_)
        if (chars[i] == '*' || chars[i] == '_') && (i == 0 || chars[i - 1].is_whitespace() || chars[i - 1] == '(') {
            let marker = chars[i];
            if let Some(close_idx) = find_next_char(&chars, i + 1, marker) {
                // Pastikan bukan double
                if close_idx + 1 == len || chars[close_idx + 1] != marker {
                    let inner_raw: String = chars[i + 1..close_idx].iter().collect();
                    if !inner_raw.trim().is_empty() {
                        result.push_str(&format!("<i>{}</i>", escape_html(&inner_raw)));
                        i = close_idx + 1;
                        continue;
                    }
                }
            }
        }

        // Karakter biasa dengan HTML entity escaping
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

/// Helper khusus untuk memproses italic di dalam bold (misal **TTSR (*Time-Traveling*):**)
fn parse_inline_italic(input: &str) -> String {
    let mut result = String::with_capacity(input.len() + 16);
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let len = chars.len();

    while i < len {
        if chars[i] == '`' {
            if let Some(close_idx) = find_next_char(&chars, i + 1, '`') {
                let code_content: String = chars[i + 1..close_idx].iter().collect();
                result.push_str(&format!("<code>{}</code>", escape_html(&code_content)));
                i = close_idx + 1;
                continue;
            }
        }

        if chars[i] == '*' || chars[i] == '_' {
            let marker = chars[i];
            if let Some(close_idx) = find_next_char(&chars, i + 1, marker) {
                let inner_raw: String = chars[i + 1..close_idx].iter().collect();
                result.push_str(&format!("<i>{}</i>", escape_html(&inner_raw)));
                i = close_idx + 1;
                continue;
            }
        }

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

fn clean_excessive_newlines(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut consecutive_newlines = 0;

    for c in input.chars() {
        if c == '\n' {
            consecutive_newlines += 1;
            if consecutive_newlines <= 2 {
                out.push(c);
            }
        } else {
            consecutive_newlines = 0;
            out.push(c);
        }
    }

    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_headings_and_nested_bold_italic() {
        let md = "### 1. Sistem Keamanan\n* **TTSR (*Time-Traveling*):** Pemantauan live stream.";
        let html = markdown_to_telegram_html(md);
        assert!(html.contains("<b>1. Sistem Keamanan</b>"));
        assert!(html.contains("• <b>TTSR (<i>Time-Traveling</i>):</b> Pemantauan live stream."));
    }

    #[test]
    fn test_code_block() {
        let md = "```rust\nfn main() {\n    println!(\"hello\");\n}\n```";
        let html = markdown_to_telegram_html(md);
        assert!(html.contains("<pre><code class=\"language-rust\">fn main() {\n    println!(&quot;hello&quot;);"));
    }

    #[test]
    fn test_table_conversion() {
        let md = "| Kategori | Command | Deskripsi |\n| :--- | :--- | :--- |\n| Prompting | `prompt` | Kirim pesan prompt |";
        let html = markdown_to_telegram_html(md);
        assert!(html.contains("📋 <b>Prompting</b>"));
        assert!(html.contains("<code>prompt</code>"));
        assert!(html.contains("Kirim pesan prompt"));
    }
}
