//! Typed RD commands and the single spec table shared by decoder and encoder.

use crate::field::{AbsCoord, Byte7, Power, RelCoord, Speed, U35};

/// Laser/channel index as used by the public registry labels (`MIN_POWER_1`, ...).
/// A label does not prove that a second physical laser is active (see the Line fixture).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Laser {
    L1,
    L2,
    L3,
    L4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PowerBound {
    Min,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DelayEdge {
    On,
    Off,
}

/// `CA 01 xx` flags, named after the public registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModeFlag {
    /// `CA 01 00`, labelled LAYER_END upstream; LightBurn also writes it before a Line body.
    LayerEnd,
    /// `CA 01 01`: the raster/scan work mode in the Fill and Grayscale fixtures.
    WorkMode1,
    WorkMode2,
    WorkMode3,
    WorkMode4,
    /// `CA 01 55` (upstream WORK_MODE_5).
    WorkMode5,
    /// `CA 01 05` (upstream WORK_MODE_6).
    WorkMode6,
    LaserDevice0,
    LaserDevice1,
    AirAssistOff,
    AirAssistOn,
    DbHead,
    Laser2Offset0,
    Laser2Offset1,
}

/// Five-byte X/Y bounds records without part index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoundsKind {
    ProcessTopLeft,
    ProcessBottomRight,
    DocumentMin,
    DocumentMax,
    ArrayMin,
    ArrayMax,
}

/// Part bounds: `E7 52/53/61/62 part X5 Y5`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PartBoundsKind {
    Min,
    Max,
    MinEx,
    MaxEx,
}

/// `D8 00..03`. Inside a file `D8 00` is part of the init; sent live it may start a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessAction {
    Start,
    Stop,
    Pause,
    Restore,
}

/// `D8 10/11/12`, kept under their upstream labels; the frame semantics are not confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RefPoint {
    RefPoint2,
    RefPoint1,
    RefPoint0,
}

/// Logical (unscrambled) single-byte replies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplyByte {
    Ack,
    Err,
    Enq,
    Nak,
}

/// One RD record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    MoveAbs {
        x: AbsCoord,
        y: AbsCoord,
    },
    MoveRel {
        dx: RelCoord,
        dy: RelCoord,
    },
    MoveRelX(RelCoord),
    MoveRelY(RelCoord),
    CutAbs {
        x: AbsCoord,
        y: AbsCoord,
    },
    CutRel {
        dx: RelCoord,
        dy: RelCoord,
    },
    CutRelX(RelCoord),
    CutRelY(RelCoord),
    /// `C7/C0/C2/C3 power2` (upstream IMD_POWER_1..4). In the Grayscale fixture these carry
    /// per-run modulation up to 16383 while max power is 60 %: the firmware formula is OPEN.
    ImmediatePower {
        slot: Laser,
        value: Power,
    },
    LaserPower {
        laser: Laser,
        bound: PowerBound,
        value: Power,
    },
    PartPower {
        laser: Laser,
        bound: PowerBound,
        part: Byte7,
        value: Power,
    },
    /// `C6 50/51/55/56 power2`; precise effect OPEN.
    ThroughPower {
        laser: Laser,
        value: Power,
    },
    /// `C6 12/13 time5`; time scale not confirmed (only zero observed).
    LaserDelay {
        edge: DelayEdge,
        raw: U35,
    },
    Speed(Speed),
    PartSpeed {
        part: Byte7,
        speed: Speed,
    },
    Mode(ModeFlag),
    LayerNumberPart(Byte7),
    /// `CA 03 mask`: laser enable/select; multi-channel meaning not confirmed.
    LaserSelect(Byte7),
    LayerColorPart {
        part: Byte7,
        color: U35,
    },
    WorkModePart {
        part: Byte7,
        mode: Byte7,
    },
    MaxLayerPart(Byte7),
    Bounds {
        kind: BoundsKind,
        x: AbsCoord,
        y: AbsCoord,
    },
    PartBounds {
        kind: PartBoundsKind,
        part: Byte7,
        x: AbsCoord,
        y: AbsCoord,
    },
    BlockEnd,
    ArrayStart(Byte7),
    ArrayEnd,
    SetAbsolute,
    Process(ProcessAction),
    Reference(RefPoint),
    /// `E5 05 sum5`: the file sum (see [`crate::file_sum`]).
    FileSum(U35),
    EndOfFile,
    Reply(ReplyByte),
    /// A registry prefix whose payload grammar is not typed here. `bytes` is the whole record,
    /// split at the next high-bit byte; it is preserved verbatim.
    Registered {
        label: &'static str,
        bytes: Vec<u8>,
    },
    /// No known prefix. `bytes` is the whole record (opcode included), preserved verbatim.
    Unknown {
        opcode: u8,
        bytes: Vec<u8>,
    },
}

