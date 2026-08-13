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

fn decode_basic_save(buf: &[u8]) -> BasicSave {
    let mut basic = BasicSave::default();
    let mut pos = 0;
    while pos < buf.len() {
        let (tag, new_pos) = read_varint(buf, pos).unwrap_or((0, buf.len()));
        pos = new_pos;
        let field = tag >> 3;
        match field {
            1 => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    basic.level = v as u32;
                    pos = np;
                }
            }
            2 => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    basic.avatar_id = v as u32;
                    pos = np;
                }
            }
            3 => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    basic.control_avatar_id = v as u32;
                    pos = np;
                }
            }
            4 => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    basic.control_guise_avatar_id = v as u32;
                    pos = np;
                }
            }
            _ => {
                if !skip_field(tag & 7, buf, &mut pos) {
                    break;
                }
            }
        }
    }
    basic
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
            (1, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.id = v as u32;
                    pos = np;
                }
            }
            (2, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.level = v as u32;
                    pos = np;
                }
            }
            (3, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.exp = v as u32;
                    pos = np;
                }
            }
            (4, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.rank = v as u32;
                    pos = np;
                }
            }
            (5, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.talents = v as u32;
                    pos = np;
                }
            }
            (6, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.talent_switch = v as u32;
                    pos = np;
                }
            }
            (7, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.favorite = v != 0;
                    pos = np;
                }
            }
            (8, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.skill_levels.push(v as u32);
                    pos = np;
                }
            }
            (8, 2) => {
                let (sub, np) = read_ld(buf, pos).unwrap_or((&[], buf.len()));
                pos = np;
                item.skill_levels = decode_varint_list(sub);
            }
            (9, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.skin_id = v as u32;
                    pos = np;
                }
            }
            (10, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.weapon_uid = v as u32;
                    pos = np;
                }
            }
            (11, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.equipment_uids.push(v as u32);
                    pos = np;
                }
            }
            (11, 2) => {
                let (sub, np) = read_ld(buf, pos).unwrap_or((&[], buf.len()));
                pos = np;
                item.equipment_uids = decode_varint_list(sub);
            }
            (12, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.awake_available = v != 0;
                    pos = np;
                }
            }
            (13, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.awake_enabled = v != 0;
                    pos = np;
                }
            }
            (14, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.awake_id = v as u32;
                    pos = np;
                }
            }
            (15, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.awake_material_count = v as u32;
                    pos = np;
                }
            }
            _ => {
                if !skip_field(wire, buf, &mut pos) {
                    break;
                }
            }
        }
    }
    item
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
            _ => {
                if !skip_field(tag & 7, buf, &mut pos) {
                    break;
                }
            }
        }
    }
    item
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
            _ => {
                if !skip_field(tag & 7, buf, &mut pos) {
                    break;
                }
            }
        }
    }
    prop
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
            (1, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.uid = v as u32;
                    pos = np;
                }
            }
            (2, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.id = v as u32;
                    pos = np;
                }
            }
            (3, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.level = v as u32;
                    pos = np;
                }
            }
            (4, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.star = v as u32;
                    pos = np;
                }
            }
            (5, 2) => {
                let (sub, np) = read_ld(buf, pos).unwrap_or((&[], buf.len()));
                pos = np;
                item.properties.push(decode_equip_property(sub));
            }
            _ => {
                if !skip_field(wire, buf, &mut pos) {
                    break;
                }
            }
        }
    }
    item
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

fn decode_buddy_save(buf: &[u8]) -> BuddyItemSave {
    let mut item = BuddyItemSave::default();
    let mut pos = 0;
    while pos < buf.len() {
        let (tag, new_pos) = read_varint(buf, pos).unwrap_or((0, buf.len()));
        pos = new_pos;
        let field = tag >> 3;
        let wire = tag & 7;
        match (field, wire) {
            (1, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.id = v as u32;
                    pos = np;
                }
            }
            (2, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.level = v as u32;
                    pos = np;
                }
            }
            (3, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.exp = v as u32;
                    pos = np;
                }
            }
            (4, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.rank = v as u32;
                    pos = np;
                }
            }
            (5, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.star = v as u32;
                    pos = np;
                }
            }
            (6, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.favorite = v != 0;
                    pos = np;
                }
            }
            (7, 0) => {
                if let Some((v, np)) = read_varint(buf, pos) {
                    item.skill_levels.push(v as u32);
                    pos = np;
                }
            }
            (7, 2) => {
                let (sub, np) = read_ld(buf, pos).unwrap_or((&[], buf.len()));
                pos = np;
                item.skill_levels = decode_varint_list(sub);
            }
            _ => {
                if !skip_field(wire, buf, &mut pos) {
                    break;
                }
            }
        }
    }
    item
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
            if !skip_field(tag & 7, buf, &mut pos) {
                break;
            }
        }
    }
    hall
}

