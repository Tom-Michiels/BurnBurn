//! Headless models both native hosts (macOS and Windows) show: the one NL/EN catalogue, the panel
//! view models and their enablement, toolbar texts, the recent-projects list, the quit guard
//! and the automation pump. No AppKit, Windows, GPU or UI-toolkit type appears here (ADR 0009); the
//! hosts only draw what these modules decide.

pub mod app_text;
pub mod app_view;
pub mod automation_host;
pub mod hub_panel;
pub mod l10n;
pub mod l10n_files;
pub mod l10n_g5;
pub mod l10n_g6;
pub mod l10n_g6_werk;
pub mod l10n_g7_editor;
pub mod l10n_g7_trace;
pub mod l10n_g7_werk;
pub mod l10n_machine;
pub mod panels;
pub mod panels_files;
pub mod panels_image;
pub mod quit_guard;
pub mod recent;
pub mod toolbar;
