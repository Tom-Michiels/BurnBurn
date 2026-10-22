//! Desktop automation (API01) in the real application object: the listener thread exists only while
//! the preference "Desktop-automatisering" is on, answers on 127.0.0.1 only, and carries out
//! LOADFILE / IMPORT through the host's own dispatcher on the main thread. A machine verb is
//! refused and the host never receives a machine intent.
//!
//! Manual (not covered here): the preference row in the Voorkeuren panel with a real click, and a
//! client program outside this process (the test client is a UDP socket in this process).

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("automation: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("automation_follows_the_preference_and_runs_requests_on_the_main_thread", scenario())
}

/// Sends `request` from a loopback client and pumps the main thread until the reply is there.
#[cfg(target_os = "macos")]
fn ask(
    s: &common::Session,
    client: &std::net::UdpSocket,
    addr: std::net::SocketAddr,
    request: &str,
) -> Result<Option<String>, String> {
    client.send_to(request.as_bytes(), addr).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 256];
    for _ in 0..200 {
        s.pump(0.02);
        if let Ok((n, _)) = client.recv_from(&mut buf) {
            return Ok(Some(String::from_utf8_lossy(&buf[..n]).into_owned()));
        }
    }
    Ok(None)
}

#[cfg(target_os = "macos")]
fn scenario() -> Result<(), String> {
    use burnburn_io::{Project, ProjectUuid, save_project};
    use burnburn_presentation::{AppIntent, PrefKey, UiIntent};
    use common::expect;

    let s = common::start()?;
    let user = s.dir.join("user");
    std::fs::create_dir_all(&user).map_err(|e| e.to_string())?;
    s.app.use_user_dir(user);
    s.app.set_automation_port(0);
    expect(|| "automation is off by default: no listener thread".into(), !s.app.automation_running())?;

    let doc = s.new_document()?;
    s.app.dispatch(UiIntent::App(AppIntent::SetPreference { key: PrefKey::DesktopAutomation, text: "on".into() }));
    s.wait(3.0, "the listener", || s.app.automation_running())?;
    let addr = s.app.automation_addr().ok_or("no address")?;
    expect(|| format!("loopback only, got {addr}"), addr.ip().is_loopback())?;
    let client = std::net::UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    client.set_nonblocking(true).map_err(|e| e.to_string())?;

    expect(|| "PING".into(), ask(&s, &client, addr, "PING")? == Some("OK:PONG".into()))?;
    let status = ask(&s, &client, addr, "STATUS")?;
    expect(|| format!("a clean untitled document, got {status:?}"), status.as_deref() == Some("OK:READY:CLEAN"))?;

    // IMPORT adds a drawing to the active document as one undo step.
    let svg = s.dir.join("logo.svg");
    std::fs::write(
        &svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="40mm" height="20mm" viewBox="0 0 40 20"><rect x="0" y="0" width="40" height="20" fill="none" stroke="black"/></svg>"#,
    )
    .map_err(|e| e.to_string())?;
    let reply = ask(&s, &client, addr, &format!("IMPORT:{}", svg.display()))?;
    expect(|| format!("import, got {reply:?}"), reply.as_deref() == Some("OK"))?;
    let vm = s.doc(doc).ok_or("no document")?;
    expect(|| format!("one object arrived, got {}", vm.objects.len()), vm.objects.len() == 1)?;
    expect(|| "the document is now changed".into(), vm.dirty)?;
    let status = ask(&s, &client, addr, "STATUS")?;
    expect(|| format!("dirty, got {status:?}"), status.as_deref() == Some("OK:READY:DIRTY"))?;

    // LOADFILE never replaces unsaved work.
    let project = s.dir.join("a.burnburn");
    let empty = burnburn_document::DocumentSnapshot::from_parts(Vec::new(), 1).map_err(|e| format!("{e:?}"))?;
    save_project(&burnburn_io::std_fs::StdFileSystem, &project, &Project::new(ProjectUuid::from_bytes([9; 16]), empty))
        .map_err(|e| format!("{e:?}"))?;
    let before = s.app.open_documents();
    let reply = ask(&s, &client, addr, &format!("LOADFILE:{}", project.display()))?;
    expect(|| format!("refused, got {reply:?}"), reply.as_deref() == Some("ERR:DIRTY"))?;
    expect(|| "no window opened".into(), s.app.open_documents() == before)?;

    // Machine verbs are refused, and the host never got a machine intent.
    s.app.take_dispatched();
    for verb in ["START", "FRAME:/x.burnburn", "JOG", "HOME", "UPLOAD", "SEND:/x.rd"] {
        let reply = ask(&s, &client, addr, verb)?;
        expect(|| format!("{verb} is forbidden, got {reply:?}"), reply.as_deref() == Some("ERR:FORBIDDEN"))?;
    }
    let sent = s.app.take_dispatched();
    expect(|| format!("the verbs sent no intent, got {sent:?}"), sent.is_empty())?;

    // After a save the document is clean and LOADFILE opens the project in a window.
    s.app.dispatch(UiIntent::Save { document: doc, path: Some(s.dir.join("saved.burnburn")) });
    s.wait(5.0, "the save", || s.doc(doc).is_some_and(|d| !d.dirty))?;
    let status = ask(&s, &client, addr, "STATUS")?;
    expect(|| format!("after the save it is clean, got {status:?}"), status.as_deref() == Some("OK:READY:CLEAN"))?;
    s.app.take_dispatched();
    let reply = ask(&s, &client, addr, &format!("LOADFILE:{}", project.display()))?;
    expect(|| format!("loaded, got {reply:?}"), reply.as_deref() == Some("OK"))?;
    // The clean window is replaced by the project (G6-editor): the window count stays, the old
    // window is gone.
    s.wait(3.0, "the old window to close", || s.doc(doc).is_none())?;
    expect(|| "the project's window took its place".into(), s.app.open_documents().len() == before.len())?;
    let intents = s.app.take_dispatched();
    expect(
        || format!("exactly one Open and one Close were sent, got {intents:?}"),
        intents.iter().filter(|i| matches!(i, UiIntent::Open { .. })).count() == 1
            && intents.iter().filter(|i| matches!(i, UiIntent::Close { .. })).count() == 1,
    )?;
    // Nothing that reaches a machine: no machine or laser intent in the whole session.
    expect(
        || format!("no machine intent, got {intents:?}"),
        !intents
            .iter()
            .any(|i| matches!(i, UiIntent::Machine { .. } | UiIntent::Laser { .. } | UiIntent::LaserSetup { .. })),
    )?;

    // Off again: the thread ends and nothing answers.
    s.app.dispatch(UiIntent::App(AppIntent::SetPreference { key: PrefKey::DesktopAutomation, text: "off".into() }));
    s.wait(3.0, "the listener to stop", || !s.app.automation_running())?;
    let reply = ask(&s, &client, addr, "PING")?;
    expect(|| format!("silence when off, got {reply:?}"), reply.is_none())?;
    Ok(())
}