fn read_varint(buf: &[u8], pos: usize) -> Option<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift = 0;
    let mut p = pos;
    loop {
        if p >= buf.len() {
            return None;
        }
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
    if end > buf.len() {
        return None;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avatar_skill_levels_decode_repeated_wire0() {
        // Seven repeated field-8 varints (wire type 0): the wire format rmpb
        // actually emits for an ArrayList<u32>. Values: [12,12,12,12,12,7,12].
        let mut item = Vec::new();
        for &v in &[12u32, 12, 12, 12, 12, 7, 12] {
            item.extend_from_slice(&[(8u8 << 3) | 0]); // field 8, wire 0
            item.extend_from_slice(&[v as u8]); // varint < 128
        }
        let save = decode_avatar_save(&item);
        assert_eq!(save.skill_levels, vec![12, 12, 12, 12, 12, 7, 12]);
    }

    #[test]
    fn avatar_skill_levels_decode_packed_wire2() {
        // Packed form (field 8, wire 2) is also accepted for compatibility.
        let values = [12u8, 12, 12, 12, 12, 7, 12];
        let mut item = vec![(8u8 << 3) | 2, values.len() as u8];
        item.extend_from_slice(&values);
        let save = decode_avatar_save(&item);
        assert_eq!(save.skill_levels, vec![12, 12, 12, 12, 12, 7, 12]);
    }

    #[test]
    fn avatar_equipment_uids_decode_repeated_wire0() {
        let mut item = Vec::new();
        for &v in &[1001u32, 2002] {
            item.extend_from_slice(&[(11u8 << 3) | 0]);
            item.extend_from_slice(&varint_bytes(v));
        }
        let save = decode_avatar_save(&item);
        assert_eq!(save.equipment_uids, vec![1001, 2002]);
    }

    fn varint_bytes(mut v: u32) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v != 0 {
                out.push(b | 0x80);
            } else {
                out.push(b);
                break;
            }
        }
        out
    }

    #[test]
    fn equip_save_decodes_repeated_property_fields() {
        // One EquipItemSave from a real USD save: uid=0, id=34241, level=15,
        // star=1, then 5 repeated field-5 (properties) messages, each holding a
        // single EquipProperty (key=1, base_value=2, add_value=3).
        let item: Vec<u8> = vec![
            0x08, 0x00, // uid = 0
            0x10, 0xC1, 0x8B, 0x02, // id = 34241
            0x18, 0x0F, // level = 15
            0x20, 0x01, // star = 1
            // property 1: key=11103 (HP), base=550, add=0
            0x2A, 0x08, 0x08, 0xDF, 0x56, 0x10, 0xA6, 0x04, 0x18, 0x00,
            // property 2: key=20103 (CRIT Rate), base=240, add=6
            0x2A, 0x09, 0x08, 0x87, 0x9D, 0x01, 0x10, 0xF0, 0x01, 0x18, 0x06,
            // property 3: key=13102 (DEF%), base=480, add=1
            0x2A, 0x08, 0x08, 0xAE, 0x66, 0x10, 0xE0, 0x03, 0x18, 0x01,
            // property 4: key=13103 (DEF), base=15, add=1
            0x2A, 0x07, 0x08, 0xAF, 0x66, 0x10, 0x0F, 0x18, 0x01,
            // property 5: key=23203 (PEN), base=9, add=1
            0x2A, 0x08, 0x08, 0xA3, 0xB5, 0x01, 0x10, 0x09, 0x18, 0x01,
        ];
        let save = decode_equip_save(&item);
        assert_eq!(save.uid, 0);
        assert_eq!(save.id, 34241);
        assert_eq!(save.level, 15);
        assert_eq!(save.star, 1);
        assert_eq!(save.properties.len(), 5);
        assert_eq!(save.properties[0].key, 11103);
        assert_eq!(save.properties[0].base_value, 550);
        assert_eq!(save.properties[0].add_value, 0);
        assert_eq!(save.properties[1].key, 20103);
        assert_eq!(save.properties[1].base_value, 240);
        assert_eq!(save.properties[1].add_value, 6);
        assert_eq!(save.properties[2].key, 13102);
        assert_eq!(save.properties[3].key, 13103);
        assert_eq!(save.properties[4].key, 23203);
    }

    #[test]
    fn full_save_decodes_equip_properties() {
        let data = std::fs::read("tests/fixtures_USD_100.bin").unwrap();
        let save = decode_player_save(&data).expect("decode full save");
        assert_eq!(save.equip.len(), 8);
        for item in &save.equip {
            assert_eq!(item.properties.len(), 5, "equip id {} must have 5 properties", item.id);
            assert!(item.properties[0].key > 0, "main property key must be set");
        }
    }
}
