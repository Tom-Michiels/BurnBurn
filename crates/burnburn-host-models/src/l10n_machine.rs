//! Texts of the machine hub window (G5-machine-rest), NL/EN. Own file next to `l10n`: the hub is
//! macOS-only until the Windows host has one. `{0}`, `{1}` … are filled by [`fill`].

use crate::l10n::Locale;

macro_rules! texts {
    ($($name:ident => ($nl:expr, $en:expr)),* $(,)?) => {
        /// A text only the machine hub shows.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum MachineText {
            $($name),*
        }

        impl MachineText {
            /// Every text, for the completeness test.
            pub const ALL: &'static [MachineText] = &[$(MachineText::$name),*];
        }

        fn pair(text: MachineText) -> (&'static str, &'static str) {
            match text {
                $(MachineText::$name => ($nl, $en)),*
            }
        }
    };
}

texts! {
    HubTitle => ("Machine en productie", "Machine and Production"),
    TabDevice => ("Apparaat", "Device"),
    TabProduction => ("Productie", "Production"),
    TabHistory => ("Historie", "History"),
    TabCalibration => ("Kalibratie", "Calibration"),
    // --- Device.
    DeviceLabel => ("Actief apparaat:", "Active device:"),
    DeviceItem => ("{0} (revisie {1}, {2} × {3} mm)", "{0} (revision {1}, {2} × {3} mm)"),
    // What the user declared about the controller (M01); never what a controller replied.
    DeviceDeclaredBoth => (
        " — model {0}, firmware {1} (door jou opgegeven)",
        " — model {0}, firmware {1} (declared by you)"
    ),
    DeviceDeclaredModel => (" — model {0} (door jou opgegeven)", " — model {0} (declared by you)"),
    DeviceDeclaredFirmware => (" — firmware {0} (door jou opgegeven)", " — firmware {0} (declared by you)"),
    NoDevices => (
        "Nog geen machineprofiel: maak er een in Apparaatinstellingen.",
        "No machine profile yet: create one in Device Settings."
    ),
    DeviceNote => (
        "Een vervangen apparaat blijft bewaard in de profielbibliotheek en staat hier in de lijst. Kiezen maakt eerder gecompileerde plannen verouderd.",
        "A replaced device stays in the profile library and is listed here. Choosing one makes plans compiled earlier out of date."
    ),
    SupportHeader => (
        "Ondersteuning per functie (dezelfde matrix als de compiler):",
        "Support per feature (the same matrix the compiler reads):"
    ),
    // --- Production: upload.
    UploadHeader => ("Upload", "Upload"),
    UploadNone => ("Geen upload bezig.", "No upload in progress."),
    UploadVerifying => ("De controller wordt gecontroleerd…", "Checking the controller…"),
    UploadSending => (
        "Pakketten ontvangen (transport): {0} van {1}. Nog niet opgeslagen.",
        "Packets received (transport): {0} of {1}. Not stored yet."
    ),
    UploadAcked => (
        "Alle {0} pakketten zijn ontvangen (transport). Opslag is nog niet bevestigd.",
        "All {0} packets were received (transport). Storage is not confirmed yet."
    ),
    UploadStored => (
        "Opgeslagen op de controller (bevestigd door terugleesactie).",
        "Stored on the controller (confirmed by read-back)."
    ),
    UploadStopped => (
        "Upload gestopt. Een deel van het bestand kan op de controller staan.",
        "Upload stopped. Part of the file may remain on the controller."
    ),
    FailIdentityMismatch => (
        "De controller is een ander model dan het profiel: er is niets verstuurd.",
        "The controller is another model than the profile: nothing was sent."
    ),
    FailIdentityUnconfirmed => (
        "De controller antwoordde niet op de identiteitsvraag: er is niets verstuurd.",
        "The controller did not answer the identity query: nothing was sent."
    ),
    FailRejected => (
        "De controller weigerde pakket {0}. Een reden meldt de controller niet: volle opslag en een naamconflict zijn niet te onderscheiden.",
        "The controller refused packet {0}. The controller gives no reason: full storage and a name conflict cannot be told apart."
    ),
    FailNotStored => (
        "De controller bevestigt niet dat de job is opgeslagen.",
        "The controller does not confirm that the job is stored."
    ),
    FailFault => ("De controller meldde een fout.", "The controller reported a fault."),
    StopUpload => ("Stop upload", "Stop Upload"),
    // --- Production: queue.
    QueueHeader => ("Wachtrij en aantallen", "Queue and counts"),
    QueueCounts => (
        "Bezig met uploaden: {0} · Opgeslagen, niet gestart: {1} · Gestart: {2} · Voltooid: {3} · Onbekend: {4} · Mislukt of gestopt: {5}",
        "Uploading: {0} · Stored, not started: {1} · Started: {2} · Completed: {3} · Unknown: {4} · Failed or stopped: {5}"
    ),
    QueueNote => (
        "Alleen voltooide uitvoeringen tellen als gemaakt werkstuk; een upload of een opgeslagen job telt niet.",
        "Only completed executions count as a made piece; an upload or a stored job does not."
    ),
    QueueNone => ("Nog geen uitvoeringen.", "No executions yet."),
    QueueRow => ("{0} — {1}", "{0} — {1}"),
    QueueOrder => ("{0} — {1} (order {2})", "{0} — {1} (order {2})"),
    StageUploading => ("bezig met uploaden", "uploading"),
    StageStored => ("opgeslagen, niet gestart", "stored, not started"),
    StageStartRequested => ("start aangevraagd", "start requested"),
    StageRunning => ("loopt", "running"),
    StageCompleted => ("voltooid", "completed"),
    StageFailed => ("mislukt", "failed"),
    StageCancelled => ("gestopt", "stopped"),
    StageUnknown => ("onbekend — controleer de machine", "unknown — check the machine"),
    // --- History and repeat.
    HistoryHeader => ("Historie (nieuwste eerst)", "History (newest first)"),
    HistoryNone => ("Nog niets uitgevoerd.", "Nothing was run yet."),
    HistoryRow => (
        "{0} — {1} — profiel {2} r{3} — {4} bytes — schatting: {5} — gemeten: {6}",
        "{0} — {1} — profile {2} r{3} — {4} bytes — estimate: {5} — measured: {6}"
    ),
    NoEstimate => ("geen", "none"),
    NotMeasured => ("niet gemeten", "not measured"),
    Seconds => ("{0} s", "{0} s"),
    RepeatButton => ("Herhaal…", "Repeat…"),
    RepeatConfirm => ("Zet opgeslagen job klaar", "Prepare Stored Job"),
    RepeatCancel => ("Annuleer", "Cancel"),
    RepeatHeader => (
        "Herhalen met de opgeslagen snapshot van {0}; verschillen met de recepten van het document nu:",
        "Repeating with the stored snapshot of {0}; differences with the document's recipes now:"
    ),
    RepeatNoChanges => ("Geen verschillen.", "No differences."),
    RepeatProfileChanged => (
        "Het machineprofiel is niet meer dat van de opgeslagen job: herhalen is geweigerd.",
        "The machine profile is not the one the stored job was made for: repeating is refused."
    ),
    RepeatNeedsWindow => (
        "Open een documentvenster om de opgeslagen job in klaar te zetten.",
        "Open a document window to prepare the stored job in."
    ),
    RepeatNote => (
        "Klaarzetten uploadt niets: de job verschijnt als jobkaart van het venster en uploaden is een aparte stap die een nieuwe uitvoering maakt.",
        "Preparing uploads nothing: the job appears as the window's job card and uploading is a separate step that makes a new execution."
    ),
    FieldName => ("naam", "name"),
    FieldMode => ("modus", "mode"),
    FieldSpeed => ("snelheid (mm/s)", "speed (mm/s)"),
    FieldPowerMin => ("minimaal vermogen (%)", "minimum power (%)"),
    FieldPowerMax => ("maximaal vermogen (%)", "maximum power (%)"),
    FieldPasses => ("aantal passes", "passes"),
    FieldAirAssist => ("luchtondersteuning", "air assist"),
    FieldOutput => ("uitvoer", "output"),
    ChangeLine => ("{0}: {1} {2} → {3}", "{0}: {1} {2} → {3}"),
    ChangeRemoved => ("{0}: bestaat niet meer in het document", "{0}: no longer in the document"),
    ChangeAdded => ("{0}: nieuw in het document", "{0}: new in the document"),
    On => ("aan", "on"),
    Off => ("uit", "off"),
    // --- Calibration.
    CalibrationHeader => ("Kalibratieproeven", "Calibration proofs"),
    KindKerf => ("Kerf (per materiaal)", "Kerf (per material)"),
    KindScanOffset => ("Scanoffset (per snelheid)", "Scan offset (per speed)"),
    KindPointerOffset => ("Pointeroffset (rode stip)", "Pointer offset (red dot)"),
    LabelKind => ("Soort", "Kind"),
    LabelMaterial => ("Materiaal", "Material"),
    LabelSpeed => ("Snelheid (mm/s)", "Speed (mm/s)"),
    LabelValue => ("Waarde (mm)", "Value (mm)"),
    LabelValueY => ("Waarde Y (mm)", "Value Y (mm)"),
    LabelMethod => ("Meetmethode", "Measuring method"),
    LabelDate => ("Datum (JJJJ-MM-DD)", "Date (YYYY-MM-DD)"),
    RecordProof => ("Leg proef vast", "Record Proof"),
    RemoveProof => ("Verwijder gekozen proef", "Remove Chosen Proof"),
    ProofNone => ("Nog geen proeven vastgelegd.", "No proofs recorded yet."),
    ProofLine => (
        "{0} — {1} — {2} — {3} — {4} — {5}",
        "{0} — {1} — {2} — {3} — {4} — {5}"
    ),
    ProofNoMaterial => ("geen materiaal", "no material"),
    ProblemNoMachine => (
        "Er is geen actief apparaat: een proef noemt haar machine.",
        "There is no active device: a proof names its machine."
    ),
    ProblemMaterial => ("Vul het materiaal in.", "Enter the material."),
    ProblemSpeed => ("De snelheid moet een getal groter dan 0 zijn.", "The speed must be a number greater than 0."),
    ProblemValue => (
        "De waarde is geen bruikbaar getal (kerf kan niet negatief zijn).",
        "The value is not a usable number (a kerf cannot be negative)."
    ),
    ProblemValueY => ("Waarde Y is geen bruikbaar getal.", "Value Y is not a usable number."),
    ProblemMethod => ("Vul de meetmethode in.", "Enter the measuring method."),
    ProblemDate => ("De datum moet JJJJ-MM-DD zijn.", "The date must be YYYY-MM-DD."),
    ProblemDuplicateSpeed => (
        "Voor deze snelheid bestaat al een scanoffset: verwijder die eerst.",
        "A scan offset for this speed exists already: remove it first."
    ),
    CalibrationNote => (
        "Een gewijzigde kalibratie maakt gecompileerde plannen verouderd; opgeslagen jobs houden hun snapshot. De waarden worden nog niet door de compiler gebruikt en gelden alleen in deze sessie.",
        "A changed calibration makes compiled plans out of date; stored jobs keep their snapshot. The compiler does not use the values yet and they only last for this session."
    ),
}

/// `text` in `locale`.
pub fn machine_text(text: MachineText, locale: Locale) -> &'static str {
    let (nl, en) = pair(text);
    match locale {
        Locale::Nl => nl,
        Locale::En => en,
    }
}

/// `text` with `{0}`, `{1}` … replaced by `args`.
pub fn fill(text: MachineText, locale: Locale, args: &[&str]) -> String {
    let mut out = machine_text(text, locale).to_owned();
    for (i, arg) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), arg);
    }
    out
}
