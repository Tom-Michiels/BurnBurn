//! G7-editor S07: renaming a variant. The definition and every instance that uses the variant
//! follow in one transaction; the derived outputs stay as they were, and a clash or an unknown
//! name is refused without a change.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;

use burnburn_document::structure::{
    Dim, NamedShape, PartDefinition, PartDefinitionId, PartInstance, PartInstanceId, PartShape, StructureChange,
    StructureCommand, StructureError,
};
use burnburn_document::{CommandError, CommandOutcome, CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::Affine;
use burnburn_types::{DocumentId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn run(s: &mut DocumentSession, cmd: StructureCommand) -> Result<CommandOutcome, CommandError> {
    let cmd = DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Structure(cmd),
    };
    s.apply(cmd)
}

fn definition() -> PartDefinition {
    let shape = PartShape::Rectangle {
        x: Dim::Mm(0.0),
        y: Dim::Mm(0.0),
        width: Dim::Param("breedte".into()),
        height: Dim::Mm(20.0),
    };
    PartDefinition {
        name: "Beugel".into(),
        parameters: BTreeMap::from([("breedte".to_owned(), mm(40.0))]),
        shapes: BTreeMap::from([(1, NamedShape { name: "omtrek".into(), shape })]),
        variants: BTreeMap::from([
            ("klein".to_owned(), BTreeMap::from([("breedte".to_owned(), mm(25.0))])),
            ("groot".to_owned(), BTreeMap::from([("breedte".to_owned(), mm(60.0))])),
        ]),
    }
}

fn place(s: &mut DocumentSession, def: PartDefinitionId, variant: Option<&str>) -> PartInstanceId {
    let instance = PartInstance {
        definition: def,
        variant: variant.map(str::to_owned),
        overrides: BTreeMap::new(),
        transform: Affine::IDENTITY,
        plate: None,
    };
    let StructureChange::Instance(i) =
        run(s, StructureCommand::PlaceInstance { instance }).unwrap().changes.structure[0]
    else {
        panic!("an instance")
    };
    i
}

fn setup() -> (DocumentSession, PartDefinitionId, PartInstanceId, PartInstanceId) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let StructureChange::Definition(def) =
        run(&mut s, StructureCommand::DefinePart { definition: definition() }).unwrap().changes.structure[0]
    else {
        panic!("a definition")
    };
    let a = place(&mut s, def, Some("klein"));
    let b = place(&mut s, def, None);
    (s, def, a, b)
}

#[test]
fn a_renamed_variant_keeps_its_values_and_its_instances() {
    let (mut s, def, a, b) = setup();
    let width_before = s.snapshot().objects().map(|(_, o)| o.bounds().unwrap().width()).fold(0.0_f64, f64::max);
    run(&mut s, StructureCommand::RenameVariant { definition: def, from: "klein".into(), to: "smal".into() }).unwrap();

    let snap = s.snapshot();
    let d = snap.structure().definition(def).unwrap();
    assert!(!d.variants.contains_key("klein"));
    assert_eq!(d.variants["smal"]["breedte"], mm(25.0), "the values moved with the name");
    assert_eq!(snap.structure().instance(a).unwrap().variant.as_deref(), Some("smal"), "its instance follows");
    assert_eq!(snap.structure().instance(b).unwrap().variant, None, "an instance without a variant stays so");
    let width_after = snap.objects().map(|(_, o)| o.bounds().unwrap().width()).fold(0.0_f64, f64::max);
    assert_eq!(width_before, width_after);
    let narrow = snap.objects().map(|(_, o)| o.bounds().unwrap().width()).fold(f64::MAX, f64::min);
    assert_eq!(narrow, 25.0, "the instance still draws the 25 mm variant");
}

#[test]
fn renaming_is_one_undo_step_for_the_definition_and_the_instances() {
    let (mut s, def, a, _) = setup();
    run(&mut s, StructureCommand::RenameVariant { definition: def, from: "klein".into(), to: "smal".into() }).unwrap();
    s.undo(s.revision()).unwrap();
    let snap = s.snapshot();
    assert!(snap.structure().definition(def).unwrap().variants.contains_key("klein"));
    assert_eq!(snap.structure().instance(a).unwrap().variant.as_deref(), Some("klein"));
}

#[test]
fn a_clash_an_unknown_name_and_an_empty_name_are_refused_and_change_nothing() {
    let (mut s, def, _, _) = setup();
    let revision = s.revision();
    let rename =
        |from: &str, to: &str| StructureCommand::RenameVariant { definition: def, from: from.into(), to: to.into() };
    let err = run(&mut s, rename("klein", "groot")).unwrap_err();
    assert!(matches!(err, CommandError::Structure(StructureError::InvalidValue { .. })), "{err:?}");
    let err = run(&mut s, rename("bestaat niet", "x")).unwrap_err();
    assert!(matches!(err, CommandError::Structure(StructureError::UnknownVariant(_))), "{err:?}");
    let err = run(&mut s, rename("klein", "  ")).unwrap_err();
    assert!(matches!(err, CommandError::Structure(StructureError::InvalidValue { .. })), "{err:?}");
    let err = run(&mut s, rename("klein", "klein")).unwrap_err();
    assert!(matches!(err, CommandError::Structure(StructureError::NothingChanged)), "{err:?}");
    assert_eq!(s.revision(), revision);
}
