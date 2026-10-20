//! Variable text (W1-L5c, E11): serial counters, the job clock, the template language, data rows
//! with missing-value errors, the immutable production record, counter commit and bake.
//!
//! Reference dates were computed with Python's `datetime` (independent of the implementation):
//! 0 → 1970-01-01 00:00:00 (day 1); 1e9 → 2001-09-09 01:46:40 (day 252); 1e9 at +120 min →
//! 03:46:40; 951 782 400 → 2000-02-29 (day 60, leap); −86 400 → 1969-12-31 (day 365);
//! 1 781 751 845 at −300 min → 2026-06-17 22:04:05 (day 168); 4 102 444 799 at +60 min →
//! 2100-01-01 00:59:59 (2100 is not a leap year).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_document::text::{TextCommand, TextObject, VariableText};
use burnburn_generators::{
    AdvancePolicy, Commit, CutField, DataTable, FrozenRows, JobClock, JobEvent, OnEnd, ProductionCounters,
    ProductionRecord, RowBinding, RowError, SerialCounter, SerialSpec, Template, Token, VariableError, bake_command,
    resolve_text,
};
use burnburn_text::{FontRef, FontSize, FontStyle};
use burnburn_types::Length;

fn spec(start: i64, end: i64, by: i64) -> SerialSpec {
    SerialSpec::new(start, end, by, 0, OnEnd::Wrap).unwrap()
}

fn counter_values(mut c: SerialCounter, n: usize) -> Vec<i64> {
    let mut out = Vec::new();
    for _ in 0..n {
        out.push(c.current());
        c.advance().unwrap();
    }
    out
}

#[test]
fn a_serial_counts_from_start_by_the_step_and_wraps_past_the_end() {
    assert_eq!(counter_values(SerialCounter::new(spec(1, 5, 2)), 7), [1, 3, 5, 1, 3, 5, 1]);
    // End not on the lattice: 1, 3, 5 then 7 > 6 wraps.
    assert_eq!(counter_values(SerialCounter::new(spec(1, 6, 2)), 5), [1, 3, 5, 1, 3]);
    // Counting down.
    assert_eq!(counter_values(SerialCounter::new(spec(10, 0, -5)), 5), [10, 5, 0, 10, 5]);
    // A single value stays.
    assert_eq!(counter_values(SerialCounter::new(spec(4, 4, 1)), 3), [4, 4, 4]);
}

#[test]
fn previous_reset_and_set_current_stay_on_the_series() {
    let mut c = SerialCounter::new(spec(1, 9, 4));
    c.advance().unwrap();
    assert_eq!(c.current(), 5);
    c.step_back().unwrap();
    assert_eq!(c.current(), 1);
    c.step_back().unwrap();
    assert_eq!(c.current(), 9, "previous wraps backwards");
    c.reset();
    assert_eq!(c.current(), 1);
    c.set_current(5).unwrap();
    assert_eq!(c.current(), 5);
    assert_eq!(c.set_current(6), Err(VariableError::NotInSeries { value: 6 }));
    assert_eq!(c.set_current(13), Err(VariableError::NotInSeries { value: 13 }));
    assert_eq!(c.current(), 5, "a refused value changes nothing");
}

#[test]
fn a_stopping_series_refuses_to_pass_its_end() {
    let mut c = SerialCounter::new(SerialSpec::new(1, 2, 1, 0, OnEnd::Stop).unwrap());
    c.advance().unwrap();
    assert_eq!(c.advance(), Err(VariableError::SerialExhausted));
    assert_eq!(c.current(), 2);
    assert_eq!(c.value_at_offset(1), Err(VariableError::SerialExhausted));
}

#[test]
fn an_object_offset_moves_along_the_series_without_moving_the_counter() {
    let mut c = SerialCounter::new(spec(1, 6, 2)); // 1, 3, 5
    assert_eq!(c.value_at_offset(0), Ok(1));
    assert_eq!(c.value_at_offset(1), Ok(3));
    assert_eq!(c.value_at_offset(2), Ok(5));
    assert_eq!(c.value_at_offset(3), Ok(1), "wraps");
    assert_eq!(c.value_at_offset(-1), Ok(5));
    c.advance().unwrap();
    assert_eq!(c.value_at_offset(1), Ok(5));
    assert_eq!(c.current(), 3);
}