/// Typed record kinds; each has exactly one prefix in [`SPECS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    MoveAbs,
    MoveRel,
    MoveRelX,
    MoveRelY,
    CutAbs,
    CutRel,
    CutRelX,
    CutRelY,
    ImmediatePower(Laser),
    LaserPower(Laser, PowerBound),
    PartPower(Laser, PowerBound),
    ThroughPower(Laser),
    Delay(DelayEdge),
    Speed,
    PartSpeed,
    Mode(ModeFlag),
    LayerNumberPart,
    LaserSelect,
    LayerColorPart,
    WorkModePart,
    MaxLayerPart,
    Bounds(BoundsKind),
    PartBounds(PartBoundsKind),
    BlockEnd,
    ArrayStart,
    ArrayEnd,
    SetAbsolute,
    Process(ProcessAction),
    Reference(RefPoint),
    FileSum,
    EndOfFile,
    Reply(ReplyByte),
}

impl Kind {
    /// Exact payload length after the prefix.
    pub(crate) fn payload_len(self) -> usize {
        match self {
            Kind::MoveAbs | Kind::CutAbs | Kind::Bounds(_) => 10,
            Kind::MoveRel | Kind::CutRel => 4,
            Kind::MoveRelX | Kind::MoveRelY | Kind::CutRelX | Kind::CutRelY => 2,
            Kind::ImmediatePower(_) | Kind::LaserPower(..) | Kind::ThroughPower(_) => 2,
            Kind::PartPower(..) => 3,
            Kind::Delay(_) | Kind::Speed | Kind::FileSum => 5,
            Kind::PartSpeed | Kind::LayerColorPart => 6,
            Kind::PartBounds(_) => 11,
            Kind::WorkModePart => 2,
            Kind::LayerNumberPart | Kind::LaserSelect | Kind::MaxLayerPart | Kind::ArrayStart => 1,
            Kind::Mode(_)
            | Kind::BlockEnd
            | Kind::ArrayEnd
            | Kind::SetAbsolute
            | Kind::Process(_)
            | Kind::Reference(_)
            | Kind::EndOfFile
            | Kind::Reply(_) => 0,
        }
    }
}

pub(crate) struct Spec {
    pub(crate) prefix: &'static [u8],
    pub(crate) kind: Kind,
}

const fn spec(prefix: &'static [u8], kind: Kind) -> Spec {
    Spec { prefix, kind }
}

use Laser::{L1, L2, L3, L4};
use PowerBound::{Max, Min};

