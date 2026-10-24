// The ellipsis rule of the menus (LF16), shared by the headless Windows menu test
// (`tests/menus.rs`) and the native macOS menu test (`tests/native/macos/clipboard_and_recents.rs`),
// which both `include!` this file. A token names an item as `Cmd:<CommandKey>`, `Cam:<CamDialog>`,
// `Open`, `Schikken`, `Other:<name>`, `App:<AppAction>` or `Win:<name>`.

/// What choosing an item does, for the ellipsis rule of the Mac HIG: an item that opens a dialog or
/// sheet that asks for input ends in "…"; an item that only shows (or hides) a window or panel, and
/// an item that acts at once, does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opens {
    Dialog,
    Window,
    Nothing,
}

/// Tokens of the items that open a dialog, a sheet or a file panel.
const DIALOGS: &[&str] = &[
    "Open",
    "Cmd:NewFromTemplate",
    "Other:Import",
    "Cmd:ExportSvg",
    "Cmd:ExportDxf",
    "Cmd:ExportGcode",
    "Cmd:ExportBundle",
    "Cmd:ImportBundle",
    "Cmd:OpenMachineProfile",
    "Cmd:SaveAs",
    "Cmd:SaveAsTemplate",
    "Cmd:SearchProjects",
    "Cmd:ResizeSlots",
    "Cmd:ShapesJoin",
    "Cmd:ShapesRoundCorners",
    "Cmd:ShapesOptimize",
    "Cmd:TextOnPath",
    "Cmd:BarcodeNew",
    "Cmd:GeneratorNew",
    "Cmd:GeneratorEdit",
    "Other:ImportImage",
    "Cmd:AdjustImage",
    "Cmd:ReplaceImage",
    "Cmd:ReplaceImageToFit",
    "Cmd:SaveProcessedBitmap",
    "Other:TraceImage",
    "Cmd:TraceEdit",
    "Cmd:QuickNest",
    "Cmd:NestSelected",
    "Cmd:PrintAndCut",
    "Cmd:CenterFinder",
    "Cmd:ExportRd",
    "Other:ExportDiagnostics",
    "App:Preferences",
    "App:Shortcuts",
    "App:SupportData",
    // Windows with fields of their own (the RD inspector, the machine hub) count as dialogs.
    "App:InspectRd",
    "App:MachineHub",
];

/// Tokens of the items that show or hide a window or panel (no input is asked for).
const WINDOWS: &[&str] = &[
    "Other:LaserWindow",
    "Schikken",
    "Other:OffsetPanel",
    "Other:StructurePanel",
    "Other:TracePrepPanel",
    "Other:OperatorPanel",
    "Other:RemnantsPanel",
    "Other:RecognitionPanel",
    "Cmd:ShowMaterialLibrary",
    "Cmd:ShowArtLibrary",
    "Cmd:ShowNotes",
    "App:Preview",
    "App:History",
    "App:About",
    "Win:FilesPanel",
    "Win:ImagePanel",
    "Win:CameraWindow",
];

fn opens(token: &str) -> Opens {
    if token.starts_with("Cam:") || DIALOGS.contains(&token) {
        Opens::Dialog
    } else if WINDOWS.contains(&token) {
        Opens::Window
    } else {
        Opens::Nothing
    }
}