#[test]
fn serial_specs_are_validated_and_formatting_pads_with_zeros() {
    assert_eq!(
        SerialSpec::new(1, 9, 0, 0, OnEnd::Wrap),
        Err(VariableError::BadSerial { reason: "advance by is zero" })
    );
    assert!(
        matches!(SerialSpec::new(9, 1, 1, 0, OnEnd::Wrap), Err(VariableError::BadSerial { .. })),
        "end before start"
    );
    assert!(matches!(SerialSpec::new(1, 9, -1, 0, OnEnd::Wrap), Err(VariableError::BadSerial { .. })));
    assert!(matches!(SerialSpec::new(1, 9, 1, 40, OnEnd::Wrap), Err(VariableError::BadSerial { .. })));
    let c = SerialCounter::new(SerialSpec::new(1, 999, 1, 4, OnEnd::Wrap).unwrap());
    assert_eq!(c.format(7), "0007");
    assert_eq!(c.format(12345), "12345", "never truncates");
    let neg = SerialCounter::new(SerialSpec::new(-5, 5, 1, 3, OnEnd::Wrap).unwrap());
    assert_eq!(neg.format(-5), "-005");
    // No overflow at the ends of i64.
    let huge = SerialCounter::new(SerialSpec::new(i64::MAX - 2, i64::MAX, 1, 0, OnEnd::Wrap).unwrap());
    assert_eq!(huge.value_at_offset(5), Ok(i64::MAX - 2 + 5 % 3));
}

#[test]
fn the_job_clock_formats_in_its_fixed_zone() {
    let f = |ts, off, fmt: &str| JobClock::new(ts, off).unwrap().format(fmt).unwrap();
    let all = "%Y-%m-%d %H:%M:%S %j %y %z";
    assert_eq!(f(0, 0, all), "1970-01-01 00:00:00 001 70 +0000");
    assert_eq!(f(1_000_000_000, 0, all), "2001-09-09 01:46:40 252 01 +0000");
    assert_eq!(f(1_000_000_000, 120, all), "2001-09-09 03:46:40 252 01 +0200");
    assert_eq!(f(951_782_400, 0, all), "2000-02-29 00:00:00 060 00 +0000");
    assert_eq!(f(-86_400, 0, all), "1969-12-31 00:00:00 365 69 +0000");
    assert_eq!(f(1_781_751_845, -300, all), "2026-06-17 22:04:05 168 26 -0500");
    assert_eq!(f(4_102_444_799, 60, all), "2100-01-01 00:59:59 001 00 +0100");
    assert_eq!(f(0, 0, "100%% %Y"), "100% 1970");
    assert_eq!(JobClock::new(0, 24 * 60), Err(VariableError::BadClock));
    assert_eq!(JobClock::new(i64::MAX, 0), Err(VariableError::BadClock));
    assert!(matches!(JobClock::new(0, 0).unwrap().format("%Q"), Err(VariableError::BadDateFormat { .. })));
    assert!(matches!(JobClock::new(0, 0).unwrap().format("100%"), Err(VariableError::BadDateFormat { .. })));
}

#[test]
fn templates_parse_into_tokens_and_report_where_they_are_wrong() {
    let t = Template::parse("SN-{serial}/{date:%Y}{{x}} {csv:naam} {cut:C01.speed}").unwrap();
    assert_eq!(
        t.tokens(),
        [
            Token::Literal("SN-".into()),
            Token::Serial,
            Token::Literal("/".into()),
            Token::Date("%Y".into()),
            Token::Literal("{x} ".into()),
            Token::Csv("naam".into()),
            Token::Literal(" ".into()),
            Token::Cut { layer: "C01".into(), field: CutField::Speed },
        ]
    );
    assert!(t.is_dynamic());
    assert!(!Template::parse("gewoon {{tekst}}").unwrap().is_dynamic());
    assert_eq!(Template::parse("{date}").unwrap().tokens(), [Token::Date("%Y-%m-%d".into())], "default date format");
    assert_eq!(Template::parse("ab{serial"), Err(VariableError::BadTemplate { at: 2 }));
    assert_eq!(Template::parse("ab}"), Err(VariableError::BadTemplate { at: 2 }));
    assert_eq!(Template::parse("{nope}"), Err(VariableError::UnknownToken { name: "nope".into(), at: 0 }));
    assert_eq!(Template::parse("x{csv:}"), Err(VariableError::UnknownToken { name: "csv:".into(), at: 1 }));
    assert!(matches!(Template::parse("{date:%Q}"), Err(VariableError::BadDateFormat { .. })));
    assert!(matches!(Template::parse("{cut:C01.kleur}"), Err(VariableError::UnknownToken { .. })));
}

