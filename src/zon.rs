use std::collections::HashMap;

pub(crate) fn zon_parse_entries(data: &str) -> Vec<HashMap<String, String>> {
    let mut entries = Vec::new();
    let mut depth = 0;
    let mut i = 0;
    let bytes = data.as_bytes();

    while i < bytes.len() {
        // Find opening brace of an entry
        if bytes[i] == b'{' && depth == 1 {
            i += 1;
            let start = i;
            let mut entry_depth = 1;
            while i < bytes.len() && entry_depth > 0 {
                if bytes[i] == b'{' {
                    entry_depth += 1;
                }
                if bytes[i] == b'}' {
                    entry_depth -= 1;
                }
                i += 1;
            }
            let raw = &data[start..i - 1];
            let mut map = HashMap::new();
            let mut pos = 0;
            while pos < raw.len() {
                if let Some(dot) = raw[pos..].find('.') {
                    pos += dot + 1;
                    if let Some(eq) = raw[pos..].find('=') {
                        let key = raw[pos..pos + eq].trim().to_string();
                        pos += eq + 1;
                        let value_start = pos;
                        if pos < raw.len() && raw.as_bytes()[pos] == b'"' {
                            pos += 1;
                            while pos < raw.len() && raw.as_bytes()[pos] != b'"' {
                                if raw.as_bytes()[pos] == b'\\' {
                                    pos += 1;
                                }
                                pos += 1;
                            }
                            if pos < raw.len() {
                                pos += 1;
                            }
                            let v = raw[value_start + 1..pos - 1].to_string();
                            map.insert(key, v);
                        } else {
                            while pos < raw.len()
                                && raw.as_bytes()[pos] != b','
                                && raw.as_bytes()[pos] != b'\n'
                                && raw.as_bytes()[pos] != b'}'
                            {
                                pos += 1;
                            }
                            let v = raw[value_start..pos].trim().to_string();
                            map.insert(key, v);
                        }
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            if !map.is_empty() {
                entries.push(map);
            }
            continue;
        }
        if bytes[i] == b'{' {
            depth += 1;
        }
        if bytes[i] == b'}' {
            depth -= 1;
        }
        i += 1;
    }
    entries
}
