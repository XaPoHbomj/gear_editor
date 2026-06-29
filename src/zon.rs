#![allow(dead_code)]
use std::collections::HashMap;
use std::{fs, path::Path as FsPath};

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
                if bytes[i] == b'{' { entry_depth += 1; }
                if bytes[i] == b'}' { entry_depth -= 1; }
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
                                if raw.as_bytes()[pos] == b'\\' { pos += 1; }
                                pos += 1;
                            }
                            if pos < raw.len() { pos += 1; }
                            let v = raw[value_start + 1..pos - 1].to_string();
                            map.insert(key, v);
                        } else {
                            while pos < raw.len() && raw.as_bytes()[pos] != b',' && raw.as_bytes()[pos] != b'\n' && raw.as_bytes()[pos] != b'}' {
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
        if bytes[i] == b'{' { depth += 1; }
        if bytes[i] == b'}' { depth -= 1; }
        i += 1;
    }
    entries
}

#[derive(Debug, Clone)]
pub(crate) enum ZValue {
    Null,
    Bool(bool),
    Number(i64),
    String(String),
    Enum(String),
    Array(Vec<ZValue>),
    Object(Vec<(String, ZValue)>),
}

pub(crate) fn read_zon(path: &FsPath) -> Option<ZValue> {
    let data = fs::read_to_string(path).ok()?;
    parse_zon(&data).ok()
}

pub(crate) fn read_zon_verbose(path: &FsPath) -> Option<ZValue> {
    let data = fs::read_to_string(path).ok()?;
    match parse_zon(&data) {
        Ok(value) => Some(value),
        Err(err) => {
            eprintln!("[zon] parse failed path={} err={}", path.display(), err);
            None
        }
    }
}

pub(crate) fn parse_zon(data: &str) -> Result<ZValue, String> {
    let mut parser = ZonParser::new(data);
    parser.parse_value()
}

pub(crate) fn zon_serialize(value: &ZValue) -> String {
    let mut out = String::new();
    serialize_zon_pretty(value, &mut out, 0);
    out
}

pub(crate) fn format_zon_pretty(content: &str) -> String {
    let mut out = match parse_zon(content) {
        Ok(value) => zon_serialize(&value),
        Err(_) => content.to_string(),
    };
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn serialize_zon_pretty(value: &ZValue, out: &mut String, level: usize) {
    match value {
        ZValue::Null => out.push_str("null"),
        ZValue::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
        ZValue::Number(v) => out.push_str(&v.to_string()),
        ZValue::String(v) => {
            out.push('"');
            out.push_str(&v.replace('\\', "\\\\").replace('"', "\\\""));
            out.push('"');
        }
        ZValue::Enum(v) => {
            out.push('.');
            out.push_str(v);
        }
        ZValue::Array(items) => {
            out.push_str(".{");
            if !items.is_empty() {
                out.push('\n');
                for item in items {
                    write_indent(out, level + 1);
                    serialize_zon_pretty(item, out, level + 1);
                    out.push_str(",\n");
                }
                write_indent(out, level);
            }
            out.push('}');
        }
        ZValue::Object(fields) => {
            out.push_str(".{");
            if !fields.is_empty() {
                out.push('\n');
                for (key, value) in fields {
                    write_indent(out, level + 1);
                    out.push('.');
                    out.push_str(key);
                    out.push_str(" = ");
                    serialize_zon_pretty(value, out, level + 1);
                    out.push_str(",\n");
                }
                write_indent(out, level);
            }
            out.push('}');
        }
    }
}

fn write_indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("    ");
    }
}