fn rows() -> Arc<FrozenRows> {
    let table = DataTable::parse_csv("naam;nummer\n\"Jansen; P.\";7\nPietersen;\nÅsa;9\n", ';').unwrap();
    Arc::new(FrozenRows::freeze(table, "2026-10-19"))
}

fn clock() -> JobClock {
    JobClock::new(1_781_751_845, -300).unwrap()
}

fn serial() -> SerialCounter {
    SerialCounter::new(SerialSpec::new(1, 999, 1, 4, OnEnd::Wrap).unwrap())
}

#[test]
fn a_record_resolves_serial_date_row_and_cut_references() {
    let rows = rows();
    let record = ProductionRecord::builder(clock())
        .serial(serial())
        .row(rows.clone(), RowBinding { rows: rows.hash, row: 0 })
        .unwrap()
        .cut_value("C01", CutField::Speed, "400")
        .cut_value("C01", CutField::Name, "Snijden")
        .build();
    let r = record.resolve("SN-{serial}-{date:%Y%m%d} {csv:naam} {cut:C01.name}@{cut:C01.speed}", 0).unwrap();
    assert_eq!(r, "SN-0001-20260617 Jansen; P. Snijden@400");
    assert_eq!(record.resolve("{serial}", 2).unwrap(), "0003", "the object's own offset");
}

#[test]
fn missing_values_are_errors_never_empty_labels() {
    let rows = rows();
    let bind = |row| ProductionRecord::builder(clock()).row(rows.clone(), RowBinding { rows: rows.hash, row }).unwrap();
    let r1 = bind(1).build();
    assert_eq!(r1.resolve("{csv:naam}", 0).unwrap(), "Pietersen");
    assert_eq!(
        r1.resolve("{csv:nummer}", 0),
        Err(VariableError::MissingValue { column: "nummer".into(), row: 1 }),
        "an empty cell is a missing value"
    );
    assert_eq!(r1.resolve("{csv:kleur}", 0), Err(VariableError::MissingColumn { column: "kleur".into() }));
    let none = ProductionRecord::builder(clock()).build();
    assert_eq!(none.resolve("{csv:naam}", 0), Err(VariableError::NoRow));
    assert_eq!(none.resolve("{serial}", 0), Err(VariableError::NoSerial));
    assert_eq!(
        none.resolve("{cut:C02.power}", 0),
        Err(VariableError::NoCutValue { layer: "C02".into(), field: CutField::Power })
    );
    assert_eq!(none.resolve("tekst {date:%Y}", 0).unwrap(), "tekst 2026");
}

#[test]
fn a_row_binding_made_for_other_data_is_refused() {
    let a = rows();
    let other = Arc::new(FrozenRows::freeze(DataTable::parse_csv("naam\nx\n", ',').unwrap(), "2026-10-19"));
    let err = ProductionRecord::builder(clock()).row(other, RowBinding { rows: a.hash, row: 0 }).unwrap_err();
    assert_eq!(err, VariableError::Row(RowError::StaleRows));
    let err = ProductionRecord::builder(clock()).row(a.clone(), RowBinding { rows: a.hash, row: 9 }).unwrap_err();
    assert_eq!(err, VariableError::Row(RowError::RowOutOfRange { row: 9, rows: 3 }));
}

#[test]
fn csv_bytes_must_be_utf8_and_a_byte_order_mark_is_ignored() {
    let t = DataTable::from_utf8(b"\xEF\xBB\xBFnaam,stad\n\"Zo\xC3\xAB\",\"Gent\nBelgi\xC3\xAB\"\n", ',').unwrap();
    assert_eq!(t.headers, ["naam", "stad"], "BOM is not part of the first header");
    assert_eq!(t.rows[0], ["Zoë", "Gent\nBelgië"]);
    let tab = DataTable::from_utf8(b"a\tb\n\"x\ty\"\tz\n", '\t').unwrap();
    assert_eq!(tab.rows[0], ["x\ty", "z"], "a quoted delimiter is data");
    let bad = DataTable::from_utf8(b"naam\nZo\xEB\n", ',').unwrap_err();
    assert_eq!(bad, burnburn_generators::CsvError::NotUtf8 { valid_up_to: 7 });
}

