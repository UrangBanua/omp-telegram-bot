//! Utilitas untuk konversi Markdown ke Telegram HTML, chunking pesan, dan formatting tampilan.

/// Memecah pesan panjang menjadi beberapa chunk teks biasa yang aman untuk limit Telegram (4096 karakter).
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

/// Memecah dokumen Markdown menjadi sub-messages HTML yang 100% valid dan terisolasi rapi.
/// Menggabungkan Opsi 1 & Opsi 2:
/// - Memotong berdasarkan batas seksi/heading dan mengisolasi blok kode script utuh.
/// - Menjamin setiap balon pesan memiliki tag penutup HTML yang sempurna.
pub fn split_markdown_into_html_messages(markdown: &str, max_chars: usize) -> Vec<String> {
    if markdown.trim().is_empty() {
        return vec!["(Pesan kosong)".to_string()];
    }

    let raw_blocks = extract_markdown_blocks(markdown);
    let mut messages = Vec::new();
    let mut current_markdown_buf = String::new();

    for block in raw_blocks {
        // Jika blok adalah Code Block besar (> max_chars), potong kode secara aman
        if block.is_code_block && block.content.len() > max_chars {
            // Flush buffer teks sebelumnya jika ada
            if !current_markdown_buf.trim().is_empty() {
                let html = markdown_to_telegram_html(&current_markdown_buf);
                if !html.trim().is_empty() {
                    messages.push(html);
                }
                current_markdown_buf.clear();
            }

            let code_chunks = split_large_code_block(&block.lang, &block.content, max_chars);
            for code_chunk in code_chunks {
                let html = markdown_to_telegram_html(&code_chunk);
                if !html.trim().is_empty() {
                    messages.push(html);
                }
            }
            continue;
        }

        // Jika blok adalah Code Block (Opsi 2: Utamakan isolasi ke balon pesan tersendiri)
        if block.is_code_block {
            if !current_markdown_buf.trim().is_empty() {
                let html = markdown_to_telegram_html(&current_markdown_buf);
                if !html.trim().is_empty() {
                    messages.push(html);
                }
                current_markdown_buf.clear();
            }

            let code_md = format!("```{}\n{}\n```", block.lang, block.content);
            let html = markdown_to_telegram_html(&code_md);
            if !html.trim().is_empty() {
                messages.push(html);
            }
            continue;
        }

        // Untuk teks biasa / heading / list:
        if current_markdown_buf.len() + block.content.len() > max_chars && !current_markdown_buf.trim().is_empty() {
            let html = markdown_to_telegram_html(&current_markdown_buf);
            if !html.trim().is_empty() {
                messages.push(html);
            }
            current_markdown_buf.clear();
        }

        if !current_markdown_buf.is_empty() {
            current_markdown_buf.push_str("\n\n");
        }
        current_markdown_buf.push_str(&block.content);
    }

    // Flush sisa teks di buffer akhir
    if !current_markdown_buf.trim().is_empty() {
        let html = markdown_to_telegram_html(&current_markdown_buf);
        if !html.trim().is_empty() {
            messages.push(html);
        }
    }

    if messages.is_empty() {
        vec![markdown_to_telegram_html(markdown)]
    } else {
        messages
    }
}

/// Struktur representasi blok Markdown (Teks biasa atau Code Block).
struct MarkdownBlock {
    is_code_block: bool,
    lang: String,
    content: String,
}

