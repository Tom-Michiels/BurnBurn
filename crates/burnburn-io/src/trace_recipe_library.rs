//! The user's own trace recipes (G7-trace, T10): named, kept outside every project in one library
//! file next to the other libraries.
//!
//! JSON (`format`, `schema`, `recipes`), written atomically through the [`FileSystem`] port. A recipe
//! is the engine parameter form of a trace recipe (the same typed values a project stores) plus the
//! mode it was made for. A list that would not load again (a duplicate name, a number that is not
//! finite) is never written.

use std::collections::BTreeSet;
use std::path::Path;

use burnburn_document::engine::EngineRecipe;
use serde::{Deserialize, Serialize};

use crate::engine_dto as d;
use crate::engine_map::{recipe_from, recipe_to};
use crate::error::{LoadError, SaveError};
use crate::fs::FileSystem;
use crate::store::write_atomic;

pub const TRACE_RECIPE_LIBRARY_SCHEMA: u32 = 1;
const FORMAT: &str = "burnburn-trace-recipe-library";
const MAX_RECIPES: usize = 1_000;
const MAX_NAME_CHARS: usize = 80;
const MAX_BYTES: u64 = 16 << 20;

/// One saved recipe: the engine parameters of a trace recipe and the mode (contour or centre line).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredRecipe {
    pub name: String,
    pub centerline: bool,
    pub recipe: EngineRecipe,
}

/// Why a recipe could not be put in the library. Nothing changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LibraryError {
    EmptyName,
    NameTooLong,
    TooMany,
    InvalidRecipe,
}

/// The recipes in the order they were first saved.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TraceRecipeLibrary {
    pub recipes: Vec<StoredRecipe>,
}

impl TraceRecipeLibrary {
    /// Adds the recipe, or replaces the one with the same (trimmed) name where it stands.
    pub fn set(&mut self, mut recipe: StoredRecipe) -> Result<(), LibraryError> {
        recipe.name = recipe.name.trim().to_owned();
        if recipe.name.is_empty() {
            return Err(LibraryError::EmptyName);
        }
        if recipe.name.chars().count() > MAX_NAME_CHARS {
            return Err(LibraryError::NameTooLong);
        }
        if recipe.recipe.validate().is_err() {
            return Err(LibraryError::InvalidRecipe);
        }
        if let Some(at) = self.recipes.iter().position(|r| r.name == recipe.name) {
            self.recipes[at] = recipe;
        } else if self.recipes.len() >= MAX_RECIPES {
            return Err(LibraryError::TooMany);
        } else {
            self.recipes.push(recipe);
        }
        Ok(())
    }

    /// Takes the recipe called `name` out; whether there was one.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.recipes.len();
        self.recipes.retain(|r| r.name != name);
        self.recipes.len() != before
    }

    pub fn get(&self, name: &str) -> Option<&StoredRecipe> {
        self.recipes.iter().find(|r| r.name == name)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDto {
    format: String,
    schema: u32,
    recipes: Vec<RecipeDto>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecipeDto {
    name: String,
    centerline: bool,
    recipe: d::Recipe,
}

fn json_error(e: &serde_json::Error) -> LoadError {
    let detail = e.to_string();
    if detail.starts_with("unknown field") || detail.starts_with("unknown variant") {
        LoadError::UnknownField { entry: "trace recipe library", detail }
    } else {
        LoadError::Json { entry: "trace recipe library", detail }
    }
}

/// Bytes → recipes, fully validated.
pub fn decode_trace_recipe_library(bytes: &[u8]) -> Result<TraceRecipeLibrary, LoadError> {
    let loose: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| json_error(&e))?;
    if loose.get("format").and_then(serde_json::Value::as_str) != Some(FORMAT) {
        return Err(LoadError::NotAProject);
    }
    let schema = loose.get("schema").and_then(serde_json::Value::as_u64).ok_or(LoadError::NotAProject)?;
    if schema > u64::from(TRACE_RECIPE_LIBRARY_SCHEMA) {
        return Err(LoadError::NewerSchema {
            found: u32::try_from(schema).unwrap_or(u32::MAX),
            supported: TRACE_RECIPE_LIBRARY_SCHEMA,
        });
    }
    if schema < 1 {
        return Err(LoadError::InvalidValue { path: "schema".into(), reason: format!("unknown schema {schema}") });
    }
    let dto: FileDto = serde_json::from_value(loose).map_err(|e| json_error(&e))?;
    if dto.recipes.len() > MAX_RECIPES {
        return Err(LoadError::Limit("trace recipes"));
    }
    let mut names = BTreeSet::new();
    let mut recipes = Vec::with_capacity(dto.recipes.len());
    for (i, r) in dto.recipes.into_iter().enumerate() {
        let path = format!("recipes[{i}]");
        let invalid = |reason: &str| LoadError::InvalidValue { path: path.clone(), reason: reason.into() };
        let trimmed = r.name.trim();
        if trimmed.is_empty() || trimmed != r.name || r.name.chars().count() > MAX_NAME_CHARS {
            return Err(invalid("a name is text of 1 to 80 characters without spaces around it"));
        }
        if !names.insert(r.name.clone()) {
            return Err(invalid("two recipes have this name"));
        }
        let recipe = recipe_from(&|| format!("{path}.recipe"), &r.recipe)?;
        recipes.push(StoredRecipe { name: r.name, centerline: r.centerline, recipe });
    }
    Ok(TraceRecipeLibrary { recipes })
}

/// Recipes → bytes (compact JSON).
pub fn encode_trace_recipe_library(library: &TraceRecipeLibrary) -> Result<Vec<u8>, SaveError> {
    let dto = FileDto {
        format: FORMAT.to_owned(),
        schema: TRACE_RECIPE_LIBRARY_SCHEMA,
        recipes: library
            .recipes
            .iter()
            .map(|r| RecipeDto { name: r.name.clone(), centerline: r.centerline, recipe: recipe_to(&r.recipe) })
            .collect(),
    };
    serde_json::to_vec(&dto).map_err(|e| SaveError::Encode(e.to_string()))
}

/// Atomic save. What is written is decoded again first, so a list that would not load is never
/// written and the old file stays.
pub fn save_trace_recipe_library(
    fs: &dyn FileSystem,
    path: &Path,
    library: &TraceRecipeLibrary,
) -> Result<(), SaveError> {
    let bytes = encode_trace_recipe_library(library)?;
    decode_trace_recipe_library(&bytes).map_err(|e| SaveError::Invalid(e.to_string()))?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs.create_dir_all(dir).map_err(SaveError::Fs)?;
    }
    write_atomic(fs, path, &bytes)
}

pub fn load_trace_recipe_library(fs: &dyn FileSystem, path: &Path) -> Result<TraceRecipeLibrary, LoadError> {
    let bytes = fs.read(path, MAX_BYTES).map_err(LoadError::Fs)?;
    decode_trace_recipe_library(&bytes)
}