#[test]
fn frozen_job_keeps_chosen_data_rows() {
    // E11: a job fixes its row, serial and time; neither the live counters, nor a re-imported table,
    // nor the clock changes what the job burns, and the record identity says so.
    let rows = rows();
    let mut live = ProductionCounters::new(Some(serial()), AdvancePolicy::OnJobStarted);
    live.advance_manually().unwrap();
    live.advance_manually().unwrap(); // serial 3, row 2
    let record = live.record(clock(), Some(rows.clone()), &[("C01", CutField::Speed, "400")]).unwrap();
    let template = "{csv:naam} #{serial} {date:%d/%m/%Y %H:%M} {cut:C01.speed}";
    let before = record.resolve(template, 0).unwrap();
    assert_eq!(before, "Åsa #0003 17/06/2026 22:04 400");
    let id = record.id();

    // Later the user moves on: counters advance, a new table is loaded, time passes.
    live.advance_manually().unwrap();
    let changed =
        Arc::new(FrozenRows::freeze(DataTable::parse_csv("naam;nummer\nAndere;1\n", ';').unwrap(), "2026-12-24"));
    let _later = live.record(JobClock::new(1_800_000_000, 0).unwrap(), Some(changed), &[]);
    assert_eq!(record.resolve(template, 0).unwrap(), before, "the frozen job burns what it was prepared with");
    assert_eq!(record.id(), id);

    // Same inputs, same identity; any input changing changes it.
    let again = ProductionCounters::new(Some(serial()), AdvancePolicy::Manual);
    let mut a = again.clone();
    a.advance_manually().unwrap();
    a.advance_manually().unwrap();
    assert_eq!(a.record(clock(), Some(rows.clone()), &[("C01", CutField::Speed, "400")]).unwrap().id(), id);
    assert_ne!(a.record(clock(), Some(rows.clone()), &[("C01", CutField::Speed, "401")]).unwrap().id(), id);
    assert_ne!(
        a.record(JobClock::new(1_781_751_846, -300).unwrap(), Some(rows.clone()), &[("C01", CutField::Speed, "400")])
            .unwrap()
            .id(),
        id
    );
    assert_ne!(
        a.record(JobClock::new(1_781_751_845, 0).unwrap(), Some(rows.clone()), &[("C01", CutField::Speed, "400")])
            .unwrap()
            .id(),
        id,
        "the zone is part of the record"
    );
    assert_ne!(again.record(clock(), Some(rows.clone()), &[("C01", CutField::Speed, "400")]).unwrap().id(), id);
}

#[test]
fn the_counter_advances_once_on_the_chosen_event_and_never_after_a_failed_upload() {
    let rows = rows();
    let mut live = ProductionCounters::new(Some(serial()), AdvancePolicy::OnUploadConfirmed);
    let job = live.record(clock(), Some(rows.clone()), &[]).unwrap();
    assert_eq!((live.serial().unwrap().current(), live.row()), (1, 0));

    assert_eq!(live.commit(&job, JobEvent::UploadFailed), Ok(Commit::Ignored));
    assert_eq!(live.commit(&job, JobEvent::JobStarted), Ok(Commit::Ignored), "not the chosen event");
    assert_eq!(live.commit(&job, JobEvent::JobCancelled), Ok(Commit::Ignored));
    assert_eq!((live.serial().unwrap().current(), live.row(), live.generation()), (1, 0, 0), "nothing moved yet");

    assert_eq!(live.commit(&job, JobEvent::UploadConfirmed), Ok(Commit::Advanced));
    assert_eq!((live.serial().unwrap().current(), live.row(), live.generation()), (2, 1, 1));
    // The same job cannot advance the counters twice, also not by a repeated report.
    assert_eq!(live.commit(&job, JobEvent::UploadConfirmed), Err(VariableError::StaleRecord));
    assert_eq!(live.serial().unwrap().current(), 2);

    // A failed upload of the *next* job leaves the counters where they are: it is retried with the same number.
    let next = live.record(clock(), Some(rows.clone()), &[]).unwrap();
    assert_eq!(live.commit(&next, JobEvent::UploadFailed), Ok(Commit::Ignored));
    let retry = live.record(clock(), Some(rows), &[]).unwrap();
    assert_eq!(retry.resolve("{serial}", 0).unwrap(), "0002");
}

