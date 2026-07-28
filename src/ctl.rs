use std::net::UdpSocket;
use std::time::Duration;

const CTL_TIMEOUT: Duration = Duration::from_secs(5);

fn write_u8(buf: &mut [u8], pos: &mut usize, val: u8) {
    buf[*pos] = val;
    *pos += 1;
}

fn write_u16_le(buf: &mut [u8], pos: &mut usize, val: u16) {
    buf[*pos..*pos + 2].copy_from_slice(&val.to_le_bytes());
    *pos += 2;
}

fn write_u32_le(buf: &mut [u8], pos: &mut usize, val: u32) {
    buf[*pos..*pos + 4].copy_from_slice(&val.to_le_bytes());
    *pos += 4;
}

fn write_u64_le(buf: &mut [u8], pos: &mut usize, val: u64) {
    buf[*pos..*pos + 8].copy_from_slice(&val.to_le_bytes());
    *pos += 8;
}

fn nak_reason_name(reason: u32) -> &'static str {
    match reason {
        1 => "protocol_version_mismatch",
        2 => "operation_version_mismatch",
        3 => "unknown_operation_tag",
        4 => "invalid_parameter",
        5 => "no_entry (player not online or entity not found)",
        6 => "no_space_left",
        _ => "unknown",
    }
}

fn send_and_ack(addr: &str, data: &[u8]) -> Result<(), String> {
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("bind: {e}"))?;
    socket
        .set_read_timeout(Some(CTL_TIMEOUT))
        .map_err(|e| format!("set_read_timeout: {e}"))?;
    socket
        .send_to(data, addr)
        .map_err(|e| format!("send: {e}"))?;
    let mut buf = [0u8; 16];
    let (n, _src) = socket
        .recv_from(&mut buf)
        .map_err(|e| format!("recv (ack): {e}"))?;
    if n < 8 {
        return Err(format!("short response: {} bytes", n));
    }
    let tag = u16::from_le_bytes([buf[2], buf[3]]);
    if tag == 1 {
        if n < 16 {
            return Err(format!("short NAK response: {} bytes", n));
        }
        let reason = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
        let extra = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
        let reason_name = nak_reason_name(reason);
        return Err(format!("NAK {reason_name} (code={reason}) extra={extra}"));
    }
    if tag != 0 {
        return Err(format!("unexpected event tag={tag}"));
    }
    Ok(())
}

const HEADER_SIZE: usize = 8;

fn make_header(operation_tag: u16, operation_version: u8) -> [u8; HEADER_SIZE] {
    let mut h = [0u8; HEADER_SIZE];
    let mut p = 0;
    write_u8(&mut h, &mut p, 0); // protocol_version
    write_u8(&mut h, &mut p, operation_version);
    write_u16_le(&mut h, &mut p, operation_tag);
    write_u32_le(&mut h, &mut p, rand::random::<u32>());
    h
}

fn pack_weapon_meta(level: u8, star: u8, refine: u8) -> u16 {
    (level as u16) | ((star as u16) << 6) | ((refine as u16) << 9)
}

fn pack_equip_meta(level: u8, star: u8) -> u16 {
    (level as u16) | ((star as u16) << 4)
}

fn pack_equip_property(key: u16, base_value: u16, add_value: u8) -> u32 {
    (key as u32) | ((base_value as u32) << 16) | ((add_value as u32) << 28)
}

pub fn mod_avatar_meta(
    addr: &str,
    player_uid: u32,
    avatar_id: u32,
    field: u8,
    value: u64,
) -> Result<(), String> {
    // header(8) + value(8) + player_uid(4) + avatar_id(4) + field(1) = 25
    let mut buf = [0u8; 25];
    buf[..HEADER_SIZE].copy_from_slice(&make_header(2, 0));
    let mut p = HEADER_SIZE;
    write_u64_le(&mut buf, &mut p, value);
    write_u32_le(&mut buf, &mut p, player_uid);
    write_u32_le(&mut buf, &mut p, avatar_id);
    write_u8(&mut buf, &mut p, field);
    broadcast(addr, &buf[..p])
}

const WEAPON_UID_BASE: u32 = 0x01_00_00;
const EQUIP_UID_BASE: u32 = 0x02_00_00;

pub fn create_weapon(
    addr: &str,
    player_uid: u32,
    item_id: u16,
    level: u8,
    star: u8,
    refine: u8,
) -> Result<(), String> {
    // header(8) + player_uid(4) + count(4) + entry: id(2) + meta(2) = 20
    let mut buf = [0u8; 20];
    buf[..HEADER_SIZE].copy_from_slice(&make_header(3, 0));
    let mut p = HEADER_SIZE;
    write_u32_le(&mut buf, &mut p, player_uid);
    write_u32_le(&mut buf, &mut p, 1); // count = 1
    write_u16_le(&mut buf, &mut p, item_id);
    write_u16_le(&mut buf, &mut p, pack_weapon_meta(level, star, refine));
    broadcast(addr, &buf[..p])
}

pub fn mod_weapon(
    addr: &str,
    player_uid: u32,
    weapon_uid_in_save: u32,
    level: u8,
    star: u8,
    refine: u8,
) -> Result<(), String> {
    // header(8) + player_uid(4) + weapon_uid(4) + meta(2) = 18
    let mut buf = [0u8; 18];
    buf[..HEADER_SIZE].copy_from_slice(&make_header(6, 0));
    let mut p = HEADER_SIZE;
    write_u32_le(&mut buf, &mut p, player_uid);
    write_u32_le(&mut buf, &mut p, WEAPON_UID_BASE + weapon_uid_in_save);
    write_u16_le(&mut buf, &mut p, pack_weapon_meta(level, star, refine));
    broadcast(addr, &buf[..p])
}