/// The typed subset. Prefixes and payload lengths follow `02-binaire-specificatie.md` §4 and the
/// fixtures; labels come from the public registry.
pub(crate) const SPECS: &[Spec] = &[
    spec(&[0x88], Kind::MoveAbs),
    spec(&[0x89], Kind::MoveRel),
    spec(&[0x8a], Kind::MoveRelX),
    spec(&[0x8b], Kind::MoveRelY),
    spec(&[0xa8], Kind::CutAbs),
    spec(&[0xa9], Kind::CutRel),
    spec(&[0xaa], Kind::CutRelX),
    spec(&[0xab], Kind::CutRelY),
    spec(&[0xc7], Kind::ImmediatePower(L1)),
    spec(&[0xc0], Kind::ImmediatePower(L2)),
    spec(&[0xc2], Kind::ImmediatePower(L3)),
    spec(&[0xc3], Kind::ImmediatePower(L4)),
    spec(&[0xc6, 0x01], Kind::LaserPower(L1, Min)),
    spec(&[0xc6, 0x02], Kind::LaserPower(L1, Max)),
    spec(&[0xc6, 0x21], Kind::LaserPower(L2, Min)),
    spec(&[0xc6, 0x22], Kind::LaserPower(L2, Max)),
    spec(&[0xc6, 0x05], Kind::LaserPower(L3, Min)),
    spec(&[0xc6, 0x06], Kind::LaserPower(L3, Max)),
    spec(&[0xc6, 0x07], Kind::LaserPower(L4, Min)),
    spec(&[0xc6, 0x08], Kind::LaserPower(L4, Max)),
    spec(&[0xc6, 0x31], Kind::PartPower(L1, Min)),
    spec(&[0xc6, 0x32], Kind::PartPower(L1, Max)),
    spec(&[0xc6, 0x41], Kind::PartPower(L2, Min)),
    spec(&[0xc6, 0x42], Kind::PartPower(L2, Max)),
    spec(&[0xc6, 0x35], Kind::PartPower(L3, Min)),
    spec(&[0xc6, 0x36], Kind::PartPower(L3, Max)),
    spec(&[0xc6, 0x37], Kind::PartPower(L4, Min)),
    spec(&[0xc6, 0x38], Kind::PartPower(L4, Max)),
    spec(&[0xc6, 0x50], Kind::ThroughPower(L1)),
    spec(&[0xc6, 0x51], Kind::ThroughPower(L2)),
    spec(&[0xc6, 0x55], Kind::ThroughPower(L3)),
    spec(&[0xc6, 0x56], Kind::ThroughPower(L4)),
    spec(&[0xc6, 0x12], Kind::Delay(DelayEdge::On)),
    spec(&[0xc6, 0x13], Kind::Delay(DelayEdge::Off)),
    spec(&[0xc9, 0x02], Kind::Speed),
    spec(&[0xc9, 0x04], Kind::PartSpeed),
    spec(&[0xca, 0x01, 0x00], Kind::Mode(ModeFlag::LayerEnd)),
    spec(&[0xca, 0x01, 0x01], Kind::Mode(ModeFlag::WorkMode1)),
    spec(&[0xca, 0x01, 0x02], Kind::Mode(ModeFlag::WorkMode2)),
    spec(&[0xca, 0x01, 0x03], Kind::Mode(ModeFlag::WorkMode3)),
    spec(&[0xca, 0x01, 0x04], Kind::Mode(ModeFlag::WorkMode4)),
    spec(&[0xca, 0x01, 0x55], Kind::Mode(ModeFlag::WorkMode5)),
    spec(&[0xca, 0x01, 0x05], Kind::Mode(ModeFlag::WorkMode6)),
    spec(&[0xca, 0x01, 0x10], Kind::Mode(ModeFlag::LaserDevice0)),
    spec(&[0xca, 0x01, 0x11], Kind::Mode(ModeFlag::LaserDevice1)),
    spec(&[0xca, 0x01, 0x12], Kind::Mode(ModeFlag::AirAssistOff)),
    spec(&[0xca, 0x01, 0x13], Kind::Mode(ModeFlag::AirAssistOn)),
    spec(&[0xca, 0x01, 0x14], Kind::Mode(ModeFlag::DbHead)),
    spec(&[0xca, 0x01, 0x30], Kind::Mode(ModeFlag::Laser2Offset0)),
    spec(&[0xca, 0x01, 0x31], Kind::Mode(ModeFlag::Laser2Offset1)),
    spec(&[0xca, 0x02], Kind::LayerNumberPart),
    spec(&[0xca, 0x03], Kind::LaserSelect),
    spec(&[0xca, 0x06], Kind::LayerColorPart),
    spec(&[0xca, 0x22], Kind::MaxLayerPart),
    spec(&[0xca, 0x41], Kind::WorkModePart),
    spec(&[0xe7, 0x03], Kind::Bounds(BoundsKind::ProcessTopLeft)),
    spec(&[0xe7, 0x07], Kind::Bounds(BoundsKind::ProcessBottomRight)),
    spec(&[0xe7, 0x50], Kind::Bounds(BoundsKind::DocumentMin)),
    spec(&[0xe7, 0x51], Kind::Bounds(BoundsKind::DocumentMax)),
    spec(&[0xe7, 0x13], Kind::Bounds(BoundsKind::ArrayMin)),
    spec(&[0xe7, 0x17], Kind::Bounds(BoundsKind::ArrayMax)),
    spec(&[0xe7, 0x52], Kind::PartBounds(PartBoundsKind::Min)),
    spec(&[0xe7, 0x53], Kind::PartBounds(PartBoundsKind::Max)),
    spec(&[0xe7, 0x61], Kind::PartBounds(PartBoundsKind::MinEx)),
    spec(&[0xe7, 0x62], Kind::PartBounds(PartBoundsKind::MaxEx)),
    spec(&[0xe7, 0x00], Kind::BlockEnd),
    spec(&[0xea], Kind::ArrayStart),
    spec(&[0xeb], Kind::ArrayEnd),
    spec(&[0xe6, 0x01], Kind::SetAbsolute),
    spec(&[0xd8, 0x00], Kind::Process(ProcessAction::Start)),
    spec(&[0xd8, 0x01], Kind::Process(ProcessAction::Stop)),
    spec(&[0xd8, 0x02], Kind::Process(ProcessAction::Pause)),
    spec(&[0xd8, 0x03], Kind::Process(ProcessAction::Restore)),
    spec(&[0xd8, 0x10], Kind::Reference(RefPoint::RefPoint2)),
    spec(&[0xd8, 0x11], Kind::Reference(RefPoint::RefPoint1)),
    spec(&[0xd8, 0x12], Kind::Reference(RefPoint::RefPoint0)),
    spec(&[0xe5, 0x05], Kind::FileSum),
    spec(&[0xd7], Kind::EndOfFile),
    spec(&[0xcc], Kind::Reply(ReplyByte::Ack)),
    spec(&[0xcd], Kind::Reply(ReplyByte::Err)),
    spec(&[0xce], Kind::Reply(ReplyByte::Enq)),
    spec(&[0xcf], Kind::Reply(ReplyByte::Nak)),
];

pub(crate) fn spec_for(kind: Kind) -> Option<&'static Spec> {
    SPECS.iter().find(|s| s.kind == kind)
}
