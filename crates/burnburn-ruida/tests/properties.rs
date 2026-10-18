//! Property tests: fuzz-ready decoding never panics; decode/encode invariants.
#![allow(clippy::unwrap_used)] // proptest strategies outside #[test] fns

use burnburn_ruida::{
    AbsCoord, BoundsKind, Byte7, Command, Laser, Magic, Power, PowerBound, RelCoord, Speed, U35, decode,
    decode_rd_file, encode, registry_entries, scramble, unscramble,
};
use proptest::prelude::*;

/// Byte streams shaped like RD records: known prefixes or random opcodes with 7-bit payloads.
fn record_like() -> impl Strategy<Value = Vec<u8>> {
    let prefix = prop_oneof![
        (0..registry_entries().len()).prop_map(|i| registry_entries()[i].0.to_vec()),
        (0x80u8..=0xff).prop_map(|b| vec![b]),
    ];
    let record = (prefix, prop::collection::vec(0u8..0x80, 0..14)).prop_map(|(mut p, payload)| {
        p.extend(payload);
        p
    });
    prop::collection::vec(record, 0..40).prop_map(|rs| rs.concat())
}

fn power() -> impl Strategy<Value = Power> {
    (0u16..=Power::MAX_RAW).prop_map(|r| Power::from_raw(r).unwrap())
}
fn rel() -> impl Strategy<Value = RelCoord> {
    (RelCoord::MIN_UM..=RelCoord::MAX_UM).prop_map(|v| RelCoord::from_um(v).unwrap())
}
fn abs() -> impl Strategy<Value = AbsCoord> {
    (0..=AbsCoord::MAX_UM).prop_map(|v| AbsCoord::from_um(v).unwrap())
}
fn byte7() -> impl Strategy<Value = Byte7> {
    (0u8..=127).prop_map(|v| Byte7::new(v).unwrap())
}
fn laser() -> impl Strategy<Value = Laser> {
    prop_oneof![Just(Laser::L1), Just(Laser::L2), Just(Laser::L3), Just(Laser::L4)]
}
fn bound() -> impl Strategy<Value = PowerBound> {
    prop_oneof![Just(PowerBound::Min), Just(PowerBound::Max)]
}

fn typed_command() -> impl Strategy<Value = Command> {
    prop_oneof![
        (abs(), abs()).prop_map(|(x, y)| Command::MoveAbs { x, y }),
        (abs(), abs()).prop_map(|(x, y)| Command::CutAbs { x, y }),
        (rel(), rel()).prop_map(|(dx, dy)| Command::CutRel { dx, dy }),
        rel().prop_map(Command::MoveRelY),
        rel().prop_map(Command::CutRelX),
        (laser(), power()).prop_map(|(slot, value)| Command::ImmediatePower { slot, value }),
        (laser(), bound(), power()).prop_map(|(laser, bound, value)| Command::LaserPower { laser, bound, value }),
        (laser(), bound(), byte7(), power()).prop_map(|(laser, bound, part, value)| Command::PartPower {
            laser,
            bound,
            part,
            value
        }),
        (0..=Speed::MAX_UM_PER_S).prop_map(|v| Command::Speed(Speed::from_um_per_s(v).unwrap())),
        (byte7(), 0..=U35::MAX).prop_map(|(part, c)| Command::LayerColorPart { part, color: U35::new(c).unwrap() }),
        (abs(), abs()).prop_map(|(x, y)| Command::Bounds { kind: BoundsKind::DocumentMax, x, y }),
        byte7().prop_map(Command::ArrayStart),
        Just(Command::BlockEnd),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 2000, ..ProptestConfig::default() })]

    #[test]
    fn decode_never_panics_on_arbitrary_bytes(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        let _ = decode(&bytes);
        let _ = decode_rd_file(&bytes, Magic::FIXTURE_0X88);
    }

    /// Whatever `decode` accepts is covered contiguously and re-encodes to the same bytes
    /// (typed fields are canonical; unknown records are verbatim).
    #[test]
    fn accepted_input_reencodes_identically(bytes in record_like()) {
        if let Ok(records) = decode(&bytes) {
            let mut next = 0;
            for r in &records {
                prop_assert_eq!(r.offset, next);
                next += r.len;
            }
            prop_assert_eq!(next, bytes.len());
            let cmds: Vec<Command> = records.into_iter().map(|r| r.command).collect();
            prop_assert_eq!(encode(&cmds).unwrap(), bytes);
        }
    }

    #[test]
    fn typed_commands_roundtrip(cmds in prop::collection::vec(typed_command(), 0..30)) {
        let bytes = encode(&cmds).unwrap();
        let decoded: Vec<Command> = decode(&bytes).unwrap().into_iter().map(|r| r.command).collect();
        prop_assert_eq!(decoded, cmds);
    }

    #[test]
    fn scramble_roundtrips_for_any_magic(bytes in prop::collection::vec(any::<u8>(), 0..512), magic in any::<u8>()) {
        prop_assert_eq!(unscramble(&scramble(&bytes, Magic(magic)), Magic(magic)), bytes);
    }
}
