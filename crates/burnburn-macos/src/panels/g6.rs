//! The golf-6 editor panels, headless (G6-editor): the generator panel (E10). Shares the plain-line
//! model of the golf-5 panels (`G5Line`, `G5Control`): both hosts draw the lines and report a
//! control with the panel's current values; field text goes to presentation unparsed (DR-05) and a
//! value presentation already shows sends nothing.

use burnburn_presentation::{
    DocumentViewModel, FilesIntent, GeneratorEdit, GeneratorField, GeneratorIssue, GeneratorKind, GeneratorProblem,
    SearchField, SearchResultRow, UiIntent,
};
use burnburn_types::DocumentId;

use super::g5::{G5Control, G5Line, flag_of, index_of, value_of};
use crate::l10n::Locale;
use crate::panels_image::BitmapImage;
use crate::l10n_g6::{G6Text, g6_fmt, g6_text};

const KINDS: [GeneratorKind; 2] = [GeneratorKind::FingerBox, GeneratorKind::LivingHinge];

/// A number of millimetres with up to three decimals, in the window language.
fn mm_text(value: f64, locale: Locale) -> String {
    let mut text = format!("{value:.3}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    match locale {
        Locale::Nl => text.replace('.', ","),
        Locale::En => text,
    }
}

fn caption_text(kind: GeneratorKind, field: GeneratorField) -> G6Text {
    use GeneratorField as F;
    match (kind, field) {
        (GeneratorKind::FingerBox, F::Width) => G6Text::FieldBoxWidth,
        (GeneratorKind::FingerBox, F::Depth) => G6Text::FieldBoxDepth,
        (GeneratorKind::FingerBox, F::Height) => G6Text::FieldBoxHeight,
        (GeneratorKind::LivingHinge, F::Width) => G6Text::FieldHingeWidth,
        (GeneratorKind::LivingHinge, F::Height) => G6Text::FieldHingeHeight,
        (_, F::Depth) => G6Text::FieldBoxDepth,
        (_, F::Thickness) => G6Text::FieldThickness,
        (_, F::FingerWidth) => G6Text::FieldFingerWidth,
        (_, F::PanelSpacing) => G6Text::FieldPanelSpacing,
        (_, F::CutLength) => G6Text::FieldCutLength,
        (_, F::Bridge) => G6Text::FieldBridge,
        (_, F::ColumnSpacing) => G6Text::FieldColumnSpacing,
    }
}

fn problem_text(kind: GeneratorKind, p: &GeneratorProblem, locale: Locale) -> String {
    let caption = g6_text(caption_text(kind, p.field), locale);
    match p.issue {
        GeneratorIssue::InvalidNumber => g6_fmt(G6Text::ProblemNumber, locale, &[caption]),
        GeneratorIssue::NotPositive => g6_fmt(G6Text::ProblemNotPositive, locale, &[caption]),
        GeneratorIssue::Negative => g6_fmt(G6Text::ProblemNegative, locale, &[caption]),
        GeneratorIssue::TooSmall { min_mm, value_mm } => {
            g6_fmt(G6Text::ProblemTooSmall, locale, &[caption, &mm_text(min_mm, locale), &mm_text(value_mm, locale)])
        }
        GeneratorIssue::Count { min, max, value } => g6_fmt(
            G6Text::ProblemCount,
            locale,
            &[caption, &min.to_string(), &max.to_string(), &value.to_string()],
        ),
        _ => g6_fmt(G6Text::ProblemEngine, locale, &[caption]),
    }
}

