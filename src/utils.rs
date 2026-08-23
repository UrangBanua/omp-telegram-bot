//! Utilitas untuk chunking pesan, escaping karakter Telegram, dan formatting tampilan.

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

        // Cari batas baris baru terdekat sebelum batas max_len
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

/// Format indikator status eksekusi tool OMP.
pub fn format_tool_status(tool_name: &str, intent: Option<&str>) -> String {
    let mut out = format!("🛠️ <i>[Tool: <code>{}</code>]</i>", escape_html(tool_name));
    if let Some(i) = intent {
        if !i.is_empty() {
            out.push_str(&format!(" - {}", escape_html(i)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_message_short() {
        let text = "Halo dunia";
        let chunks = chunk_message(text, 100);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Halo dunia");
    }

    #[test]
    fn test_chunk_message_long_newlines() {
        let text = "Baris 1\nBaris 2\nBaris 3\nBaris 4";
        let chunks = chunk_message(text, 16);
        assert!(chunks.len() >= 2);
    }

    #[test]
    fn test_escape_html() {
        let raw = "<script>alert('x & y');</script>";
        let escaped = escape_html(raw);
        assert_eq!(escaped, "&lt;script&gt;alert('x &amp; y');&lt;/script&gt;");
    }
}
