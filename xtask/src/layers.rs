//! `cargo xtask layers`: crate layering and forbidden-dependency rules.
//!
//! The rules and the table live in `docs/design/layers.toml` (owner: Design; rules R1–R8 are
//! described in its header). This module only implements them.
//!
//! Structure adapted from solidcraft's `xtask/src/layers.rs` (MIT OR Apache-2.0): the rule
//! engine works on a small model that is independent of `cargo metadata`, so it can be
//! unit-tested; `packages_from_metadata`/`closure_from_metadata` build that model.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde_json::Value;

pub const TEST_SUPPORT: &str = "burnburn-test-support";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Core,
    Adapter,
    Host,
    Testkit,
    Tool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsafePolicy {
    Forbid,
    Adapter,
}

/// One `[crates.<name>]` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub layer: u32,
    pub kind: Kind,
    pub unsafe_policy: UnsafePolicy,
    pub allow: Vec<String>,
    pub forbid: Vec<String>,
    pub forbid_transitive: Vec<String>,
}

/// The parsed `docs/design/layers.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Table {
    pub crates: BTreeMap<String, Spec>,
    pub groups: BTreeMap<String, Vec<String>>,
    pub triples: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DepKind {
    Normal,
    Dev,
    Build,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dep {
    pub name: String,
    pub kind: DepKind,
    /// The dependency is a workspace member.
    pub workspace: bool,
}

/// A workspace package as far as the rules care.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub deps: Vec<Dep>,
    /// `[lints] workspace = true` in its manifest.
    pub workspace_lints: bool,
}

/// Per target triple: workspace package -> external package names in its normal+build closure.
pub type Closures = BTreeMap<String, BTreeMap<String, BTreeSet<String>>>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Violation {
    /// R1
    Unregistered { krate: String },
    /// Table error: `allow`/`forbid` names something that does not exist.
    UnknownReference { krate: String, name: String },
    /// R2: normal/build dependency not in `allow`.
    NotAllowed { krate: String, dep: String, kind: DepKind },
    /// R2/R3: dependency on an equal (R2) or higher layer.
    Upward { krate: String, dep: String, from: u32, to: u32, kind: DepKind },
    /// R3/R8: hosts and tools are never dependencies.
    DependsOnLeaf { krate: String, dep: String },
    /// R4
    TestkitAsNormalDep { krate: String, kind: DepKind },
    /// R5
    ForbiddenDirect { krate: String, dep: String, group: String },
    /// R6
    ForbiddenTransitive { krate: String, dep: String, group: String, triple: String },
    /// R7
    LintsNotInherited { krate: String },
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Violation::Unregistered { krate } => {
                write!(f, "R1 {krate}: workspace crate not in docs/design/layers.toml (ask Design to add it)")
            }
            Violation::UnknownReference { krate, name } => {
                write!(f, "table {krate}: `{name}` is neither a known crate nor a [groups] entry")
            }
            Violation::NotAllowed { krate, dep, kind } => {
                write!(f, "R2 {krate} -> {dep} [{kind:?}]: not in `allow` of {krate}")
            }
            Violation::Upward { krate, dep, from, to, kind } => {
                write!(f, "R2/R3 {krate} (L{from}) -> {dep} (L{to}) [{kind:?}]: dependency on an equal or higher layer")
            }
            Violation::DependsOnLeaf { krate, dep } => {
                write!(f, "R3/R8 {krate} -> {dep}: hosts and tools may not be depended on")
            }
            Violation::TestkitAsNormalDep { krate, kind } => {
                write!(f, "R4 {krate}: {TEST_SUPPORT} may only be a dev-dependency (found {kind:?})")
            }
            Violation::ForbiddenDirect { krate, dep, group } => {
                write!(f, "R5 {krate}: direct dependency `{dep}` is in forbidden group `{group}`")
            }
            Violation::ForbiddenTransitive { krate, dep, group, triple } => {
                write!(f, "R6 {krate} ({triple}): `{dep}` in its dependency closure is in forbidden group `{group}`")
            }
            Violation::LintsNotInherited { krate } => {
                write!(f, "R7 {krate}: unsafe = \"forbid\" requires `[lints] workspace = true`")
            }
        }
    }
}

