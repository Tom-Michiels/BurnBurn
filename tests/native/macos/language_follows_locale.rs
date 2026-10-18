//! IM20 localisation: with English chosen, the main menu, the close sheet and the panels speak
//! English; the strings come from the one Rust catalogue (`burnburn_macos::l10n`). Without an
//! explicit choice the host follows `NSLocale.preferredLanguages`.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("language_follows_locale: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("menus_sheets_and_panels_follow_the_chosen_language", scenario())
}

#[cfg(target_os = "macos")]
fn scenario() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::appkit::system_locale;
    use burnburn_macos::l10n::{Locale, locale_from_preferred};
    use burnburn_presentation::CommandKey;
    use common::expect;
    use objc2_foundation::NSLocale;

    let preferred: Vec<String> = NSLocale::preferredLanguages().iter().map(|l| l.to_string()).collect();
    let system = system_locale();
    expect(
        || format!("system locale {system:?} from {preferred:?}"),
        system == locale_from_preferred(preferred.iter().map(String::as_str)),
    )?;

    let s = common::start_with(|o| o.locale = Some(Locale::En))?;
    let titles: Vec<String> = (0..s.app.menu().numberOfItems())
        .filter_map(|i| s.app.menu().itemAtIndex(i))
        .map(|i| i.title().to_string())
        .collect();
    expect(
        || format!("menu bar {titles:?}"),
        titles.starts_with(&["BurnBurn".to_owned(), "File".to_owned(), "Edit".to_owned()]),
    )?;
    let new = s.menu_item_state(MenuItem::Command(CommandKey::New)).map(|(_, t)| t);
    expect(|| format!("New item {new:?}"), new.as_deref() == Some("New"))?;

    let doc = s.new_document()?;
    s.draw_rectangle(doc, (0.0, 0.0), (40.0, 20.0))?;
    let undo = s.menu_item_state(MenuItem::Command(CommandKey::Undo)).map(|(_, t)| t);
    expect(
        || format!("undo title {undo:?}"),
        undo.as_deref() == Some("Undo Draw Shape") || undo.as_deref() == Some("Undo Draw Rectangle"),
    )?;

    s.app.window(doc).ok_or("no window")?.performClose(None);
    s.wait_close_alert(doc)?;
    let (message, buttons) = s
        .app
        .with_window(doc, |w| {
            w.close_alert.map(|a| {
                (a.messageText().to_string(), a.buttons().iter().map(|b| b.title().to_string()).collect::<Vec<_>>())
            })
        })
        .flatten()
        .ok_or("no alert")?;
    expect(|| format!("message {message:?}"), message.starts_with("Do you want to save the changes made to “"))?;
    expect(|| format!("buttons {buttons:?}"), buttons == ["Save", "Cancel", "Don’t Save"])?;
    s.click_close_alert(doc, 1)?;
    s.pump(0.2);
    let column =
        s.app.with_window(doc, |w| w.table.tableColumns().firstObject().map(|c| c.title().to_string())).flatten();
    expect(|| format!("object column {column:?}"), column.as_deref() == Some("Objects"))?;
    Ok(())
}
