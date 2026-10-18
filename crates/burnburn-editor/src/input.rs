use burnburn_geometry::Point;
use burnburn_types::MonotonicTime;

/// Which pointer produced an event. 0 is the mouse/trackpad; touch and pen get their own ids later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PointerId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerPhase {
    Down,
    /// Drag (button held) or hover (no button). Hover moves update snap candidates only.
    Move,
    Up,
    /// The OS took the pointer (focus loss, capture lost). The gesture must be abandoned.
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    /// Right click, or Ctrl-click on macOS (translated by the host).
    Secondary,
    Middle,
}

/// Normalised modifiers. `command` is the platform's primary shortcut modifier: Cmd on macOS,
/// Ctrl on Windows. `control` is the Mac Control key (unused on Windows). `alt` is Option on macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub command: bool,
    pub control: bool,
}

/// Pointer input for the editor, in **world millimetres** (contract §4, DR-04). The host sends
/// logical canvas points to presentation, which applies the window's camera; tools never see
/// screen coordinates. Screen tolerances reach tools as `ToolContext::mm_per_point`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerEvent {
    pub pointer: PointerId,
    pub phase: PointerPhase,
    /// `None` for a hover move.
    pub button: Option<PointerButton>,
    pub position: Point,
    pub modifiers: Modifiers,
    /// 2 on the second press of a double click.
    pub click_count: u8,
    pub time: MonotonicTime,
}

impl PointerEvent {
    /// A primary-button mouse event without modifiers.
    pub fn mouse(phase: PointerPhase, position: Point, time: MonotonicTime) -> Self {
        PointerEvent {
            pointer: PointerId(0),
            phase,
            button: Some(PointerButton::Primary),
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
            time,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CanvasKey {
    Escape,
    Delete,
    /// Return/Enter: finish a polyline or pen path.
    Return,
    /// Remove the last placed point of a polyline or pen path.
    Backspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyPhase {
    Down,
    Up,
}

/// Everything a tool reacts to. Camera input (scroll, magnify, resize) is window state and stays
/// in presentation. Text entry goes through native fields/IME, not here.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum CanvasInput {
    Pointer(PointerEvent),
    Key { key: CanvasKey, modifiers: Modifiers, phase: KeyPhase },
}