/// Parses `docs/design/layers.toml`.
pub fn parse_table(text: &str) -> Result<Table, String> {
    let doc: toml::Table = toml::from_str(text).map_err(|e| format!("layers.toml: {e}"))?;
    let strings = |v: Option<&toml::Value>, what: &str| -> Result<Vec<String>, String> {
        match v {
            None => Ok(Vec::new()),
            Some(v) => v
                .as_array()
                .ok_or(format!("{what}: expected an array"))?
                .iter()
                .map(|x| x.as_str().map(str::to_owned).ok_or(format!("{what}: expected strings")))
                .collect(),
        }
    };
    let mut table = Table::default();
    if let Some(groups) = doc.get("groups").and_then(toml::Value::as_table) {
        for (name, members) in groups {
            table.groups.insert(name.clone(), strings(Some(members), &format!("groups.{name}"))?);
        }
    }
    table.triples = strings(doc.get("targets").and_then(|t| t.get("triples")), "targets.triples")?;
    if let Some(crates) = doc.get("crates").and_then(toml::Value::as_table) {
        for (name, spec) in crates {
            let field = |key: &str| spec.get(key).ok_or(format!("crates.{name}: missing `{key}`"));
            let layer = field("layer")?
                .as_integer()
                .and_then(|l| u32::try_from(l).ok())
                .ok_or(format!("crates.{name}: `layer` must be a non-negative integer"))?;
            let kind = match field("kind")?.as_str() {
                Some("core") => Kind::Core,
                Some("adapter") => Kind::Adapter,
                Some("host") => Kind::Host,
                Some("testkit") => Kind::Testkit,
                Some("tool") => Kind::Tool,
                other => return Err(format!("crates.{name}: unknown kind {other:?}")),
            };
            let unsafe_policy = match field("unsafe")?.as_str() {
                Some("forbid") => UnsafePolicy::Forbid,
                Some("adapter") => UnsafePolicy::Adapter,
                other => return Err(format!("crates.{name}: unknown unsafe policy {other:?}")),
            };
            table.crates.insert(
                name.clone(),
                Spec {
                    layer,
                    kind,
                    unsafe_policy,
                    allow: strings(spec.get("allow"), &format!("crates.{name}.allow"))?,
                    forbid: strings(spec.get("forbid"), &format!("crates.{name}.forbid"))?,
                    forbid_transitive: strings(
                        spec.get("forbid_transitive"),
                        &format!("crates.{name}.forbid_transitive"),
                    )?,
                },
            );
        }
    }
    Ok(table)
}

fn pattern_matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == pattern,
    }
}

/// The first group in `groups` (in table order) that contains `dep`.
fn forbidden_group<'a>(table: &'a Table, groups: &'a [String], dep: &str) -> Option<&'a String> {
    groups.iter().find(|g| table.groups.get(*g).is_some_and(|ps| ps.iter().any(|p| pattern_matches(p, dep))))
}

