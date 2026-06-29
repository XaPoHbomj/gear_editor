use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub(crate) struct PlayerSave {
    pub(crate) basic: Option<BasicSave>,
    pub(crate) avatar: Vec<AvatarItemSave>,
    pub(crate) weapon: Vec<WeaponItemSave>,
    pub(crate) equip: Vec<EquipItemSave>,
    pub(crate) buddy: Vec<BuddyItemSave>,
    pub(crate) hall: Option<HallSave>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct BasicSave {
    pub(crate) level: u32,
    pub(crate) avatar_id: u32,
    pub(crate) control_avatar_id: u32,
    pub(crate) control_guise_avatar_id: u32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AvatarItemSave {
    pub(crate) id: u32,
    pub(crate) level: u32,
    pub(crate) exp: u32,
    pub(crate) rank: u32,
    pub(crate) talents: u32,
    pub(crate) talent_switch: u32,
    pub(crate) favorite: bool,
    pub(crate) skill_levels: Vec<u32>,
    pub(crate) skin_id: u32,
    pub(crate) awake_available: bool,
    pub(crate) awake_enabled: bool,
    pub(crate) awake_id: u32,
    pub(crate) weapon_uid: u32,
    pub(crate) equipment_uids: Vec<u32>,
    pub(crate) awake_material_count: u32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct WeaponItemSave {
    pub(crate) uid: u32,
    pub(crate) id: u32,
    pub(crate) level: u32,
    pub(crate) star: u32,
    pub(crate) refine: u32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct EquipItemSave {
    pub(crate) uid: u32,
    pub(crate) id: u32,
    pub(crate) level: u32,
    pub(crate) star: u32,
    pub(crate) properties: Vec<EquipProperty>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct EquipProperty {
    pub(crate) key: u32,
    pub(crate) base_value: u32,
    pub(crate) add_value: u32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct BuddyItemSave {
    pub(crate) id: u32,
    pub(crate) level: u32,
    pub(crate) exp: u32,
    pub(crate) rank: u32,
    pub(crate) star: u32,
    pub(crate) favorite: bool,
    pub(crate) skill_levels: Vec<u32>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct HallSave {
    pub(crate) section_id: u32,
}

pub(crate) fn decode_player_save(buf: &[u8]) -> Option<PlayerSave> {
    let mut save = PlayerSave::default();
    let mut pos = 0;
    while pos < buf.len() {
        let (tag, new_pos) = read_varint(buf, pos)?;
        pos = new_pos;
        let field = tag >> 3;
        let wire = tag & 7;
        match (field, wire) {
            (1, 2) => {
                let (sub, new_pos) = read_ld(buf, pos)?;
                pos = new_pos;
                save.basic = Some(decode_basic_save(sub));
            }
            (2, 2) => {
                let (sub, new_pos) = read_ld(buf, pos)?;
                pos = new_pos;
                save.avatar = decode_avatar_save_list(sub);
            }
            (3, 2) => {
                let (sub, new_pos) = read_ld(buf, pos)?;
                pos = new_pos;
                save.weapon = decode_weapon_save_list(sub);
            }
            (4, 2) => {
                let (sub, new_pos) = read_ld(buf, pos)?;
                pos = new_pos;
                save.equip = decode_equip_save_list(sub);
            }
            (5, 2) => {
                let (sub, new_pos) = read_ld(buf, pos)?;
                pos = new_pos;
                save.buddy = decode_buddy_save_list(sub);
            }
            (6, 2) => {
                let (sub, new_pos) = read_ld(buf, pos)?;
                pos = new_pos;
                save.hall = Some(decode_hall_save(sub));
            }
            _ => {
                if !skip_field(wire, buf, &mut pos) {
                    return None;
                }
            }
        }
    }
    Some(save)
}

pub(crate) fn encode_player_save(save: &PlayerSave) -> Vec<u8> {
    let mut buf = Vec::new();
    if let Some(ref basic) = save.basic {
        let inner = encode_basic_save(basic);
        encode_ld(&mut buf, 1, &inner);
    }
    if !save.avatar.is_empty() {
        let inner = encode_avatar_save_list(&save.avatar);
        encode_ld(&mut buf, 2, &inner);
    }
    if !save.weapon.is_empty() {
        let inner = encode_weapon_save_list(&save.weapon);
        encode_ld(&mut buf, 3, &inner);
    }
    if !save.equip.is_empty() {
        let inner = encode_equip_save_list(&save.equip);
        encode_ld(&mut buf, 4, &inner);
    }
    if !save.buddy.is_empty() {
        let inner = encode_buddy_save_list(&save.buddy);
        encode_ld(&mut buf, 5, &inner);
    }
    if let Some(ref hall) = save.hall {
        let inner = encode_hall_save(hall);
        encode_ld(&mut buf, 6, &inner);
    }
    buf
}

fn decode_basic_save(buf: &[u8]) -> BasicSave {
    let mut basic = BasicSave::default();
    let mut pos = 0;
    while pos < buf.len() {
        let (tag, new_pos) = read_varint(buf, pos).unwrap_or((0, buf.len()));
        pos = new_pos;
        let field = tag >> 3;
        match field {
            1 => { if let Some((v, np)) = read_varint(buf, pos) { basic.level = v as u32; pos = np; } }
            2 => { if let Some((v, np)) = read_varint(buf, pos) { basic.avatar_id = v as u32; pos = np; } }
            3 => { if let Some((v, np)) = read_varint(buf, pos) { basic.control_avatar_id = v as u32; pos = np; } }
            4 => { if let Some((v, np)) = read_varint(buf, pos) { basic.control_guise_avatar_id = v as u32; pos = np; } }
            _ => { if !skip_field(tag & 7, buf, &mut pos) { break; } }
        }
    }
    basic
}

fn encode_basic_save(save: &BasicSave) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_varint_field(&mut buf, 1, save.level as u64);
    encode_varint_field(&mut buf, 2, save.avatar_id as u64);
    encode_varint_field(&mut buf, 3, save.control_avatar_id as u64);
    encode_varint_field(&mut buf, 4, save.control_guise_avatar_id as u64);
    buf
}

fn decode_avatar_save(buf: &[u8]) -> AvatarItemSave {
    let mut item = AvatarItemSave::default();
    let mut pos = 0;
    while pos < buf.len() {
        let (tag, new_pos) = read_varint(buf, pos).unwrap_or((0, buf.len()));
        pos = new_pos;
        let field = tag >> 3;
        let wire = tag & 7;
        match (field, wire) {
            (1, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.id = v as u32; pos = np; } }
            (2, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.level = v as u32; pos = np; } }
            (3, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.exp = v as u32; pos = np; } }
            (4, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.rank = v as u32; pos = np; } }
            (5, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.talents = v as u32; pos = np; } }
            (6, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.talent_switch = v as u32; pos = np; } }
            (7, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.favorite = v != 0; pos = np; } }
            (8, 2) => {
                let (sub, np) = read_ld(buf, pos).unwrap_or((&[], buf.len()));
                pos = np;
                item.skill_levels = decode_varint_list(sub);
            }
            (9, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.skin_id = v as u32; pos = np; } }
            (10, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.weapon_uid = v as u32; pos = np; } }
            (11, 2) => {
                let (sub, np) = read_ld(buf, pos).unwrap_or((&[], buf.len()));
                pos = np;
                item.equipment_uids = decode_varint_list(sub);
            }
            (12, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.awake_available = v != 0; pos = np; } }
            (13, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.awake_enabled = v != 0; pos = np; } }
            (14, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.awake_id = v as u32; pos = np; } }
            (15, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.awake_material_count = v as u32; pos = np; } }
            _ => { if !skip_field(wire, buf, &mut pos) { break; } }
        }
    }
    item
}

