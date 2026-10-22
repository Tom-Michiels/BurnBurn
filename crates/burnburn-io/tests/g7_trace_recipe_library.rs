//! G7-trace, T10: the user's own trace recipes are kept in a library file next to the other
//! libraries, so a recipe outlives the session and the project. Written atomically through the
//! filesystem port; a newer schema is reported with its version, a damaged file is a typed error and
//! never an empty list, and a list that would not load again is never written.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use burnburn_document::engine::{EngineRecipe, ParamValue};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::trace_recipe_library::{
    LibraryError, StoredRecipe, TRACE_RECIPE_LIBRARY_SCHEMA, TraceRecipeLibrary, decode_trace_recipe_library,
    encode_trace_recipe_library, load_trace_recipe_library, save_trace_recipe_library,
};
use burnburn_io::{LoadError, fs::FileSystem};

fn recipe(tolerance: f64) -> EngineRecipe {
    EngineRecipe::new("burnburn-trace/contour-1")
        .with("fit.tolerance_mm", ParamValue::Number(tolerance))
        .with("image.threshold", ParamValue::Text("adaptive".into()))
        .with("image.adaptive_radius", ParamValue::Integer(15))
        .with("fit.curves", ParamValue::Bool(true))
        .with(
            "contour.keep",
            ParamValue::List(vec![ParamValue::List(vec![ParamValue::Number(1.5), ParamValue::Number(2.25)])]),
        )
}

fn stored(name: &str, centerline: bool, tolerance: f64) -> StoredRecipe {
    StoredRecipe { name: name.into(), centerline, recipe: recipe(tolerance) }
}

#[test]
fn recipes_with_every_kind_of_value_survive_a_save_and_load_in_their_order() {
    let fs = MemoryFileSystem::new();
    let path = Path::new("/lib/trace-recipes.json");
    let mut library = TraceRecipeLibrary::default();
    library.set(stored("Logo scherp", false, 0.02)).unwrap();
    library.set(stored("Handschrift", true, 0.1)).unwrap();
    save_trace_recipe_library(&fs, path, &library).unwrap();
    assert_eq!(load_trace_recipe_library(&fs, path).unwrap(), library);
    assert_eq!(library.recipes.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["Logo scherp", "Handschrift"]);
}

#[test]
fn the_file_uses_its_documented_names() {
    // Typed out by hand: the names of the format are pinned independently of the encoder.
    let text = br#"{"format":"burnburn-trace-recipe-library","schema":1,"recipes":[
        {"name":"Foto","centerline":false,"recipe":{"engine":"burnburn-trace/contour-1","params":{"fit.curves":{"bool":true},"fit.tolerance_mm":{"number":0.1}}}}]}"#;
    let library = decode_trace_recipe_library(text).unwrap();
    assert_eq!(library.recipes.len(), 1);
    assert_eq!(library.recipes[0].name, "Foto");
    assert!(!library.recipes[0].centerline);
    assert_eq!(library.recipes[0].recipe.get("fit.tolerance_mm"), Some(&ParamValue::Number(0.1)));
    let again = decode_trace_recipe_library(&encode_trace_recipe_library(&library).unwrap()).unwrap();
    assert_eq!(again, library);
}

#[test]
fn saving_under_a_name_that_exists_replaces_that_recipe_in_place() {
    let mut library = TraceRecipeLibrary::default();
    library.set(stored("A", false, 0.1)).unwrap();
    library.set(stored("B", false, 0.1)).unwrap();
    library.set(stored("A", true, 0.2)).unwrap();
    assert_eq!(library.recipes.len(), 2);
    assert_eq!(library.recipes[0].name, "A");
    assert!(library.recipes[0].centerline);
    assert_eq!(library.get("A").unwrap().recipe.get("fit.tolerance_mm"), Some(&ParamValue::Number(0.2)));
    assert!(library.remove("A") && !library.remove("A"));
    assert_eq!(library.recipes.len(), 1);
}

#[test]
fn a_name_must_be_text_of_reasonable_length_and_a_recipe_must_hold_finite_numbers() {
    let mut library = TraceRecipeLibrary::default();
    assert_eq!(library.set(stored("   ", false, 0.1)), Err(LibraryError::EmptyName));
    assert_eq!(library.set(stored(&"x".repeat(81), false, 0.1)), Err(LibraryError::NameTooLong));
    assert_eq!(library.set(stored("NaN", false, f64::NAN)), Err(LibraryError::InvalidRecipe));
    let mut blank = stored("blank", false, 0.1);
    blank.recipe.engine = String::new();
    assert_eq!(library.set(blank), Err(LibraryError::InvalidRecipe));
    assert!(library.recipes.is_empty());
    // The name is stored trimmed.
    library.set(stored("  Foto  ", false, 0.1)).unwrap();
    assert_eq!(library.recipes[0].name, "Foto");
    assert!(library.get("Foto").is_some());
}

#[test]
fn a_damaged_newer_or_foreign_file_is_a_typed_error_and_the_old_file_stays_when_saving_fails() {
    let fs = MemoryFileSystem::new();
    let path = Path::new("/lib/trace-recipes.json");
    assert!(matches!(decode_trace_recipe_library(b"{not json"), Err(LoadError::Json { .. })));
    assert!(matches!(
        decode_trace_recipe_library(br#"{"format":"burnburn-calibration-library","schema":1,"recipes":[]}"#),
        Err(LoadError::NotAProject)
    ));
    let newer = format!(
        r#"{{"format":"burnburn-trace-recipe-library","schema":{},"recipes":[]}}"#,
        TRACE_RECIPE_LIBRARY_SCHEMA + 1
    );
    assert!(matches!(
        decode_trace_recipe_library(newer.as_bytes()),
        Err(LoadError::NewerSchema { found: 2, supported: 1 })
    ));
    let twice = br#"{"format":"burnburn-trace-recipe-library","schema":1,"recipes":[
        {"name":"A","centerline":false,"recipe":{"engine":"e","params":{}}},
        {"name":"A","centerline":false,"recipe":{"engine":"e","params":{}}}]}"#;
    assert!(matches!(decode_trace_recipe_library(twice), Err(LoadError::InvalidValue { .. })));
    let unknown = br#"{"format":"burnburn-trace-recipe-library","schema":1,"recipes":[],"extra":1}"#;
    assert!(matches!(decode_trace_recipe_library(unknown), Err(LoadError::UnknownField { .. })));

    // A good file is written; a list that would not load again is refused and the file is untouched.
    let mut library = TraceRecipeLibrary::default();
    library.set(stored("Goed", false, 0.1)).unwrap();
    save_trace_recipe_library(&fs, path, &library).unwrap();
    let before = fs.read(path, 1 << 20).unwrap();
    library.recipes.push(stored("Goed", false, 0.3)); // a duplicate name, bypassing `set`
    assert!(save_trace_recipe_library(&fs, path, &library).is_err());
    assert_eq!(fs.read(path, 1 << 20).unwrap(), before);
    assert!(matches!(load_trace_recipe_library(&fs, Path::new("/lib/none.json")), Err(LoadError::Fs(_))));
}
