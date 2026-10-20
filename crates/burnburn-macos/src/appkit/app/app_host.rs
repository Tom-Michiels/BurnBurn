//! The host's part of the app-level panels (W3-UI-app): performing the menu items of the
//! application, Window and Help menus. The panels themselves live in `appkit::app_panels`.

use crate::app_menu::AppAction;

use super::HostApp;

impl HostApp {
    /// A menu item that opens an app-level panel or dialog.
    pub(crate) fn perform_app_action(&self, _action: AppAction) {}
}
