//! X01 (G6-editor): the core of the app works without any network. The flows run on ports that have
//! no network (an in-memory disk, a queued task port, a scripted local controller); the app cannot
//! reach the internet because nothing in it can open a socket except the machine transports and
//! the loopback-only desktop automation; and a missing optional service (a damaged library file,
//! no fonts, no camera, no journal) blocks no local project.
//!
//! Not covered here (manual / not built): "updates are applied outside an active transfer" - the
//! app has no updater yet - and the lsof check of a real window (the native test
//! `app_panels_open_no_network_socket` does that).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod acc_machine;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use acc_machine::*;
use burnburn_geometry::{Point, Rect};
use burnburn_io::fs::FileSystem;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid, load_project, save_project};
use burnburn_machine::Request;
use burnburn_presentation::{
    AppController, AppPorts, CommandKey, HeadlessEditorHarness, HostEffect, MachineStatusKey, QueuedTaskPort, UiIntent,
};
use burnburn_types::{CancellationToken, DocumentId, Length, RequestId, StableId, TaskId};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// ---------------------------------------------------------------------------------------------
// The core flows on ports without a network
// ---------------------------------------------------------------------------------------------

/// The controller is built from a task port, a file system and an identity - nothing else. The
/// exhaustive pattern stops compiling the day a network port is added to `AppPorts`, and this test
/// has to be read again.
#[test]
fn x01_the_app_controller_needs_a_task_port_a_file_system_and_an_identity_and_nothing_else() {
    let ports = AppPorts {
        tasks: Arc::new(QueuedTaskPort::default()),
        fs: Arc::new(MemoryFileSystem::new()),
        new_project_uuid: Arc::new(|| ProjectUuid::from_bytes([1; 16])),
    };
    let AppPorts { tasks: _, fs: _, new_project_uuid: _ } = &ports;
    let mut controller = AppController::new(ports);
    let update = controller.handle(UiIntent::NewDocument);
    assert!(update.effects.iter().any(|e| matches!(e, HostEffect::OpenWindow { .. })), "a window opens");
}

#[test]
fn x01_open_draw_trace_compile_and_the_local_machine_work_with_no_network_port() {
    // Open: a project saved on the in-memory disk comes back in a window.
    let mut r = rig();
    r.h.fs().create_dir_all_for_test(Path::new("/werk"));
    let rect = Rect::new(Point::new(0.0, 0.0), Length::from_mm(20.0).unwrap(), Length::from_mm(10.0).unwrap()).unwrap();
    let mut session = burnburn_document::DocumentSession::new(DocumentId::from_raw(9));
    let cmd = burnburn_document::DocumentCommand {
        document: session.id(),
        expected: session.revision(),
        request: RequestId::from_raw(1),
        payload: burnburn_document::CommandPayload::CreateRectangle { rect, name: "Plaatje".into() },
    };
    session.apply(cmd).unwrap();
    let path = PathBuf::from("/werk/plaatje.burnburn");
    save_project(r.h.fs(), &path, &Project::new(ProjectUuid::from_bytes([7; 16]), (*session.snapshot()).clone()))
        .unwrap();
    assert!(load_project(r.h.fs(), &path).is_ok());
    r.h.send(UiIntent::Open { path });
    let opened = r.h.run_tasks().iter().flat_map(|u| u.effects.clone()).any(|e| matches!(e, HostEffect::OpenWindow { .. }));
    assert!(opened, "open works on the local disk");

    // Draw: the rig's window already holds a drawn rectangle and a cut operation.
    assert_eq!(r.h.document(r.doc).unwrap().objects.len(), 1);

    // Trace: a bitmap becomes contours on this machine (no service is asked).
    let traced = trace_a_square();
    assert!(traced >= 1, "{traced} contours");

    // Compile, prepare and upload to the local controller; the scripted peer is the only far end.
    let job = prepare(&mut r.h, r.doc);
    upload_and_store(&mut r, &job);
    assert_eq!(r.h.machine_view().expect("machine panel").status, MachineStatusKey::Stored);
    let sent = r.peer.sent();
    assert!(
        sent.iter().all(|q| matches!(q, Request::QueryIdentity | Request::UploadChunk { .. } | Request::QueryStored)),
        "only the machine conversation left the app: {sent:?}"
    );
    assert_eq!(starts_sent(&r.peer), 0, "and nothing started by itself");
}

fn trace_a_square() -> usize {
    use burnburn_image::{AlphaPolicy, ImageRecipe, PixelScale, SourceImage, Threshold};
    use burnburn_trace::{FitSettings, TraceRecipe, TraceRequest, trace};
    let (w, h) = (40usize, 40usize);
    let mut rgba = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let ink = (10..30).contains(&x) && (10..30).contains(&y);
            rgba.extend_from_slice(if ink { &[0, 0, 0, 255] } else { &[255, 255, 255, 255] });
        }
    }
    let recipe = TraceRecipe {
        image: ImageRecipe {
            scale: Some(PixelScale::uniform_mm_per_pixel(0.1).unwrap()),
            threshold: Threshold::Fixed(127),
            alpha: AlphaPolicy::default(),
            ..ImageRecipe::default()
        },
        fit: FitSettings { tolerance_mm: 0.02, ..FitSettings::default() },
        ..TraceRecipe::default()
    };
    let request = TraceRequest {
        task: TaskId::from_raw(1),
        source_revision: burnburn_types::RevisionId::INITIAL,
        source: Arc::new(SourceImage::from_rgba8(w as u32, h as u32, rgba).unwrap()),
        recipe,
    };
    trace(&request, &CancellationToken::never()).unwrap().contours.len()
}