pub fn delete_weapon(addr: &str, player_uid: u32, weapon_uid_in_save: u32) -> Result<(), String> {
    // header(8) + player_uid(4) + weapon_uid(4) = 16
    let mut buf = [0u8; 16];
    buf[..HEADER_SIZE].copy_from_slice(&make_header(8, 0));
    let mut p = HEADER_SIZE;
    write_u32_le(&mut buf, &mut p, player_uid);
    write_u32_le(&mut buf, &mut p, WEAPON_UID_BASE + weapon_uid_in_save);
    broadcast(addr, &buf[..p])
}

pub fn create_equip(
    addr: &str,
    player_uid: u32,
    item_id: u16,
    level: u8,
    star: u8,
    properties: &[(u16, u16, u8); 5],
) -> Result<(), String> {
    // header(8) + player_uid(4) + count(4) + entry: props(20) + id(2) + meta(2) = 40
    let mut buf = [0u8; 40];
    buf[..HEADER_SIZE].copy_from_slice(&make_header(4, 0));
    let mut p = HEADER_SIZE;
    write_u32_le(&mut buf, &mut p, player_uid);
    write_u32_le(&mut buf, &mut p, 1); // count = 1
    for &(key, base, add) in properties {
        write_u32_le(&mut buf, &mut p, pack_equip_property(key, base, add));
    }
    write_u16_le(&mut buf, &mut p, item_id);
    write_u16_le(&mut buf, &mut p, pack_equip_meta(level, star));
    broadcast(addr, &buf[..p])
}

pub fn mod_equip(
    addr: &str,
    player_uid: u32,
    equip_uid_in_save: u32,
    level: u8,
    star: u8,
    properties: &[(u16, u16, u8); 5],
) -> Result<(), String> {
    // header(8) + player_uid(4) + equip_uid(4) + level(1) + star(1) + props(20) = 38
    let mut buf = [0u8; 38];
    buf[..HEADER_SIZE].copy_from_slice(&make_header(7, 0));
    let mut p = HEADER_SIZE;
    write_u32_le(&mut buf, &mut p, player_uid);
    write_u32_le(&mut buf, &mut p, EQUIP_UID_BASE + equip_uid_in_save);
    write_u8(&mut buf, &mut p, level);
    write_u8(&mut buf, &mut p, star);
    for &(key, base, add) in properties {
        write_u32_le(&mut buf, &mut p, pack_equip_property(key, base, add));
    }
    broadcast(addr, &buf[..p])
}

pub fn delete_equip(addr: &str, player_uid: u32, equip_uid_in_save: u32) -> Result<(), String> {
    // header(8) + player_uid(4) + equip_uid(4) = 16
    let mut buf = [0u8; 16];
    buf[..HEADER_SIZE].copy_from_slice(&make_header(9, 0));
    let mut p = HEADER_SIZE;
    write_u32_le(&mut buf, &mut p, player_uid);
    write_u32_le(&mut buf, &mut p, EQUIP_UID_BASE + equip_uid_in_save);
    broadcast(addr, &buf[..p])
}

/// Send to all 3 servers (port, port+1, port+2). Returns Ok if at least one ACKs.
/// Ignores no_entry (reason=5, player not online on that server).
fn broadcast(base_addr: &str, data: &[u8]) -> Result<(), String> {
    let (host, port_str) = base_addr
        .rsplit_once(':')
        .ok_or_else(|| "no port in address".to_string())?;
    let base_port: u32 = port_str.parse().map_err(|_| "invalid port".to_string())?;
    let mut last_err = String::new();
    for offset in 0..3 {
        let addr = format!("{}:{}", host, base_port + offset);
        match send_and_ack(&addr, data) {
            Ok(()) => return Ok(()),
            Err(e) => {
                // no_entry (code=5) means player not on this server — expected
                if !e.contains("code=5") {
                    last_err = e;
                } else if last_err.is_empty() {
                    last_err = e;
                }
            }
        }
    }
    Err(last_err)
}

pub fn mod_hadal_entrance(addr: &str, entrance_id: u32, zone_id: u32) -> Result<(), String> {
    // header(8) + count(4) + entry: entrance_id(4) + zone_id(4) = 20
    let mut buf = [0u8; 20];
    buf[..HEADER_SIZE].copy_from_slice(&make_header(5, 0));
    let mut p = HEADER_SIZE;
    write_u32_le(&mut buf, &mut p, 1); // count = 1
    write_u32_le(&mut buf, &mut p, entrance_id);
    write_u32_le(&mut buf, &mut p, zone_id);
    send_and_ack(addr, &buf[..p])
}

pub fn ctl_addr_for_server(base_addr: &str, server_num: u32) -> Result<String, String> {
    let (host, port_str) = base_addr
        .rsplit_once(':')
        .ok_or_else(|| "no port in address".to_string())?;
    let host = host.to_string();
    let port: u32 = port_str.parse().map_err(|_| "invalid port".to_string())?;
    let base_port = port
        .checked_sub(1)
        .ok_or_else(|| "port underflow".to_string())?;
    let new_port = base_port
        .checked_add(server_num)
        .ok_or_else(|| "port overflow".to_string())?;
    Ok(format!("{}:{}", host, new_port))
}