/// Mengekstrak dokumen Markdown menjadi rangkaian blok semantik.
fn extract_markdown_blocks(input: &str) -> Vec<MarkdownBlock> {
    let mut blocks = Vec::new();
    let mut lines = input.lines().peekable();

    let mut in_code = false;
    let mut code_lang = String::new();
    let mut code_buf = String::new();
    let mut text_buf = String::new();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();

        if trimmed.starts_with("```") {
            if !in_code {
                // Masuk ke code block: flush text_buf sebelumnya
                if !text_buf.trim().is_empty() {
                    blocks.push(MarkdownBlock {
                        is_code_block: false,
                        lang: String::new(),
                        content: text_buf.trim().to_string(),
                    });
                    text_buf.clear();
                }

                in_code = true;
                code_lang = trimmed.trim_start_matches('`').trim().to_string();
                code_buf.clear();
            } else {
                // Keluar dari code block
                in_code = false;
                blocks.push(MarkdownBlock {
                    is_code_block: true,
                    lang: code_lang.clone(),
                    content: code_buf.trim().to_string(),
                });
                code_buf.clear();
            }
            continue;
        }

        if in_code {
            if !code_buf.is_empty() {
                code_buf.push('\n');
            }
            code_buf.push_str(line);
        } else {
            if !text_buf.is_empty() {
                text_buf.push('\n');
            }
            text_buf.push_str(line);
        }
    }

    // Flush sisa buffer
    if in_code && !code_buf.is_empty() {
        blocks.push(MarkdownBlock {
            is_code_block: true,
            lang: code_lang,
            content: code_buf.trim().to_string(),
        });
    } else if !text_buf.trim().is_empty() {
        blocks.push(MarkdownBlock {
            is_code_block: false,
            lang: String::new(),
            content: text_buf.trim().to_string(),
        });
    }

    blocks
}

/// Memotong kode script yang sangat panjang menjadi beberapa sub-blok code valid.
fn split_large_code_block(lang: &str, code: &str, max_chars: usize) -> Vec<String> {
    let mut out = Vec::new();
    let lines: Vec<&str> = code.lines().collect();
    let mut cur_buf = String::new();

    for line in lines {
        if cur_buf.len() + line.len() + 1 > max_chars && !cur_buf.is_empty() {
            out.push(format!("```{}\n{}\n```", lang, cur_buf.trim()));
            cur_buf.clear();
        }

        if !cur_buf.is_empty() {
            cur_buf.push('\n');
        }
        cur_buf.push_str(line);
    }

    if !cur_buf.trim().is_empty() {
        out.push(format!("```{}\n{}\n```", lang, cur_buf.trim()));
    }

    out
}

/// Mengonversi teks Markdown standar (dari LLM / OMP) menjadi format HTML resmi Telegram.
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

        // 3. Penanganan Separator Horizontal Line (--- / ***) -> lewati dengan spasi bersih
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            output.push('\n');
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

fn parse_list_item(line: &str) -> Option<String> {
    if (line.starts_with("* ") || line.starts_with("- ")) && line.len() > 2 {
        Some(line[2..].trim().to_string())
    } else {
        None
    }
}

fn is_numbered_list(line: &str) -> bool {
    let mut parts = line.splitn(2, '.');
    if let (Some(num), Some(rest)) = (parts.next(), parts.next()) {
        if num.chars().all(|c| c.is_ascii_digit()) && rest.starts_with(' ') {
            return true;
        }
    }
    false
}

fn parse_blockquote(line: &str) -> Option<String> {
    if line.starts_with("> ") && line.len() > 2 {
        Some(line[2..].trim().to_string())
    } else if line.starts_with('>') && line.len() > 1 {
        Some(line[1..].trim().to_string())
    } else {
        None
    }
}

