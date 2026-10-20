//! Cut / Copy / Paste decisions of the canvas, without AppKit: which intent a paste sends given
//! what the pasteboard holds, and when the edit items are enabled. The bytes are opaque to the
//! host: presentation validates them (`burnburn_io::clipboard`), and a foreign or damaged payload
//! under our type is reported by presentation, never applied.

use burnburn_presentation::{AppViewModel, CommandKey, UiIntent};
use burnburn_types::DocumentId;

/// The pasteboard type BurnBurn's own copies go under (a private reverse-DNS identifier next to
/// the project type `app.burnburn.project`; declared in the bundle's Info.plist).
pub const CLIPBOARD_TYPE: &str = "app.burnburn.clipboard.v1";

/// The intent of a paste into `document` (`None`: the active one). `pasteboard` is the bytes under
/// [`CLIPBOARD_TYPE`], if the pasteboard holds that type: they win over the controller's own copy
/// (another BurnBurn window or process may have copied since). Without our type the controller's
/// last Copy is pasted.
pub fn paste_intent(document: Option<DocumentId>, in_place: bool, pasteboard: Option<Vec<u8>>) -> UiIntent {
    match pasteboard {
        Some(bytes) => UiIntent::PasteData { document, bytes, in_place },
        None => {
            UiIntent::Invoke { document, command: if in_place { CommandKey::PasteInPlace } else { CommandKey::Paste } }
        }
    }
}

/// Enablement of an Edit item that goes through the responder chain (Cut, Copy, Paste, Paste in
/// Place): the view model decides, and a paste is also possible when the pasteboard holds our
/// type while a document is open.
pub fn edit_item_enabled(command: CommandKey, vm: &AppViewModel, pasteboard_has_ours: bool) -> bool {
    let by_view_model = vm.commands.get(&command).is_some_and(|c| c.enabled);
    match command {
        CommandKey::Paste | CommandKey::PasteInPlace => by_view_model || (pasteboard_has_ours && vm.active.is_some()),
        _ => by_view_model,
    }
}