fn encode_avatar_save(item: &AvatarItemSave) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_varint_field(&mut buf, 1, item.id as u64);
    encode_varint_field(&mut buf, 2, item.level as u64);
    encode_varint_field(&mut buf, 3, item.exp as u64);
    encode_varint_field(&mut buf, 4, item.rank as u64);
    encode_varint_field(&mut buf, 5, item.talents as u64);
    encode_varint_field(&mut buf, 6, item.talent_switch as u64);
    encode_varint_field(&mut buf, 7, item.favorite as u64);
    if !item.skill_levels.is_empty() {
        let sk_buf: Vec<u8> = item.skill_levels.iter().flat_map(|v| encode_varint(*v as u64)).collect();
        encode_ld(&mut buf, 8, &sk_buf);
    }
    encode_varint_field(&mut buf, 9, item.skin_id as u64);
    encode_varint_field(&mut buf, 10, item.weapon_uid as u64);
    if !item.equipment_uids.is_empty() {
        let eq_buf: Vec<u8> = item.equipment_uids.iter().flat_map(|v| encode_varint(*v as u64)).collect();
        encode_ld(&mut buf, 11, &eq_buf);
    }
    encode_varint_field(&mut buf, 12, item.awake_available as u64);
    encode_varint_field(&mut buf, 13, item.awake_enabled as u64);
    encode_varint_field(&mut buf, 14, item.awake_id as u64);
    encode_varint_field(&mut buf, 15, item.awake_material_count as u64);
    buf
}