pub fn parse_inline_formatting(input: &str) -> String {
    let mut result = String::with_capacity(input.len() + 32);
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

        if (chars[i] == '*' || chars[i] == '_') && (i == 0 || chars[i - 1].is_whitespace() || chars[i - 1] == '(') {
            let marker = chars[i];
            if let Some(close_idx) = find_next_char(&chars, i + 1, marker) {
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

/// Representasi item sesi untuk picker /resume Telegram.
#[derive(Debug, Clone)]
pub struct SessionItem {
    pub id_prefix: String,
    pub title: String,
    pub file_path: String,
    pub timestamp_str: String,
}

/// Membaca daftar file sesi .jsonl dari ~/.omp/agent/sessions/ yang sesuai dengan workspace.
pub fn list_workspace_sessions(workspace: &std::path::Path) -> Vec<SessionItem> {
    let mut results = Vec::new();
    let home = match std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        Ok(h) => h,
        Err(_) => return results,
    };

    let sessions_base = std::path::PathBuf::from(home).join(".omp").join("agent").join("sessions");
    if !sessions_base.exists() {
        return results;
    }

    let canonical = workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());
    let canonical_str = canonical.to_string_lossy();
    let clean_ws = canonical_str.replace([':', '\\', '/', '_'], "-");

    // Cari direktori session yang cocok
    let mut target_dir = None;
    if let Ok(entries) = std::fs::read_dir(&sessions_base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let dirname = path.file_name().unwrap_or_default().to_string_lossy();
                if dirname.contains(&clean_ws) || clean_ws.contains(&dirname.replace("--", "")) {
                    target_dir = Some(path);
                    break;
                }
            }
        }
    }

    let target_dir = match target_dir {
        Some(d) => d,
        None => return results,
    };

    // Baca seluruh file .jsonl
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&target_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                if let Ok(metadata) = entry.metadata() {
                    let modified = metadata.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    files.push((path, modified));
                }
            }
        }
    }

    // Urutkan dari yang paling baru diubah (newest first)
    files.sort_by(|a, b| b.1.cmp(&a.1));

    // Ambil maksimal 6 sesi terbaru untuk Telegram keyboard
    // Ambil maksimal 6 sesi terbaru untuk Telegram keyboard
    for (path, _) in files.into_iter().take(6) {
        let file_path_str = path.to_string_lossy().to_string();
        let mut title = String::new();
        let mut id_prefix = String::new();
        let mut timestamp_str = String::new();

        if let Ok(file) = std::fs::File::open(&path) {
            let reader = std::io::BufReader::new(file);
            for line_res in std::io::BufRead::lines(reader).take(8) {
                if let Ok(line) = line_res {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                        if let Some(t) = json.get("title").and_then(|t| t.as_str()) {
                            if !t.trim().is_empty() && title.is_empty() {
                                title = t.trim().to_string();
                            }
                        }
                        if let Some(id) = json.get("id").and_then(|i| i.as_str()) {
                            if id_prefix.is_empty() && id.len() >= 8 {
                                id_prefix = id[..8].to_string();
                            }
                        }
                        if let Some(ts) = json.get("timestamp").and_then(|t| t.as_str()) {
                            if timestamp_str.is_empty() && ts.len() >= 10 {
                                timestamp_str = ts[..10].to_string();
                            }
                        }
                        if title.is_empty() {
                            if let Some(msg_text) = json.get("message")
                                .and_then(|m| m.get("content"))
                                .and_then(|c| c.as_array())
                                .and_then(|arr| arr.first())
                                .and_then(|item| item.get("text"))
                                .and_then(|t| t.as_str())
                            {
                                if !msg_text.trim().is_empty() {
                                    title = msg_text.chars().take(35).collect();
                                }
                            }
                        }
                    }
                }
            }
        }

        if title.is_empty() {
            title = "Sesi Koding".to_string();
        }
        if id_prefix.is_empty() {
            id_prefix = path.file_stem().unwrap_or_default().to_string_lossy().chars().take(8).collect();
        }

        results.push(SessionItem {
            id_prefix,
            title,
            file_path: file_path_str,
            timestamp_str,
        });
    }

    results
}

/// Mengambil judul sesi aktif terbaru dari disk berdasarkan workspace.
pub fn get_active_session_title(workspace: &std::path::Path) -> String {
    let sessions = list_workspace_sessions(workspace);
    if let Some(first) = sessions.first() {
        if !first.title.trim().is_empty() && first.title != "Sesi Koding" {
            return first.title.clone();
        }
    }
    "Sesi Utama".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_markdown_with_code_block() {
        let md = "Pengantar teks.\n\n```typescript\nconst x = 10;\n```\n\nPenutup teks.";
        let messages = split_markdown_into_html_messages(md, 3500);
        assert_eq!(messages.len(), 3);
        assert!(messages[0].contains("Pengantar teks."));
        assert!(messages[1].contains("<pre><code class=\"language-typescript\">const x = 10;</code></pre>"));
        assert!(messages[2].contains("Penutup teks."));
    }
}