/// The lines of the generator panel, or `None` while the dialog is closed.
pub(super) fn generator_lines(doc: &DocumentViewModel, locale: Locale) -> Option<Vec<G5Line>> {
    let d = doc.generator_dialog.as_ref()?;
    let t = |x: G6Text| g6_text(x, locale).to_owned();
    let mut lines = vec![G5Line::Choice {
        control: G5Control::GeneratorKind,
        caption: t(G6Text::KindCaption),
        options: vec![t(G6Text::KindBox), t(G6Text::KindHinge)],
        selected: KINDS.iter().position(|k| *k == d.kind).unwrap_or(0),
    }];
    for row in &d.fields {
        lines.push(G5Line::Field {
            control: G5Control::GeneratorField(row.field),
            caption: t(caption_text(d.kind, row.field)),
            text: row.text.clone(),
        });
    }
    if let Some(open) = d.lid_open {
        lines.push(G5Line::Check { control: G5Control::GeneratorLid, caption: t(G6Text::LidOff), on: open });
    }
    if d.kind == GeneratorKind::FingerBox {
        lines.push(G5Line::Text(t(G6Text::KerfNote)));
    }
    for p in &d.problems {
        lines.push(G5Line::Problem(problem_text(d.kind, p, locale)));
    }
    if let Some(p) = &d.preview {
        let (first, text) = match d.kind {
            GeneratorKind::FingerBox => (p.parts, G6Text::PreviewBox),
            GeneratorKind::LivingHinge => (p.cuts, G6Text::PreviewHinge),
        };
        lines.push(G5Line::Text(g6_fmt(text, locale, &[&first.to_string(), &p.width, &p.height])));
    }
    lines.push(G5Line::Button {
        control: G5Control::GeneratorInsert,
        title: t(if d.editing { G6Text::Apply } else { G6Text::Insert }),
        enabled: d.can_insert,
    });
    lines.push(G5Line::Button { control: G5Control::GeneratorCancel, title: t(G6Text::Cancel), enabled: true });
    Some(lines)
}

/// The intent of a generator-panel control.
pub(super) fn generator_intent(
    doc: &DocumentViewModel,
    document: DocumentId,
    control: G5Control,
    values: &[(G5Control, String)],
) -> Option<UiIntent> {
    let d = doc.generator_dialog.as_ref()?;
    let edit = match control {
        G5Control::GeneratorKind => {
            let kind = *KINDS.get(index_of(values, control)?)?;
            (kind != d.kind).then_some(GeneratorEdit::Kind(kind))?
        }
        G5Control::GeneratorField(field) => {
            let text = value_of(values, control)?;
            let shown = d.fields.iter().find(|r| r.field == field)?;
            (text != shown.text).then(|| GeneratorEdit::Field(field, text.to_owned()))?
        }
        G5Control::GeneratorLid => {
            let open = flag_of(values, control)?;
            (Some(open) != d.lid_open && d.lid_open.is_some()).then_some(GeneratorEdit::LidOpen(open))?
        }
        G5Control::GeneratorInsert => d.can_insert.then_some(GeneratorEdit::Insert)?,
        G5Control::GeneratorCancel => GeneratorEdit::Cancel,
        _ => return None,
    };
    Some(UiIntent::Generator { document, edit })
}

// --- Project search (P08) ---------------------------------------------------------------------------

/// `2026-06-18 14:30 UTC` for seconds since the Unix epoch (the file system knows no time zone).
fn utc_text(seconds: u64) -> String {
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let rest = seconds % 86_400;
    // Civil date from days since 1970-01-01 (Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02} UTC", rest / 3600, rest % 3600 / 60)
}

fn found_in(field: SearchField) -> G6Text {
    match field {
        SearchField::FileName => G6Text::SearchFieldFileName,
        SearchField::Object => G6Text::SearchFieldObject,
        SearchField::Operation => G6Text::SearchFieldOperation,
        SearchField::Material => G6Text::SearchFieldMaterial,
        SearchField::Part => G6Text::SearchFieldPart,
        SearchField::Plate => G6Text::SearchFieldPlate,
        _ => G6Text::SearchFieldRevision,
    }
}

/// The result and revision the panel shows, from what the user chose (the first when unknown).
fn chosen<'a>(
    results: &'a [SearchResultRow],
    values: &[(G5Control, String)],
) -> Option<(usize, &'a SearchResultRow, usize)> {
    let index = index_of(values, G5Control::SearchResult).unwrap_or(0).min(results.len().checked_sub(1)?);
    let row = results.get(index)?;
    let revision = index_of(values, G5Control::SearchRevision).unwrap_or(0).min(row.revisions.len().saturating_sub(1));
    Some((index, row, revision))
}