fn decode_avatar_save_list(buf: &[u8]) -> Vec<AvatarItemSave> {
    let mut items = Vec::new();
    let mut pos = 0;
    while pos < buf.len() {
        let (tag, new_pos) = read_varint(buf, pos).unwrap_or((0, buf.len()));
        pos = new_pos;
        let field = tag >> 3;
        let wire = tag & 7;
        if field == 1 && wire == 2 {
            if let Some((sub, np)) = read_ld(buf, pos) {
                pos = np;
                items.push(decode_avatar_save(sub));
            } else {
                break;
            }
        } else {
            break;
        }
    }
    items
}

fn encode_avatar_save_list(items: &[AvatarItemSave]) -> Vec<u8> {
    let mut buf = Vec::new();
    for item in items {
        let inner = encode_avatar_save(item);
        encode_ld(&mut buf, 1, &inner);
    }
    buf
}

fn decode_weapon_save(buf: &[u8]) -> WeaponItemSave {
    let mut item = WeaponItemSave::default();
    let mut pos = 0;
    while pos < buf.len() {
        let (tag, new_pos) = read_varint(buf, pos).unwrap_or((0, buf.len()));
        pos = new_pos;
        let field = tag >> 3;
        match field {
            1 | 2 | 3 | 4 | 5 => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    pos = np;
                    match field {
                        1 => item.uid = v as u32,
                        2 => item.id = v as u32,
                        3 => item.level = v as u32,
                        4 => item.star = v as u32,
                        5 => item.refine = v as u32,
                        _ => {}
                    }
                }
            }
            _ => { if !skip_field(tag & 7, buf, &mut pos) { break; } }
        }
    }
    item
}

fn encode_weapon_save(item: &WeaponItemSave) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_varint_field(&mut buf, 1, item.uid as u64);
    encode_varint_field(&mut buf, 2, item.id as u64);
    encode_varint_field(&mut buf, 3, item.level as u64);
    encode_varint_field(&mut buf, 4, item.star as u64);
    encode_varint_field(&mut buf, 5, item.refine as u64);
    buf
}

fn decode_weapon_save_list(buf: &[u8]) -> Vec<WeaponItemSave> {
    let mut items = Vec::new();
    let mut pos = 0;
    while pos < buf.len() {
        let (tag, new_pos) = read_varint(buf, pos).unwrap_or((0, buf.len()));
        pos = new_pos;
        let field = tag >> 3;
        let wire = tag & 7;
        if field == 1 && wire == 2 {
            if let Some((sub, np)) = read_ld(buf, pos) {
                pos = np;
                items.push(decode_weapon_save(sub));
            } else {
                break;
            }
        } else {
            break;
        }
    }
    items
}

fn encode_weapon_save_list(items: &[WeaponItemSave]) -> Vec<u8> {
    let mut buf = Vec::new();
    for item in items {
        let inner = encode_weapon_save(item);
        encode_ld(&mut buf, 1, &inner);
    }
    buf
}

fn decode_equip_property(buf: &[u8]) -> EquipProperty {
    let mut prop = EquipProperty::default();
    let mut pos = 0;
    while pos < buf.len() {
        let (tag, new_pos) = read_varint(buf, pos).unwrap_or((0, buf.len()));
        pos = new_pos;
        let field = tag >> 3;
        match field {
            1 | 2 | 3 => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    pos = np;
                    match field {
                        1 => prop.key = v as u32,
                        2 => prop.base_value = v as u32,
                        3 => prop.add_value = v as u32,
                        _ => {}
                    }
                }
            }
            _ => { if !skip_field(tag & 7, buf, &mut pos) { break; } }
        }
    }
    prop
}

fn encode_equip_property(prop: &EquipProperty) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_varint_field(&mut buf, 1, prop.key as u64);
    encode_varint_field(&mut buf, 2, prop.base_value as u64);
    encode_varint_field(&mut buf, 3, prop.add_value as u64);
    buf
}

