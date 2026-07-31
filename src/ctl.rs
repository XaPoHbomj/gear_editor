use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const CTL_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_secs(1);
const PROBE_CACHE_TTL: Duration = Duration::from_secs(10);

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
    send_and_ack(addr, &buf[..p])
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
    send_and_ack(addr, &buf[..p])
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
    send_and_ack(addr, &buf[..p])
}

pub fn delete_weapon(addr: &str, player_uid: u32, weapon_uid_in_save: u32) -> Result<(), String> {
    // header(8) + player_uid(4) + weapon_uid(4) = 16
    let mut buf = [0u8; 16];
    buf[..HEADER_SIZE].copy_from_slice(&make_header(8, 0));
    let mut p = HEADER_SIZE;
    write_u32_le(&mut buf, &mut p, player_uid);
    write_u32_le(&mut buf, &mut p, WEAPON_UID_BASE + weapon_uid_in_save);
    send_and_ack(addr, &buf[..p])
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
    send_and_ack(addr, &buf[..p])
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
    send_and_ack(addr, &buf[..p])
}

pub fn delete_equip(addr: &str, player_uid: u32, equip_uid_in_save: u32) -> Result<(), String> {
    // header(8) + player_uid(4) + equip_uid(4) = 16
    let mut buf = [0u8; 16];
    buf[..HEADER_SIZE].copy_from_slice(&make_header(9, 0));
    let mut p = HEADER_SIZE;
    write_u32_le(&mut buf, &mut p, player_uid);
    write_u32_le(&mut buf, &mut p, EQUIP_UID_BASE + equip_uid_in_save);
    send_and_ack(addr, &buf[..p])
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Presence {
    Online,
    Offline,
    Unreachable,
}

type ProbeKey = (String, u32);

static PROBE_CACHE: OnceLock<Mutex<HashMap<ProbeKey, (Presence, Instant)>>> = OnceLock::new();

fn probe_cache() -> &'static Mutex<HashMap<ProbeKey, (Presence, Instant)>> {
    PROBE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Returns the cached presence for (addr, player_uid) if it is still fresh.
fn cached_presence(addr: &str, player_uid: u32) -> Option<Presence> {
    let cache = probe_cache().lock().unwrap();
    cache
        .get(&(addr.to_string(), player_uid))
        .filter(|(_, at)| at.elapsed() < PROBE_CACHE_TTL)
        .map(|(presence, _)| *presence)
}

fn store_presence(addr: &str, player_uid: u32, presence: Presence) {
    let mut cache = probe_cache().lock().unwrap();
    cache.insert(
        (addr.to_string(), player_uid),
        (presence, Instant::now()),
    );
}

fn probe_presence(addr: &str, player_uid: u32) -> Presence {
    // CreateWeapon with count = 0. On a live server this is a no-op and the
    // player is online iff we get an ACK; offline players yield NAK no_entry;
    // a silent socket means the server is unreachable.
    let mut buf = [0u8; 16];
    buf[..HEADER_SIZE].copy_from_slice(&make_header(3, 0));
    let mut p = HEADER_SIZE;
    write_u32_le(&mut buf, &mut p, player_uid);
    write_u32_le(&mut buf, &mut p, 0); // count = 0
    let data = &buf[..p];

    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return Presence::Unreachable,
    };
    if socket.set_read_timeout(Some(PROBE_TIMEOUT)).is_err() {
        return Presence::Unreachable;
    }
    if socket.send_to(data, addr).is_err() {
        return Presence::Unreachable;
    }

    let mut rsp = [0u8; 16];
    match socket.recv_from(&mut rsp) {
        Ok((n, _)) if n >= 16 => {
            let tag = u16::from_le_bytes([rsp[2], rsp[3]]);
            if tag == 0 {
                Presence::Online
            } else if tag == 1 {
                let reason = u32::from_le_bytes([rsp[8], rsp[9], rsp[10], rsp[11]]);
                if reason == 5 {
                    Presence::Offline
                } else {
                    Presence::Unreachable
                }
            } else {
                Presence::Unreachable
            }
        }
        _ => Presence::Unreachable,
    }
}

/// Check whether the player is currently online on the given server, using a
/// short-lived cache (10s) to avoid hammering every server on each page load.
pub fn player_is_online(addr: &str, player_uid: u32) -> bool {
    presence_of(addr, player_uid) == Presence::Online
}

/// Presence of the player on the given server, cached for PROBE_CACHE_TTL.
pub fn presence_of(addr: &str, player_uid: u32) -> Presence {
    if let Some(presence) = cached_presence(addr, player_uid) {
        return presence;
    }
    let presence = probe_presence(addr, player_uid);
    store_presence(addr, player_uid, presence);
    presence
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::UdpSocket;

    fn server_handle() -> (UdpSocket, String) {
        let s = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = s.local_addr().unwrap().to_string();
        (s, addr)
    }

    #[test]
    fn probe_classifies_online_offline_unreachable() {
        // ACK (online)
        {
            let (srv, addr) = server_handle();
            let t = std::thread::spawn(move || {
                let mut buf = [0u8; 16];
                let (n, peer) = srv.recv_from(&mut buf).unwrap();
                // echo an ACK: header(8) + event(8). bytes 2-3 = event_tag = 0
                let mut rsp = [0u8; 16];
                rsp[2] = 0;
                rsp[3] = 0;
                rsp[4] = buf[4];
                rsp[5] = buf[5];
                rsp[6] = buf[6];
                rsp[7] = buf[7];
                srv.send_to(&rsp[..n.min(16)], peer).unwrap();
            });
            let presence = probe_presence(&addr, 1);
            t.join().unwrap();
            assert_eq!(presence, Presence::Online);
        }
        // NAK no_entry (offline)
        {
            let (srv, addr) = server_handle();
            let t = std::thread::spawn(move || {
                let mut buf = [0u8; 16];
                let (n, peer) = srv.recv_from(&mut buf).unwrap();
                let mut rsp = [0u8; 16];
                rsp[2] = 1;
                rsp[3] = 0;
                rsp[8] = 5; // reason = no_entry
                rsp[4] = buf[4];
                rsp[5] = buf[5];
                rsp[6] = buf[6];
                rsp[7] = buf[7];
                srv.send_to(&rsp[..n.min(16)], peer).unwrap();
            });
            let presence = probe_presence(&addr, 1);
            t.join().unwrap();
            assert_eq!(presence, Presence::Offline);
        }
        // No listener -> unreachable
        {
            let (_srv, addr) = server_handle();
            let presence = probe_presence(&addr, 1);
            assert_eq!(presence, Presence::Unreachable);
        }
    }
}
