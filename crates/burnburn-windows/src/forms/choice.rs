//! A pop-up whose blocked items cannot be chosen. A drop-down list of the system cannot grey one
//! item, so the blocked ones are left out of the list that is shown (the one currently selected
//! stays, so the control can show it) and named in the control's tooltip. This maps between the
//! shown positions and the form's item numbers.

use super::ChoiceItem;

/// The list a drop-down control shows for a form's [`ChoiceItem`]s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceList {
    /// The titles in the drop-down, in order.
    pub titles: Vec<String>,
    /// The form's item number behind each shown position.
    pub items: Vec<usize>,
    /// The shown position of the form's selected item.
    pub selected: Option<usize>,
    /// The blocked items with their reasons (the titles already say why), for the tooltip; empty
    /// when nothing is blocked.
    pub blocked_note: String,
}

impl ChoiceList {
    pub fn new(items: &[ChoiceItem], selected: usize) -> ChoiceList {
        let mut list =
            ChoiceList { titles: Vec::new(), items: Vec::new(), selected: None, blocked_note: String::new() };
        let mut blocked: Vec<&str> = Vec::new();
        for (index, item) in items.iter().enumerate() {
            if !item.enabled && index != selected {
                blocked.push(&item.title);
                continue;
            }
            if index == selected {
                list.selected = Some(list.items.len());
            }
            list.titles.push(item.title.clone());
            list.items.push(index);
        }
        list.blocked_note = blocked.join("\n");
        list
    }

    /// The form's item number for the shown position `position`.
    pub fn item_at(&self, position: usize) -> Option<usize> {
        self.items.get(position).copied()
    }
}