fn decode_equip_save(buf: &[u8]) -> EquipItemSave {
    let mut item = EquipItemSave::default();
    let mut pos = 0;
    while pos < buf.len() {
        let (tag, new_pos) = read_varint(buf, pos).unwrap_or((0, buf.len()));
        pos = new_pos;
        let field = tag >> 3;
        let wire = tag & 7;
        match (field, wire) {
            (1, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.uid = v as u32; pos = np; } }
            (2, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.id = v as u32; pos = np; } }
            (3, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.level = v as u32; pos = np; } }
            (4, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.star = v as u32; pos = np; } }
            (5, 2) => {
                let (sub, np) = read_ld(buf, pos).unwrap_or((&[], buf.len()));
                pos = np;
                item.properties = decode_equip_properties_list(sub);
            }
            _ => { if !skip_field(wire, buf, &mut pos) { break; } }
        }
    }
    item
}

fn decode_equip_properties_list(buf: &[u8]) -> Vec<EquipProperty> {
    let mut props = Vec::new();
    let mut pos = 0;
    while pos < buf.len() {
        let (tag, new_pos) = read_varint(buf, pos).unwrap_or((0, buf.len()));
        pos = new_pos;
        let field = tag >> 3;
        let wire = tag & 7;
        if field == 0 && wire == 2 {
            // packed repeated message — not used here, we expect field=1 repeated
            if let Some((sub, np)) = read_ld(buf, pos) {
                pos = np;
                props.push(decode_equip_property(sub));
            } else {
                break;
            }
        } else if field == 0 && wire == 0 {
            if let Some((_, np)) = read_varint(buf, pos) {
                pos = np;
            }
        } else {
            break;
        }
    }
    props
}

fn encode_equip_save(item: &EquipItemSave) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_varint_field(&mut buf, 1, item.uid as u64);
    encode_varint_field(&mut buf, 2, item.id as u64);
    encode_varint_field(&mut buf, 3, item.level as u64);
    encode_varint_field(&mut buf, 4, item.star as u64);
    if !item.properties.is_empty() {
        let mut props_buf = Vec::new();
        for prop in &item.properties {
            let inner = encode_equip_property(prop);
            encode_ld(&mut props_buf, 0, &inner);
        }
        encode_ld(&mut buf, 5, &props_buf);
    }
    buf
}

fn decode_equip_save_list(buf: &[u8]) -> Vec<EquipItemSave> {
    let mut items = Vec::new();
    let mut pos = 0;
    while pos < buf.len() {
        let (tag, new_pos) = read_varint(buf, pos).unwrap_or((0, buf.len()));
        pos = new_pos;
        let field = tag >> 3;
        let wire = tag & 7;
        if field == 1 && wire == 2 {
            if let Some((sub, np)) = read_ld(buf, pos) {
                pos = np;
                items.push(decode_equip_save(sub));
            } else {
                break;
            }
        } else {
            break;
        }
    }
    items
}

fn encode_equip_save_list(items: &[EquipItemSave]) -> Vec<u8> {
    let mut buf = Vec::new();
    for item in items {
        let inner = encode_equip_save(item);
        encode_ld(&mut buf, 1, &inner);
    }
    buf
}

fn decode_buddy_save(buf: &[u8]) -> BuddyItemSave {
    let mut item = BuddyItemSave::default();
    let mut pos = 0;
    while pos < buf.len() {
        let (tag, new_pos) = read_varint(buf, pos).unwrap_or((0, buf.len()));
        pos = new_pos;
        let field = tag >> 3;
        let wire = tag & 7;
        match (field, wire) {
            (1, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.id = v as u32; pos = np; } }
            (2, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.level = v as u32; pos = np; } }
            (3, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.exp = v as u32; pos = np; } }
            (4, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.rank = v as u32; pos = np; } }
            (5, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.star = v as u32; pos = np; } }
            (6, 0) => { if let Some((v, np)) = read_varint(buf, pos) { item.favorite = v != 0; pos = np; } }
            (7, 2) => {
                let (sub, np) = read_ld(buf, pos).unwrap_or((&[], buf.len()));
                pos = np;
                item.skill_levels = decode_varint_list(sub);
            }
            _ => { if !skip_field(wire, buf, &mut pos) { break; } }
        }
    }
    item
}