/// The lines of the search panel, or `None` while the search is closed.
pub(super) fn search_lines(
    doc: &DocumentViewModel,
    locale: Locale,
    draft: &[(G5Control, String)],
) -> Option<Vec<G5Line>> {
    let search = doc.files.search.as_ref()?;
    let t = |x: G6Text| g6_text(x, locale).to_owned();
    let mut lines = vec![
        G5Line::Field { control: G5Control::SearchFolder, caption: t(G6Text::SearchFolderCaption), text: search.folder.clone() },
        G5Line::Field { control: G5Control::SearchQuery, caption: t(G6Text::SearchQueryCaption), text: search.query.clone() },
        G5Line::Button { control: G5Control::SearchRescan, title: t(G6Text::SearchRescan), enabled: !search.scanning },
    ];
    if search.scanning {
        lines.push(G5Line::Text(t(G6Text::SearchScanning)));
    } else if search.folder_unreadable {
        lines.push(G5Line::Problem(t(G6Text::SearchFolderUnreadable)));
    } else if let Some(scanned) = search.scanned {
        lines.push(G5Line::Text(g6_fmt(G6Text::SearchScanned, locale, &[&scanned.to_string()])));
    }
    for issue in &search.issues {
        lines.push(G5Line::Problem(g6_fmt(G6Text::SearchIssue, locale, &[&issue.file_name])));
    }
    let Some((index, row, revision)) = chosen(&search.results, draft) else {
        if search.scanned.is_some() && !search.folder_unreadable {
            lines.push(G5Line::Text(t(G6Text::SearchNone)));
        }
        lines.push(G5Line::Button { control: G5Control::SearchClose, title: t(G6Text::SearchClose), enabled: true });
        return Some(lines);
    };
    lines.push(G5Line::Choice {
        control: G5Control::SearchResult,
        caption: t(G6Text::SearchResultCaption),
        options: search
            .results
            .iter()
            .map(|r| match r.modified {
                Some(seconds) => format!("{} - {}", r.file_name, utc_text(seconds)),
                None => r.file_name.clone(),
            })
            .collect(),
        selected: index,
    });
    if let Some(thumbnail) = &row.thumbnail {
        let rgba = thumbnail.coverage.iter().flat_map(|c| [255 - c, 255 - c, 255 - c, 255]).collect();
        lines.push(G5Line::Image(BitmapImage { width: thumbnail.width, height: thumbnail.height, rgba }));
    }
    let objects = row.objects.to_string();
    lines.push(G5Line::Text(match row.modified {
        Some(seconds) => g6_fmt(G6Text::SearchInfo, locale, &[&objects, &utc_text(seconds)]),
        None => g6_fmt(G6Text::SearchInfoUnknown, locale, &[&objects]),
    }));
    if !row.matched.is_empty() {
        let fields: Vec<&str> = row.matched.iter().map(|f| g6_text(found_in(*f), locale)).collect();
        lines.push(G5Line::Text(g6_fmt(G6Text::SearchFoundIn, locale, &[&fields.join(", ")])));
    }
    if !row.revisions.is_empty() {
        lines.push(G5Line::Choice {
            control: G5Control::SearchRevision,
            caption: t(G6Text::SearchRevisionCaption),
            options: row.revisions.iter().map(|r| format!("{}: {}", r.number, r.label)).collect(),
            selected: revision,
        });
    }
    lines.push(G5Line::Button { control: G5Control::SearchOpenProject, title: t(G6Text::SearchOpenProject), enabled: true });
    lines.push(G5Line::Button {
        control: G5Control::SearchOpenRevision,
        title: t(G6Text::SearchOpenRevision),
        enabled: !row.revisions.is_empty(),
    });
    lines.push(G5Line::Button { control: G5Control::SearchClose, title: t(G6Text::SearchClose), enabled: true });
    Some(lines)
}

/// The intent of a search-panel control.
pub(super) fn search_intent(
    doc: &DocumentViewModel,
    document: DocumentId,
    control: G5Control,
    values: &[(G5Control, String)],
) -> Option<UiIntent> {
    let search = doc.files.search.as_ref()?;
    let intent = match control {
        G5Control::SearchFolder => {
            let folder = value_of(values, control)?;
            (folder != search.folder).then(|| FilesIntent::SetSearchFolder { document, folder: folder.to_owned() })?
        }
        G5Control::SearchQuery => {
            let query = value_of(values, control)?;
            (query != search.query).then(|| FilesIntent::SetSearchQuery { document, query: query.to_owned() })?
        }
        G5Control::SearchRescan => FilesIntent::RescanSearch { document },
        G5Control::SearchOpenProject => {
            let (_, row, _) = chosen(&search.results, values)?;
            FilesIntent::OpenFoundProject { document, path: row.path.clone() }
        }
        G5Control::SearchOpenRevision => {
            let (_, row, revision) = chosen(&search.results, values)?;
            let number = row.revisions.get(revision)?.number;
            FilesIntent::OpenFoundRevision { document, path: row.path.clone(), number }
        }
        G5Control::SearchClose => FilesIntent::CloseSearch { document },
        // Choosing a result or revision only changes what the panel shows.
        _ => return None,
    };
    Some(UiIntent::Files(intent))
}