struct ZonParser<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ZonParser<'a> {
    fn new(data: &'a str) -> Self {
        Self {
            data: data.as_bytes(),
            pos: 0,
        }
    }

    fn parse_value(&mut self) -> Result<ZValue, String> {
        self.skip_ws();
        match self.peek_char() {
            Some(b'.') => {
                if self.peek_next_is(b".{") {
                    self.parse_container()
                } else {
                    self.consume_char();
                    let ident = self.parse_ident()?;
                    Ok(ZValue::Enum(ident))
                }
            }
            Some(b'"') => Ok(ZValue::String(self.parse_string()?)),
            Some(b't') | Some(b'f') => Ok(ZValue::Bool(self.parse_bool()?)),
            Some(b'n') => {
                self.expect_keyword("null")?;
                Ok(ZValue::Null)
            }
            Some(b'-') | Some(b'0'..=b'9') => Ok(ZValue::Number(self.parse_number()?)),
            other => Err(format!("unexpected token at {} ({:?})", self.pos, other)),
        }
    }

    fn parse_container(&mut self) -> Result<ZValue, String> {
        self.expect_keyword(".{")?;
        self.skip_ws();
        let mut entries: Vec<ContainerEntry> = Vec::new();

        while !self.peek_char_is(b'}') {
            self.skip_ws();
            if self.peek_char_is(b'}') {
                break;
            }
            if self.peek_char_is(b'.') {
                if self.peek_next_is(b".{") {
                    let value = self.parse_value()?;
                    entries.push(ContainerEntry::Value(value));
                    self.skip_ws();
                    if self.peek_char_is(b',') {
                        self.consume_char();
                    }
                    continue;
                }
                let save_pos = self.pos;
                self.consume_char();
                let ident = self.parse_ident()?;
                self.skip_ws();
                if self.peek_char_is(b'=') {
                    self.consume_char();
                    let value = self.parse_value()?;
                    entries.push(ContainerEntry::Field(ident, value));
                } else {
                    self.pos = save_pos;
                    let value = self.parse_value()?;
                    entries.push(ContainerEntry::Value(value));
                }
            } else {
                let value = self.parse_value()?;
                entries.push(ContainerEntry::Value(value));
            }

            self.skip_ws();
            if self.peek_char_is(b',') {
                self.consume_char();
                self.skip_ws();
            } else {
                break;
            }
        }

        if self.peek_char_is(b'}') {
            self.consume_char();
        }

        let is_object = entries
            .iter()
            .any(|e| matches!(e, ContainerEntry::Field(_, _)));
        if is_object {
            let mut fields = Vec::new();
            for entry in entries {
                if let ContainerEntry::Field(key, value) = entry {
                    fields.push((key, value));
                }
            }
            Ok(ZValue::Object(fields))
        } else {
            let mut values = Vec::new();
            for entry in entries {
                if let ContainerEntry::Value(value) = entry {
                    values.push(value);
                }
            }
            Ok(ZValue::Array(values))
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect_char(b'"')?;
        let mut out = String::new();
        while let Some(ch) = self.peek_char() {
            self.consume_char();
            match ch {
                b'"' => break,
                b'\\' => {
                    if let Some(next) = self.peek_char() {
                        self.consume_char();
                        out.push(next as char);
                    }
                }
                _ => out.push(ch as char),
            }
        }
        Ok(out)
    }

    fn parse_bool(&mut self) -> Result<bool, String> {
        if self.consume_if_keyword("true") {
            Ok(true)
        } else if self.consume_if_keyword("false") {
            Ok(false)
        } else {
            Err("invalid bool".into())
        }
    }

    fn parse_number(&mut self) -> Result<i64, String> {
        let start = self.pos;
        if self.peek_char_is(b'-') {
            self.consume_char();
        }
        while matches!(self.peek_char(), Some(b'0'..=b'9')) {
            self.consume_char();
        }
        let slice = std::str::from_utf8(&self.data[start..self.pos]).map_err(|_| "bad number")?;
        slice.parse::<i64>().map_err(|_| "bad number".into())
    }

    fn parse_ident(&mut self) -> Result<String, String> {
        let start = self.pos;
        while matches!(
            self.peek_char(),
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'0'..=b'9')
        ) {
            self.consume_char();
        }
        if self.pos == start {
            return Err("expected identifier".into());
        }
        let slice = std::str::from_utf8(&self.data[start..self.pos]).map_err(|_| "bad ident")?;
        Ok(slice.to_string())
    }

    fn skip_ws(&mut self) {
        loop {
            while matches!(self.peek_char(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
                self.consume_char();
            }

            if self.peek_next_is(b"//") {
                while let Some(ch) = self.peek_char() {
                    self.consume_char();
                    if ch == b'\n' {
                        break;
                    }
                }
                continue;
            }

            if self.peek_next_is(b"/*") {
                self.consume_char();
                self.consume_char();
                while let Some(_) = self.peek_char() {
                    if self.peek_next_is(b"*/") {
                        self.consume_char();
                        self.consume_char();
                        break;
                    }
                    self.consume_char();
                }
                continue;
            }

            break;
        }
    }

    fn peek_char(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    fn peek_char_is(&self, ch: u8) -> bool {
        self.peek_char() == Some(ch)
    }

    fn consume_char(&mut self) {
        self.pos = self.pos.saturating_add(1);
    }

    fn expect_char(&mut self, ch: u8) -> Result<(), String> {
        if self.peek_char_is(ch) {
            self.consume_char();
            Ok(())
        } else {
            Err(format!("expected '{}'", ch as char))
        }
    }

    fn expect_keyword(&mut self, keyword: &str) -> Result<(), String> {
        if self.consume_if_keyword(keyword) {
            Ok(())
        } else {
            Err(format!("expected {keyword}"))
        }
    }

    fn consume_if_keyword(&mut self, keyword: &str) -> bool {
        if self.data[self.pos..].starts_with(keyword.as_bytes()) {
            self.pos += keyword.len();
            true
        } else {
            false
        }
    }

    fn peek_next_is(&self, bytes: &[u8]) -> bool {
        self.data[self.pos..].starts_with(bytes)
    }
}

enum ContainerEntry {
    Field(String, ZValue),
    Value(ZValue),
}

pub(crate) fn zon_get_number(value: &ZValue, field: &str) -> Option<i64> {
    match value {
        ZValue::Object(fields) => fields.iter().find_map(|(k, v)| {
            if k == field {
                if let ZValue::Number(num) = v {
                    Some(*num)
                } else {
                    None
                }
            } else {
                None
            }
        }),
        _ => None,
    }
}

pub(crate) fn zon_set_number(value: &mut ZValue, field: &str, num: i64) {
    if let ZValue::Object(fields) = value {
        if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == field) {
            *v = ZValue::Number(num);
            return;
        }
        fields.push((field.to_string(), ZValue::Number(num)));
    }
}