// ---------------------------------------------------------------------------------------------
// Nothing in the app can look for the internet
// ---------------------------------------------------------------------------------------------

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files_under(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn sources() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let crates = repo().join("crates");
    for entry in std::fs::read_dir(&crates).unwrap() {
        let src = entry.unwrap().path().join("src");
        if src.is_dir() {
            files_under(&src, &mut out);
        }
    }
    files_under(&repo().join("xtask/src"), &mut out);
    out
}

/// No HTTP client, TLS stack, resolver or socket crate is anywhere in the lock file, so no feature
/// of any crate can reach a server (a crate that would need one has to be added here on purpose).
#[test]
fn x01_the_lock_file_has_no_http_tls_resolver_or_socket_crate() {
    const DENIED: &[&str] = &[
        "reqwest",
        "hyper",
        "hyper-util",
        "ureq",
        "curl",
        "curl-sys",
        "isahc",
        "attohttpc",
        "surf",
        "async-std",
        "smol",
        "h2",
        "native-tls",
        "rustls",
        "openssl",
        "openssl-sys",
        "schannel",
        "security-framework",
        "hickory-resolver",
        "trust-dns-resolver",
        "mio",
        "socket2",
        "tungstenite",
        "tokio-tungstenite",
        "webpki",
    ];
    let lock = std::fs::read_to_string(repo().join("Cargo.lock")).unwrap();
    let names: Vec<&str> = lock.lines().filter_map(|l| l.strip_prefix("name = \"")?.strip_suffix('"')).collect();
    assert!(names.len() > 50, "the lock file was read");
    let found: Vec<&&str> = DENIED.iter().filter(|d| names.contains(d)).collect();
    assert!(found.is_empty(), "network crates in the dependency graph: {found:?}");
}

/// Only the Ruida UDP transport (the laser on the local network) and the loopback-only desktop
/// automation create sockets. Everything else - the core, the editor, presentation, both hosts -
/// has no socket constructor.
#[test]
fn x01_only_the_machine_transport_and_the_loopback_automation_create_sockets() {
    const SOCKET: &[&str] = &["UdpSocket::bind", "TcpStream::connect", "TcpListener::bind", "ToSocketAddrs", "TcpStream::"];
    let allowed = ["crates/burnburn-runtime/src/udp.rs", "crates/burnburn-runtime/src/automation.rs"];
    let root = repo().canonicalize().unwrap();
    let mut offenders = Vec::new();
    for file in sources() {
        let relative = file.canonicalize().unwrap();
        let relative = relative.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        if allowed.contains(&relative.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        for (n, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if !code.starts_with("//") && SOCKET.iter().any(|s| code.contains(s)) {
                offenders.push(format!("{relative}:{}: {}", n + 1, code.trim()));
            }
        }
    }
    assert!(offenders.is_empty(), "sockets outside the machine transport and the automation:\n{}", offenders.join("\n"));
}

/// No source file carries a web address: nothing is fetched from a built-in server. The only URLs
/// are the XML namespaces and DOCTYPE of the files the app writes (SVG, manifests, plists).
#[test]
fn x01_no_source_file_contains_a_web_address_but_namespaces_and_doctypes() {
    let mut offenders = Vec::new();
    for file in sources() {
        let text = std::fs::read_to_string(&file).unwrap();
        for (n, line) in text.lines().enumerate() {
            let code = line.trim_start();
            let is_comment = code.starts_with("//");
            let is_namespace = line.contains("xmlns") || line.contains("DOCTYPE");
            if !is_comment && !is_namespace && (line.contains("http://") || line.contains("https://")) {
                offenders.push(format!("{}:{}: {}", file.display(), n + 1, code));
            }
        }
    }
    assert!(offenders.is_empty(), "web addresses in code:\n{}", offenders.join("\n"));
}

// ---------------------------------------------------------------------------------------------
// A missing optional service blocks no local project
// ---------------------------------------------------------------------------------------------

/// A damaged library file, no fonts, no camera and no journal: the project still opens, is edited,
/// compiled and saved. The library says it is unreadable and is left alone on disk.
#[test]
fn x01_missing_optional_services_block_no_local_project() {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(Path::new("/gebruiker"));
    h.fs().insert(Path::new("/gebruiker/material-library.json"), b"{ dit is stuk".to_vec());
    h.fs().insert(Path::new("/gebruiker/art-library.json"), b"\xff\xfe".to_vec());
    h.controller().set_library_dir(Some(PathBuf::from("/gebruiker")));
    let doc = h.new_document().unwrap();
    // The optional panels open, say what is wrong and leave the files alone.
    h.invoke(doc, CommandKey::ShowMaterialLibrary);
    h.invoke(doc, CommandKey::ShowArtLibrary);
    h.run_tasks();
    let damaged_before = h.fs().read(Path::new("/gebruiker/material-library.json"), u64::MAX).unwrap();
    // The project itself: draw, add an operation, compile, save, reopen.
    design(&mut h, doc);
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let view = h.document(doc).unwrap();
    assert_eq!(view.objects.len(), 1);
    h.fs().create_dir_all_for_test(Path::new("/werk"));
    let path = PathBuf::from("/werk/zonder-diensten.burnburn");
    h.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    h.run_tasks();
    assert!(load_project(h.fs(), &path).is_ok(), "saved and readable without any optional service");
    assert_eq!(
        h.fs().read(Path::new("/gebruiker/material-library.json"), u64::MAX).unwrap(),
        damaged_before,
        "a library that cannot be read is never overwritten"
    );
    h.send(UiIntent::Open { path });
    let reopened = h.run_tasks().iter().flat_map(|u| u.effects.clone()).any(|e| matches!(e, HostEffect::OpenWindow { .. }));
    assert!(reopened, "and it opens again");
}