#[test]
fn manual_policy_ignores_job_events_and_a_stopping_series_does_not_half_advance() {
    let mut manual = ProductionCounters::new(Some(serial()), AdvancePolicy::Manual);
    let job = manual.record(clock(), None, &[]).unwrap();
    for e in [JobEvent::UploadConfirmed, JobEvent::JobStarted] {
        assert_eq!(manual.commit(&job, e), Ok(Commit::Ignored));
    }
    assert_eq!(manual.serial().unwrap().current(), 1);

    let stop = SerialCounter::new(SerialSpec::new(1, 1, 1, 0, OnEnd::Stop).unwrap());
    let mut live = ProductionCounters::new(Some(stop), AdvancePolicy::OnJobStarted);
    let job = live.record(clock(), Some(rows()), &[]).unwrap();
    assert_eq!(live.commit(&job, JobEvent::JobStarted), Err(VariableError::SerialExhausted));
    assert_eq!((live.row(), live.generation()), (0, 0), "the row did not advance either");
}

#[test]
fn running_out_of_rows_is_an_error_when_the_next_job_is_prepared() {
    let rows = rows();
    let mut live = ProductionCounters::new(None, AdvancePolicy::Manual);
    for _ in 0..3 {
        live.record(clock(), Some(rows.clone()), &[]).unwrap();
        live.advance_manually().unwrap();
    }
    assert_eq!(live.row(), 3);
    assert_eq!(
        live.record(clock(), Some(rows), &[]).unwrap_err(),
        VariableError::Row(RowError::RowOutOfRange { row: 3, rows: 3 })
    );
}

fn variable_text(content: &str, offset: i64) -> TextObject {
    let mut t = TextObject::new(
        content,
        FontRef::new("X", FontStyle::Regular),
        FontSize::from_mm(Length::from_mm(5.0).unwrap()).unwrap(),
    );
    t.variable = Some(VariableText { offset });
    t
}

#[test]
fn resolving_text_replaces_the_template_and_uses_the_objects_offset() {
    let rows = rows();
    let record = ProductionRecord::builder(clock())
        .serial(serial())
        .row(rows.clone(), RowBinding { rows: rows.hash, row: 0 })
        .unwrap()
        .build();
    let t = variable_text("{serial} {csv:naam}\n{{literal}}", 4);
    let r = resolve_text(&t, &record, 1).unwrap();
    assert_eq!(r.content, "0006 Jansen; P.\n{literal}", "offset 4 + array index 1 steps");
    assert_eq!(r.variable, None);
    assert_eq!((r.font.clone(), r.size, r.layout), (t.font.clone(), t.size, t.layout), "everything else is untouched");
    // Literal text is never interpreted.
    let mut lit = t.clone();
    lit.variable = None;
    assert_eq!(resolve_text(&lit, &record, 0), Err(VariableError::NotVariable));
}

#[test]
fn bake_is_an_undoable_set_command_with_fixed_content() {
    use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
    use burnburn_test_support::fixtures::read_fixture;
    use burnburn_text::{FontData, MemoryFontSource, TextEvaluator};
    use burnburn_types::{DocumentId, RequestId, StableId};

    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
    let r = src.insert("Liberation Sans", FontStyle::Regular, data);
    let mut text = variable_text("N{serial}", 0);
    text.font = r;
    let ev = TextEvaluator::new(src);
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let cmd = |s: &DocumentSession, c| DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Text(c),
    };
    let out = s.apply_with(cmd(&s, TextCommand::Create { text: Box::new(text.clone()) }), &[&ev]).unwrap();
    let id = *out.changes.texts.first().unwrap();

    let record = ProductionRecord::builder(clock()).serial(serial()).build();
    let bake = bake_command(id, &text, &record).unwrap();
    assert!(
        matches!(&bake, TextCommand::Set { text: t, source } if *t == id && source.content == "N0001" && source.variable.is_none())
    );
    let rev = s.revision();
    s.apply_with(cmd(&s, bake), &[&ev]).unwrap();
    assert_eq!(s.revision(), rev.next().unwrap(), "one undo step");
    assert_eq!(s.snapshot().text(id).unwrap().content, "N0001");
    assert_eq!(s.snapshot().text(id).unwrap().variable, None);
    s.undo(s.revision()).unwrap();
    assert_eq!(s.snapshot().text(id).unwrap().content, "N{serial}", "undo brings the dynamic content back");
    assert!(s.snapshot().text(id).unwrap().variable.is_some());
}