pub(crate) fn zon_get_bool(value: &ZValue, field: &str) -> Option<bool> {
    match value {
        ZValue::Object(fields) => fields.iter().find_map(|(k, v)| {
            if k == field {
                if let ZValue::Bool(b) = v {
                    Some(*b)
                } else {
                    None
                }
            } else {
                None
            }
        }),
        _ => None,
    }
}

pub(crate) fn zon_set_bool(value: &mut ZValue, field: &str, b: bool) {
    if let ZValue::Object(fields) = value {
        if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == field) {
            *v = ZValue::Bool(b);
            return;
        }
        fields.push((field.to_string(), ZValue::Bool(b)));
    }
}

pub(crate) fn zon_get_array_numbers(value: &ZValue, field: &str) -> Vec<u32> {
    match value {
        ZValue::Object(fields) => fields
            .iter()
            .find(|(k, _)| k == field)
            .and_then(|(_, v)| match v {
                ZValue::Array(items) => Some(
                    items
                        .iter()
                        .filter_map(|item| match item {
                            ZValue::Number(n) => Some(*n as u32),
                            _ => None,
                        })
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .unwrap_or_default(),
        _ => vec![],
    }
}

pub(crate) fn zon_set_dressed_equip(value: &mut ZValue, field: &str, items: &[u32], slots: usize) {
    let mut array_items = Vec::with_capacity(slots);
    for idx in 0..slots {
        if let Some(uid) = items.get(idx) {
            if *uid == 0 {
                array_items.push(ZValue::Null);
            } else {
                array_items.push(ZValue::Number(*uid as i64));
            }
        } else {
            array_items.push(ZValue::Null);
        }
    }

    let array = ZValue::Array(array_items);
    if let ZValue::Object(fields) = value {
        if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == field) {
            *v = array;
            return;
        }
        fields.push((field.to_string(), array));
    }
}

pub(crate) fn zon_get_skill_levels(value: &ZValue) -> HashMap<String, u32> {
    let mut result = HashMap::new();
    if let ZValue::Object(fields) = value {
        if let Some((_, ZValue::Array(skills))) =
            fields.iter().find(|(k, _)| k == "skill_type_level")
        {
            for skill in skills {
                if let ZValue::Object(skill_fields) = skill {
                    let mut key = None;
                    let mut level = None;
                    for (k, v) in skill_fields {
                        if k == "type" {
                            if let ZValue::Enum(name) = v {
                                key = Some(name.clone());
                            }
                        }
                        if k == "level" {
                            if let ZValue::Number(num) = v {
                                level = Some(*num as u32);
                            }
                        }
                    }
                    if let (Some(key), Some(level)) = (key, level) {
                        result.insert(key, level);
                    }
                }
            }
        }
    }

    result
}

pub(crate) fn zon_set_skill_levels(value: &mut ZValue, levels: &mut Vec<(&str, u32)>) {
    if let ZValue::Object(fields) = value {
        let mut existing: HashMap<String, u32> = HashMap::new();
        if let Some((_, ZValue::Array(skills))) =
            fields.iter().find(|(k, _)| k == "skill_type_level")
        {
            for skill in skills {
                if let ZValue::Object(skill_fields) = skill {
                    let mut key = None;
                    let mut level = None;
                    for (k, v) in skill_fields {
                        if k == "type" {
                            if let ZValue::Enum(name) = v {
                                key = Some(name.clone());
                            }
                        }
                        if k == "level" {
                            if let ZValue::Number(num) = v {
                                level = Some(*num as u32);
                            }
                        }
                    }
                    if let (Some(key), Some(level)) = (key, level) {
                        existing.insert(key, level);
                    }
                }
            }
        }

        for (name, lvl) in levels.iter() {
            existing.insert((*name).to_string(), *lvl);
        }

        let mut array = Vec::new();
        for (name, lvl) in levels.iter() {
            array.push(ZValue::Object(vec![
                ("type".to_string(), ZValue::Enum((*name).to_string())),
                ("level".to_string(), ZValue::Number(*lvl as i64)),
            ]));
        }

        for (name, lvl) in existing {
            if levels.iter().any(|(known, _)| *known == name) {
                continue;
            }
            array.push(ZValue::Object(vec![
                ("type".to_string(), ZValue::Enum(name)),
                ("level".to_string(), ZValue::Number(lvl as i64)),
            ]));
        }

        if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == "skill_type_level") {
            *v = ZValue::Array(array);
            return;
        }
        fields.push(("skill_type_level".to_string(), ZValue::Array(array)));
    }
}

pub(crate) fn zon_get_entrance_zone_id(value: &ZValue, entrance_id: u32) -> Option<u32> {
    let ZValue::Object(fields) = value else {
        return None;
    };
    let Some((_, ZValue::Array(entrances))) = fields.iter().find(|(k, _)| k == "entrances") else {
        return None;
    };
    for entry in entrances {
        let ZValue::Object(items) = entry else {
            continue;
        };
        let mut id = None;
        let mut zone_id = None;
        for (k, v) in items {
            if k == "id" {
                if let ZValue::Number(num) = v {
                    id = Some(*num as u32);
                }
            }
            if k == "zone_id" {
                if let ZValue::Number(num) = v {
                    zone_id = Some(*num as u32);
                }
            }
        }
        if id == Some(entrance_id) {
            return zone_id;
        }
    }
    None
}

pub(crate) fn zon_set_entrance_zone_id(value: &mut ZValue, entrance_id: u32, zone_id: u32) {
    let ZValue::Object(fields) = value else {
        return;
    };

    let entrances_index = fields.iter().position(|(k, _)| k == "entrances");
    if entrances_index.is_none() {
        fields.push(("entrances".to_string(), ZValue::Array(Vec::new())));
    }
    let entrances_index = entrances_index.unwrap_or(fields.len().saturating_sub(1));

    let items = match &mut fields[entrances_index].1 {
        ZValue::Array(items) => items,
        _ => {
            fields[entrances_index].1 = ZValue::Array(Vec::new());
            match &mut fields[entrances_index].1 {
                ZValue::Array(items) => items,
                _ => return,
            }
        }
    };

    for entry in items.iter_mut() {
        let ZValue::Object(entry_fields) = entry else {
            continue;
        };
        let mut id = None;
        for (k, v) in entry_fields.iter() {
            if k == "id" {
                if let ZValue::Number(num) = v {
                    id = Some(*num as u32);
                }
            }
        }
        if id == Some(entrance_id) {
            if let Some((_, v)) = entry_fields.iter_mut().find(|(k, _)| k == "zone_id") {
                *v = ZValue::Number(zone_id as i64);
            } else {
                entry_fields.push(("zone_id".to_string(), ZValue::Number(zone_id as i64)));
            }
            return;
        }
    }

    items.push(ZValue::Object(vec![
        ("id".to_string(), ZValue::Number(entrance_id as i64)),
        ("zone_id".to_string(), ZValue::Number(zone_id as i64)),
    ]));
}

pub(crate) fn zon_get_main_property(value: &ZValue) -> (u32, u32, u32) {
    if let ZValue::Object(fields) = value {
        if let Some((_, ZValue::Array(properties))) = fields.iter().find(|(k, _)| k == "properties")
        {
            if let Some(ZValue::Object(prop_fields)) = properties.first() {
                let key = prop_fields
                    .iter()
                    .find(|(k, _)| k == "key")
                    .and_then(|(_, v)| match v {
                        ZValue::Number(num) => Some(*num as u32),
                        _ => None,
                    })
                    .unwrap_or(0);
                let base = prop_fields
                    .iter()
                    .find(|(k, _)| k == "base_value")
                    .and_then(|(_, v)| match v {
                        ZValue::Number(num) => Some(*num as u32),
                        _ => None,
                    })
                    .unwrap_or(0);
                let add = prop_fields
                    .iter()
                    .find(|(k, _)| k == "add_value")
                    .and_then(|(_, v)| match v {
                        ZValue::Number(num) => Some(*num as u32),
                        _ => None,
                    })
                    .unwrap_or(0);
                return (key, base, add);
            }
        }
    }

    (0, 0, 0)
}

pub(crate) fn zon_set_main_property(value: &mut ZValue, key: u32, base: u32, add: u32) {
    let prop = ZValue::Object(vec![
        ("key".to_string(), ZValue::Number(key as i64)),
        ("base_value".to_string(), ZValue::Number(base as i64)),
        ("add_value".to_string(), ZValue::Number(add as i64)),
    ]);

    if let ZValue::Object(fields) = value {
        if let Some((_, ZValue::Array(properties))) =
            fields.iter_mut().find(|(k, _)| k == "properties")
        {
            if properties.is_empty() {
                properties.push(prop);
            } else {
                properties[0] = prop;
            }
            return;
        }
        fields.push(("properties".to_string(), ZValue::Array(vec![prop])));
    }
}

pub(crate) fn zon_get_sub_properties_list(value: &ZValue) -> Vec<(u32, u32, u32)> {
    let mut list = Vec::new();
    if let ZValue::Object(fields) = value {
        if let Some((_, ZValue::Array(properties))) =
            fields.iter().find(|(k, _)| k == "sub_properties")
        {
            for prop in properties {
                if let ZValue::Object(prop_fields) = prop {
                    let key = prop_fields
                        .iter()
                        .find(|(k, _)| k == "key")
                        .and_then(|(_, v)| match v {
                            ZValue::Number(num) => Some(*num as u32),
                            _ => None,
                        })
                        .unwrap_or(0);
                    let base = prop_fields
                        .iter()
                        .find(|(k, _)| k == "base_value")
                        .and_then(|(_, v)| match v {
                            ZValue::Number(num) => Some(*num as u32),
                            _ => None,
                        })
                        .unwrap_or(0);
                    let add = prop_fields
                        .iter()
                        .find(|(k, _)| k == "add_value")
                        .and_then(|(_, v)| match v {
                            ZValue::Number(num) => Some(*num as u32),
                            _ => None,
                        })
                        .unwrap_or(0);
                    list.push((key, base, add));
                }
            }
        }
    }

    list
}

pub(crate) fn zon_set_sub_properties(value: &mut ZValue, keys: &[u32], base: &[u32], add: &[u32]) {
    let mut list = Vec::new();
    for idx in 0..keys.len() {
        let base_val = base.get(idx).copied().unwrap_or(0);
        let add_val = add.get(idx).copied().unwrap_or(0);
        list.push(ZValue::Object(vec![
            ("key".to_string(), ZValue::Number(keys[idx] as i64)),
            ("base_value".to_string(), ZValue::Number(base_val as i64)),
            ("add_value".to_string(), ZValue::Number(add_val as i64)),
        ]));
    }

    if let ZValue::Object(fields) = value {
        if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == "sub_properties") {
            *v = ZValue::Array(list);
            return;
        }
        fields.push(("sub_properties".to_string(), ZValue::Array(list)));
    }
}