fn encode_buddy_save(item: &BuddyItemSave) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_varint_field(&mut buf, 1, item.id as u64);
    encode_varint_field(&mut buf, 2, item.level as u64);
    encode_varint_field(&mut buf, 3, item.exp as u64);
    encode_varint_field(&mut buf, 4, item.rank as u64);
    encode_varint_field(&mut buf, 5, item.star as u64);
    encode_varint_field(&mut buf, 6, item.favorite as u64);
    if !item.skill_levels.is_empty() {
        let sk_buf: Vec<u8> = item.skill_levels.iter().flat_map(|v| encode_varint(*v as u64)).collect();
        encode_ld(&mut buf, 7, &sk_buf);
    }
    buf
}

fn decode_buddy_save_list(buf: &[u8]) -> Vec<BuddyItemSave> {
    let mut items = Vec::new();
    let mut pos = 0;
    while pos < buf.len() {
        let (tag, new_pos) = read_varint(buf, pos).unwrap_or((0, buf.len()));
        pos = new_pos;
        let field = tag >> 3;
        let wire = tag & 7;
        if field == 1 && wire == 2 {
            if let Some((sub, np)) = read_ld(buf, pos) {
                pos = np;
                items.push(decode_buddy_save(sub));
            } else {
                break;
            }
        } else {
            break;
        }
    }
    items
}

fn encode_buddy_save_list(items: &[BuddyItemSave]) -> Vec<u8> {
    let mut buf = Vec::new();
    for item in items {
        let inner = encode_buddy_save(item);
        encode_ld(&mut buf, 1, &inner);
    }
    buf
}

fn decode_hall_save(buf: &[u8]) -> HallSave {
    let mut hall = HallSave::default();
    let mut pos = 0;
    while pos < buf.len() {
        let (tag, new_pos) = read_varint(buf, pos).unwrap_or((0, buf.len()));
        pos = new_pos;
        let field = tag >> 3;
        if field == 1 {
            if let Some((v, np)) = read_varint(buf, pos) {
                hall.section_id = v as u32;
                pos = np;
            }
        } else {
            if !skip_field(tag & 7, buf, &mut pos) { break; }
        }
    }
    hall
}

fn encode_hall_save(hall: &HallSave) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_varint_field(&mut buf, 1, hall.section_id as u64);
    buf
}

fn read_varint(buf: &[u8], pos: usize) -> Option<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift = 0;
    let mut p = pos;
    loop {
        if p >= buf.len() { return None; }
        let byte = buf[p];
        p += 1;
        result |= ((byte & 0x7f) as u64) << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            break;
        }
    }
    Some((result, p))
}

fn read_ld<'a>(buf: &'a [u8], pos: usize) -> Option<(&'a [u8], usize)> {
    let (len, np) = read_varint(buf, pos)?;
    let end = np + len as usize;
    if end > buf.len() { return None; }
    Some((&buf[np..end], end))
}

fn skip_field(wire: u64, buf: &[u8], pos: &mut usize) -> bool {
    match wire {
        0 => {
            // skip varint
            while *pos < buf.len() && (buf[*pos] & 0x80) != 0 {
                *pos += 1;
            }
            *pos += 1;
            true
        }
        2 => {
            // skip length-delimited
            let (len, np) = read_varint(buf, *pos).unwrap_or((0, *pos));
            *pos = np + len as usize;
            *pos <= buf.len()
        }
        _ => false,
    }
}

fn decode_varint_list(buf: &[u8]) -> Vec<u32> {
    let mut list = Vec::new();
    let mut pos = 0;
    while pos < buf.len() {
        let (v, np) = read_varint(buf, pos).unwrap_or((0, buf.len()));
        pos = np;
        list.push(v as u32);
    }
    list
}

fn encode_varint(value: u64) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut v = value;
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            buf.push(byte | 0x80);
        } else {
            buf.push(byte);
            break;
        }
    }
    buf
}

fn encode_varint_field(buf: &mut Vec<u8>, field: u32, value: u64) {
    let tag = (field << 3) | 0; // wire type 0
    buf.extend(encode_varint(tag as u64));
    buf.extend(encode_varint(value));
}

fn encode_ld(buf: &mut Vec<u8>, field: u32, data: &[u8]) {
    let tag = (field << 3) | 2; // wire type 2
    buf.extend(encode_varint(tag as u64));
    buf.extend(encode_varint(data.len() as u64));
    buf.extend_from_slice(data);
}