/// Checks R1–R8. Returns sorted, deduplicated violations.
pub fn check(table: &Table, packages: &[Package], closures: &Closures) -> Vec<Violation> {
    let mut out = Vec::new();

    for (name, spec) in &table.crates {
        for allowed in &spec.allow {
            if !table.crates.contains_key(allowed) {
                out.push(Violation::UnknownReference { krate: name.clone(), name: allowed.clone() });
            }
        }
        for group in spec.forbid.iter().chain(&spec.forbid_transitive) {
            if !table.groups.contains_key(group) {
                out.push(Violation::UnknownReference { krate: name.clone(), name: group.clone() });
            }
        }
    }

    for package in packages {
        let Some(spec) = table.crates.get(&package.name) else {
            out.push(Violation::Unregistered { krate: package.name.clone() });
            continue;
        };
        if spec.unsafe_policy == UnsafePolicy::Forbid && !package.workspace_lints {
            out.push(Violation::LintsNotInherited { krate: package.name.clone() });
        }
        if spec.kind == Kind::Tool {
            continue;
        }
        for dep in &package.deps {
            if dep.name == package.name {
                continue;
            }
            if !dep.workspace {
                if dep.kind != DepKind::Dev
                    && let Some(group) = forbidden_group(table, &spec.forbid, &dep.name)
                {
                    out.push(Violation::ForbiddenDirect {
                        krate: package.name.clone(),
                        dep: dep.name.clone(),
                        group: group.clone(),
                    });
                }
                continue;
            }
            // Unregistered workspace dependencies are reported on their own entry (R1).
            let Some(target) = table.crates.get(&dep.name) else { continue };
            if matches!(target.kind, Kind::Host | Kind::Tool) {
                out.push(Violation::DependsOnLeaf { krate: package.name.clone(), dep: dep.name.clone() });
                continue;
            }
            if target.kind == Kind::Testkit {
                if dep.kind != DepKind::Dev {
                    out.push(Violation::TestkitAsNormalDep { krate: package.name.clone(), kind: dep.kind });
                }
                continue;
            }
            let upward = Violation::Upward {
                krate: package.name.clone(),
                dep: dep.name.clone(),
                from: spec.layer,
                to: target.layer,
                kind: dep.kind,
            };
            match dep.kind {
                DepKind::Dev => {
                    if target.layer > spec.layer {
                        out.push(upward);
                    }
                }
                DepKind::Normal | DepKind::Build => {
                    if target.layer >= spec.layer {
                        out.push(upward);
                    } else if !spec.allow.contains(&dep.name) {
                        out.push(Violation::NotAllowed {
                            krate: package.name.clone(),
                            dep: dep.name.clone(),
                            kind: dep.kind,
                        });
                    }
                }
            }
        }
        for (triple, per_crate) in closures {
            for dep in per_crate.get(&package.name).into_iter().flatten() {
                if let Some(group) = forbidden_group(table, &spec.forbid_transitive, dep) {
                    out.push(Violation::ForbiddenTransitive {
                        krate: package.name.clone(),
                        dep: dep.clone(),
                        group: group.clone(),
                        triple: triple.clone(),
                    });
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn dep_kind(v: &Value) -> DepKind {
    match v.as_str() {
        Some("dev") => DepKind::Dev,
        Some("build") => DepKind::Build,
        _ => DepKind::Normal,
    }
}

/// Workspace packages from `cargo metadata --format-version 1 --no-deps`; `lints` tells whether
/// a manifest path inherits workspace lints.
pub fn packages_from_metadata(meta: &Value, lints: impl Fn(&str) -> bool) -> Result<Vec<Package>, String> {
    let members: BTreeSet<&str> = meta["workspace_members"]
        .as_array()
        .ok_or("metadata: no workspace_members")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let pkgs = meta["packages"].as_array().ok_or("metadata: no packages array")?;
    let member_names: BTreeSet<&str> = pkgs
        .iter()
        .filter(|p| p["id"].as_str().is_some_and(|id| members.contains(id)))
        .filter_map(|p| p["name"].as_str())
        .collect();
    let mut out = Vec::new();
    for p in pkgs.iter().filter(|p| p["id"].as_str().is_some_and(|id| members.contains(id))) {
        let name = p["name"].as_str().ok_or("metadata: package without name")?.to_owned();
        let manifest = p["manifest_path"].as_str().ok_or("metadata: package without manifest_path")?;
        let deps = p["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| {
                let dname = d["name"].as_str()?.to_owned();
                let workspace = member_names.contains(dname.as_str()) || d["path"].is_string();
                Some(Dep { name: dname, kind: dep_kind(&d["kind"]), workspace })
            })
            .collect();
        out.push(Package { name, deps, workspace_lints: lints(manifest) });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// For each workspace member: external package names reachable over normal/build edges, from
/// `cargo metadata --format-version 1 --filter-platform <triple>`.
pub fn closure_from_metadata(meta: &Value) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let members: BTreeSet<&str> = meta["workspace_members"]
        .as_array()
        .ok_or("metadata: no workspace_members")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let names: BTreeMap<&str, &str> = meta["packages"]
        .as_array()
        .ok_or("metadata: no packages array")?
        .iter()
        .filter_map(|p| Some((p["id"].as_str()?, p["name"].as_str()?)))
        .collect();
    let mut edges: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for node in meta["resolve"]["nodes"].as_array().ok_or("metadata: no resolve graph")? {
        let Some(id) = node["id"].as_str() else { continue };
        let targets = node["deps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|d| d["dep_kinds"].as_array().into_iter().flatten().any(|k| dep_kind(&k["kind"]) != DepKind::Dev))
            .filter_map(|d| d["pkg"].as_str())
            .collect();
        edges.insert(id, targets);
    }
    let mut out = BTreeMap::new();
    for member in &members {
        let Some(name) = names.get(member) else { continue };
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut stack: Vec<&str> = edges.get(member).cloned().unwrap_or_default();
        while let Some(id) = stack.pop() {
            if seen.insert(id) {
                stack.extend(edges.get(id).into_iter().flatten().copied());
            }
        }
        let external = seen
            .into_iter()
            .filter(|id| !members.contains(id))
            .filter_map(|id| names.get(id).map(|n| (*n).to_owned()))
            .collect();
        out.insert((*name).to_owned(), external);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use DepKind::*;

    const TABLE: &str = r#"
        [meta]
        schema = 1
        [targets]
        triples = ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"]
        [groups]
        apple_native = ["objc2", "objc2-*"]
        gpu = ["wgpu", "wgpu-*"]
        async_runtime = ["tokio"]
        [crates.burnburn-types]
        layer = 0
        kind = "core"
        unsafe = "forbid"
        allow = []
        forbid = ["apple_native"]
        forbid_transitive = ["apple_native", "gpu", "async_runtime"]
        [crates.burnburn-geometry]
        layer = 1
        kind = "core"
        unsafe = "forbid"
        allow = ["burnburn-types"]
        forbid = []
        forbid_transitive = ["gpu"]
        [crates.burnburn-ruida]
        layer = 1
        kind = "core"
        unsafe = "forbid"
        allow = ["burnburn-types"]
        forbid = []
        forbid_transitive = []
        [crates.burnburn-render]
        layer = 5
        kind = "adapter"
        unsafe = "adapter"
        allow = ["burnburn-geometry"]
        forbid = ["apple_native"]
        forbid_transitive = []
        [crates.burnburn-macos]
        layer = 7
        kind = "host"
        unsafe = "adapter"
        allow = ["burnburn-render", "burnburn-geometry"]
        forbid = []
        forbid_transitive = []
        [crates.burnburn-test-support]
        layer = 6
        kind = "testkit"
        unsafe = "forbid"
        allow = ["burnburn-types"]
        forbid = []
        forbid_transitive = []
        [crates.xtask]
        layer = 99
        kind = "tool"
        unsafe = "forbid"
        allow = []
        forbid = []
        forbid_transitive = []
    "#;

    fn table() -> Table {
        parse_table(TABLE).unwrap()
    }

    fn p(name: &str, deps: &[(&str, DepKind)]) -> Package {
        Package {
            name: name.into(),
            deps: deps
                .iter()
                .map(|(n, k)| Dep {
                    name: (*n).into(),
                    kind: *k,
                    workspace: n.starts_with("burnburn-") || *n == "xtask",
                })
                .collect(),
            workspace_lints: true,
        }
    }

    fn run(packages: &[Package]) -> Vec<Violation> {
        check(&table(), packages, &Closures::new())
    }

    #[test]
    fn parses_design_table_fields() {
        let t = table();
        assert_eq!(t.triples, ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"]);
        assert_eq!(t.groups["apple_native"], ["objc2", "objc2-*"]);
        let g = &t.crates["burnburn-geometry"];
        assert_eq!((g.layer, g.kind, g.unsafe_policy), (1, Kind::Core, UnsafePolicy::Forbid));
        assert_eq!(g.allow, ["burnburn-types"]);
        assert_eq!(t.crates["burnburn-macos"].kind, Kind::Host);
    }

    #[test]
    fn parse_rejects_unknown_kind_and_missing_layer() {
        assert!(parse_table("[crates.a]\nlayer = 0\nkind = \"gui\"\nunsafe = \"forbid\"").is_err());
        assert!(parse_table("[crates.a]\nkind = \"core\"\nunsafe = \"forbid\"").is_err());
    }

    #[test]
    fn clean_downward_graph_passes() {
        let v = run(&[
            p("burnburn-types", &[("thiserror", Normal)]),
            p("burnburn-geometry", &[("burnburn-types", Normal), ("burnburn-test-support", Dev), ("proptest", Dev)]),
            p("burnburn-render", &[("burnburn-geometry", Normal), ("wgpu", Normal)]),
            p("burnburn-macos", &[("burnburn-render", Normal), ("objc2", Normal)]),
            p("burnburn-test-support", &[("burnburn-types", Normal)]),
            p("xtask", &[("serde_json", Normal)]),
        ]);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn unregistered_crate_is_error() {
        let v = run(&[p("burnburn-mystery", &[])]);
        assert_eq!(v, [Violation::Unregistered { krate: "burnburn-mystery".into() }]);
        assert!(v[0].to_string().contains("layers.toml"));
    }

    #[test]
    fn upward_normal_dependency_is_flagged() {
        // Lower layer -> higher layer, even when (wrongly) allowed.
        let mut t = table();
        t.crates.get_mut("burnburn-types").unwrap().allow.push("burnburn-geometry".into());
        let v = check(&t, &[p("burnburn-types", &[("burnburn-geometry", Normal)])], &Closures::new());
        assert!(matches!(&v[..], [Violation::Upward { from: 0, to: 1, kind: Normal, .. }]), "{v:?}");
    }

    #[test]
    fn same_layer_normal_dependency_is_flagged() {
        let mut t = table();
        t.crates.get_mut("burnburn-ruida").unwrap().allow.push("burnburn-geometry".into());
        let v = check(&t, &[p("burnburn-ruida", &[("burnburn-geometry", Normal)])], &Closures::new());
        assert!(matches!(&v[..], [Violation::Upward { from: 1, to: 1, .. }]), "{v:?}");
    }

    #[test]
    fn lower_but_not_allowed_dependency_is_flagged() {
        let v = run(&[p("burnburn-render", &[("burnburn-types", Normal)])]);
        assert!(matches!(&v[..], [Violation::NotAllowed { kind: Normal, .. }]), "{v:?}");
        let v = run(&[p("burnburn-render", &[("burnburn-types", Build)])]);
        assert!(matches!(&v[..], [Violation::NotAllowed { kind: Build, .. }]), "{v:?}");
    }

    #[test]
    fn dev_dependency_rules() {
        // Same layer is fine for dev, higher is not, allow-list does not apply to dev.
        assert!(run(&[p("burnburn-ruida", &[("burnburn-geometry", Dev)])]).is_empty());
        let v = run(&[p("burnburn-geometry", &[("burnburn-render", Dev)])]);
        assert!(matches!(&v[..], [Violation::Upward { from: 1, to: 5, kind: Dev, .. }]), "{v:?}");
        // test-support is the explicit exception.
        assert!(run(&[p("burnburn-types", &[("burnburn-test-support", Dev)])]).is_empty());
    }

    #[test]
    fn hosts_and_tools_are_never_dependencies() {
        let v = run(&[p("burnburn-test-support", &[("burnburn-macos", Dev)])]);
        assert!(
            v.contains(&Violation::DependsOnLeaf {
                krate: "burnburn-test-support".into(),
                dep: "burnburn-macos".into()
            }),
            "{v:?}"
        );
        let v = run(&[p("burnburn-macos", &[("xtask", Dev)])]);
        assert!(v.contains(&Violation::DependsOnLeaf { krate: "burnburn-macos".into(), dep: "xtask".into() }), "{v:?}");
    }

    #[test]
    fn test_support_only_as_dev_dependency() {
        let v = run(&[p("burnburn-geometry", &[("burnburn-test-support", Normal)])]);
        assert!(
            v.contains(&Violation::TestkitAsNormalDep { krate: "burnburn-geometry".into(), kind: Normal }),
            "{v:?}"
        );
    }

    #[test]
    fn forbidden_direct_groups_with_prefix_patterns() {
        for dep in ["objc2", "objc2-app-kit"] {
            let v = run(&[p("burnburn-types", &[(dep, Normal)])]);
            assert!(
                matches!(&v[..], [Violation::ForbiddenDirect { group, .. }] if group == "apple_native"),
                "{dep}: {v:?}"
            );
        }
        // Prefix `objc2-*` does not match `objc2x`; dev-dependencies are not R5.
        assert!(run(&[p("burnburn-types", &[("objc2x", Normal)])]).is_empty());
        assert!(run(&[p("burnburn-types", &[("objc2", Dev)])]).is_empty());
    }

    #[test]
    fn forbidden_transitive_is_reported_per_triple() {
        let mut closures = Closures::new();
        closures
            .entry("x86_64-pc-windows-msvc".into())
            .or_default()
            .insert("burnburn-geometry".into(), ["serde".to_string(), "wgpu-core".to_string()].into_iter().collect());
        closures
            .entry("aarch64-apple-darwin".into())
            .or_default()
            .insert("burnburn-types".into(), ["tokio".to_string()].into_iter().collect());
        let v = check(
            &table(),
            &[p("burnburn-types", &[]), p("burnburn-geometry", &[("burnburn-types", Normal)])],
            &closures,
        );
        assert_eq!(
            v,
            [
                Violation::ForbiddenTransitive {
                    krate: "burnburn-geometry".into(),
                    dep: "wgpu-core".into(),
                    group: "gpu".into(),
                    triple: "x86_64-pc-windows-msvc".into()
                },
                Violation::ForbiddenTransitive {
                    krate: "burnburn-types".into(),
                    dep: "tokio".into(),
                    group: "async_runtime".into(),
                    triple: "aarch64-apple-darwin".into()
                },
            ]
        );
    }

    #[test]
    fn forbid_crates_must_inherit_workspace_lints() {
        let mut types = p("burnburn-types", &[]);
        types.workspace_lints = false;
        let mut render = p("burnburn-render", &[]);
        render.workspace_lints = false;
        assert_eq!(run(&[types, render]), [Violation::LintsNotInherited { krate: "burnburn-types".into() }]);
    }

    #[test]
    fn tool_is_exempt_from_dependency_rules() {
        assert!(run(&[p("xtask", &[("tokio", Normal), ("burnburn-macos", Normal)])]).is_empty());
    }

    #[test]
    fn unknown_names_in_table_are_reported() {
        let mut t = table();
        t.crates.get_mut("burnburn-ruida").unwrap().allow.push("burnburn-nope".into());
        t.crates.get_mut("burnburn-ruida").unwrap().forbid.push("no_such_group".into());
        let v = check(&t, &[], &Closures::new());
        assert_eq!(v.len(), 2, "{v:?}");
        assert!(v.iter().all(|x| matches!(x, Violation::UnknownReference { .. })));
    }

    #[test]
    fn metadata_parsing_builds_model() {
        let meta: Value = serde_json::from_str(
            r#"{"workspace_members":["a 0.1.0 (path+file:///w/crates/burnburn-geometry)","b"],
                "packages":[
                {"name":"burnburn-geometry","id":"a 0.1.0 (path+file:///w/crates/burnburn-geometry)",
                 "manifest_path":"/w/crates/burnburn-geometry/Cargo.toml","dependencies":[
                    {"name":"burnburn-types","kind":null,"path":"/w/crates/burnburn-types"},
                    {"name":"serde","kind":null},
                    {"name":"proptest","kind":"dev"},
                    {"name":"cc","kind":"build"}]},
                {"name":"burnburn-types","id":"b","manifest_path":"/w/crates/burnburn-types/Cargo.toml","dependencies":[]}
            ]}"#,
        )
        .unwrap();
        let pkgs = packages_from_metadata(&meta, |path| path.contains("geometry")).unwrap();
        assert_eq!(pkgs.len(), 2);
        let geo = &pkgs[0];
        assert_eq!(geo.name, "burnburn-geometry");
        assert!(geo.workspace_lints && !pkgs[1].workspace_lints);
        let kinds: Vec<_> = geo.deps.iter().map(|d| (d.name.as_str(), d.kind, d.workspace)).collect();
        assert_eq!(
            kinds,
            [
                ("burnburn-types", Normal, true),
                ("serde", Normal, false),
                ("proptest", Dev, false),
                ("cc", Build, false)
            ]
        );
    }

    #[test]
    fn closure_follows_normal_and_build_edges_but_not_dev() {
        let meta: Value = serde_json::from_str(
            r#"{"workspace_members":["ws"],
                "packages":[{"id":"ws","name":"burnburn-geometry"},{"id":"s","name":"serde"},
                            {"id":"d","name":"serde_derive"},{"id":"pt","name":"proptest"},
                            {"id":"cc","name":"cc"},{"id":"r","name":"rand"}],
                "resolve":{"nodes":[
                  {"id":"ws","deps":[
                    {"pkg":"s","dep_kinds":[{"kind":null}]},
                    {"pkg":"pt","dep_kinds":[{"kind":"dev"}]},
                    {"pkg":"cc","dep_kinds":[{"kind":"build"}]}]},
                  {"id":"s","deps":[{"pkg":"d","dep_kinds":[{"kind":null}]}]},
                  {"id":"pt","deps":[{"pkg":"r","dep_kinds":[{"kind":null}]}]},
                  {"id":"d","deps":[]},{"id":"cc","deps":[]},{"id":"r","deps":[]}]}}"#,
        )
        .unwrap();
        let c = closure_from_metadata(&meta).unwrap();
        let geo: Vec<_> = c["burnburn-geometry"].iter().map(String::as_str).collect();
        assert_eq!(geo, ["cc", "serde", "serde_derive"]);
    }
}
