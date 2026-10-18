//! Live (non-file) commands of the laser window: jog key events, home, pause/resume/stop and
//! controller memory reads. Wire forms come only from the public registry; the oracle here is
//! the research JSON itself (looked up by symbol name), not the embedded table.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::PathBuf;

use burnburn_ruida::{
    HomeTarget, JogKey, LiveCommand, LiveEncodeError, LiveProcess, MemoryAddress, WireEvidence, encode_live,
};

fn json() -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../research/lightburn/ruida/command-registry.json");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn hex_of(list: &str, name: &str) -> Vec<u8> {
    let json = json();
    let entry = json[list]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("{name} not in registry {list}"));
    entry["hex"].as_str().unwrap().split(' ').map(|h| u8::from_str_radix(h, 16).unwrap()).collect()
}

#[test]
fn jog_key_events_use_the_registry_keydown_and_keyup_bytes() {
    let keys = [
        (JogKey::XLeft, "X_LEFT"),
        (JogKey::XRight, "X_RIGHT"),
        (JogKey::YTop, "Y_TOP"),
        (JogKey::YBottom, "Y_BOTTOM"),
        (JogKey::ZUp, "Z_UP"),
        (JogKey::ZDown, "Z_DOWN"),
        (JogKey::UForward, "U_FORWARD"),
        (JogKey::UBackwards, "U_BACKWARDS"),
    ];
    for (key, suffix) in keys {
        assert_eq!(encode_live(&LiveCommand::KeyDown(key)).unwrap(), hex_of("commands", &format!("KEYDOWN_{suffix}")));
        assert_eq!(encode_live(&LiveCommand::KeyUp(key)).unwrap(), hex_of("commands", &format!("KEYUP_{suffix}")));
    }
}

#[test]
fn home_and_process_controls_use_the_registry_bytes() {
    let cases = [
        (LiveCommand::Home(HomeTarget::Xy), "HOME_XY"),
        (LiveCommand::Home(HomeTarget::Z), "HOME_Z"),
        (LiveCommand::Home(HomeTarget::U), "HOME_U"),
        (LiveCommand::Process(LiveProcess::Pause), "PAUSE_PROCESS"),
        (LiveCommand::Process(LiveProcess::Restore), "RESTORE_PROCESS"),
        (LiveCommand::Process(LiveProcess::Stop), "STOP_PROCESS"),
    ];
    for (cmd, name) in cases {
        assert_eq!(encode_live(&cmd).unwrap(), hex_of("commands", name), "{name}");
    }
}

#[test]
fn start_process_is_not_a_live_command() {
    // Starting a job stays the stored-job start request; D8 00 inside a live stream must not be
    // reachable from the laser window's jog/pause vocabulary.
    let start = hex_of("commands", "START_PROCESS");
    let all = [
        LiveCommand::Process(LiveProcess::Pause),
        LiveCommand::Process(LiveProcess::Restore),
        LiveCommand::Process(LiveProcess::Stop),
        LiveCommand::Home(HomeTarget::Xy),
        LiveCommand::KeyUp(JogKey::XLeft),
    ];
    assert!(all.iter().all(|c| encode_live(c).unwrap() != start));
}

#[test]
fn memory_reads_are_get_setting_with_the_two_address_bytes() {
    let get = hex_of("commands", "GET_SETTING");
    for (addr, name) in [
        (MemoryAddress::CurrentX, "MEM_CURRENT_X"),
        (MemoryAddress::CurrentY, "MEM_CURRENT_Y"),
        (MemoryAddress::CurrentZ, "MEM_CURRENT_Z"),
        (MemoryAddress::MachineStatus, "MEM_MACHINE_STATUS"),
        (MemoryAddress::CardId, "MEM_CARD_ID"),
    ] {
        let mut want = get.clone();
        want.extend(hex_of("memory_addresses", name));
        assert_eq!(encode_live(&LiveCommand::ReadMemory(addr)).unwrap(), want, "{name}");
    }
}

#[test]
fn live_commands_are_only_registry_evidenced_never_hardware_evidenced() {
    assert_eq!(LiveCommand::KeyDown(JogKey::XLeft).evidence(), WireEvidence::PublicRegistry);
    assert_eq!(LiveCommand::ReadMemory(MemoryAddress::CurrentX).evidence(), WireEvidence::PublicRegistry);
}

#[test]
fn encoded_live_commands_start_with_a_high_bit_opcode_and_keep_payload_bytes_7_bit() {
    for cmd in [
        LiveCommand::KeyDown(JogKey::ZDown),
        LiveCommand::Home(HomeTarget::Z),
        LiveCommand::ReadMemory(MemoryAddress::CurrentU),
    ] {
        let bytes = encode_live(&cmd).unwrap();
        assert!(bytes[0] & 0x80 != 0, "{cmd:?}");
        // Opcode byte(s) have the high bit; the 2-byte memory address of a read must not.
        if let LiveCommand::ReadMemory(_) = cmd {
            assert!(bytes[bytes.len() - 2..].iter().all(|b| b & 0x80 == 0));
        }
    }
}

#[test]
fn encode_error_names_the_missing_label() {
    // The error type exists so a registry drift is a typed failure, not a panic.
    let e = LiveEncodeError::NotInRegistry("NOPE");
    assert_eq!(format!("{e}"), "label NOPE is not in the public registry");
}
