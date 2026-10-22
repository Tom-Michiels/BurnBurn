//! Menu mnemonics (`&File`, Alt+F): which letter of a title is underlined. Pure text work,
//! tested on every OS; `win32::menu` writes the result into the captions.

use std::collections::{HashMap, HashSet};

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// `title` as a menu caption: a literal `&` is doubled and the character at `at` (a char index)
/// gets the mnemonic `&`.
fn caption(title: &str, at: Option<usize>) -> String {
    let mut out = String::with_capacity(title.len() + 2);
    for (i, c) in title.chars().enumerate() {
        if at == Some(i) {
            out.push('&');
        }
        if c == '&' {
            out.push('&');
        }
        out.push(c);
    }
    out
}

/// A letter a title can take and where it sits (char index).
type Letter = (char, usize);

/// The letters of a title, best first: letters that start a word, then the others; each once.
fn candidates(title: &str) -> Vec<Letter> {
    let chars: Vec<char> = title.chars().collect();
    let word_start = |i: usize| i == 0 || !chars.get(i - 1).is_some_and(|c| c.is_alphanumeric());
    let mut out: Vec<Letter> = Vec::new();
    for starts in [true, false] {
        for (i, c) in chars.iter().enumerate() {
            if c.is_alphanumeric() && word_start(i) == starts && !out.iter().any(|(l, _)| *l == fold(*c)) {
                out.push((fold(*c), i));
            }
        }
    }
    out
}

struct Plan {
    options: Vec<Vec<Letter>>,
    /// Which title holds a letter.
    taken: HashMap<char, usize>,
    chosen: Vec<Option<Letter>>,
}

impl Plan {
    /// Gives title `item` a letter: a free one, or one whose holder can move to another letter.
    fn augment(&mut self, item: usize, seen: &mut HashSet<char>) -> bool {
        for (letter, at) in self.options[item].clone() {
            if !seen.insert(letter) {
                continue;
            }
            let holder = self.taken.get(&letter).copied();
            if holder.is_none_or(|h| h != item && self.augment(h, seen)) {
                self.taken.insert(letter, item);
                self.chosen[item] = Some((letter, at));
                return true;
            }
        }
        false
    }
}

/// `titles` of one menu level, each as a caption with the `&` before its mnemonic letter. A letter
/// is used once per level (case-insensitive). Titles take their best free letter in order; one
/// that is left without a letter makes an earlier title move to another letter when that helps.
pub fn assign(titles: &[String]) -> Vec<String> {
    let options: Vec<Vec<Letter>> = titles.iter().map(|t| candidates(t)).collect();
    let mut plan = Plan { chosen: vec![None; titles.len()], taken: HashMap::new(), options };
    for item in 0..titles.len() {
        let free = plan.options[item].iter().find(|(l, _)| !plan.taken.contains_key(l)).copied();
        if let Some((letter, at)) = free {
            plan.taken.insert(letter, item);
            plan.chosen[item] = Some((letter, at));
        }
    }
    for item in 0..titles.len() {
        if plan.chosen[item].is_none() {
            plan.augment(item, &mut HashSet::new());
        }
    }
    titles.iter().zip(&plan.chosen).map(|(title, chosen)| caption(title, chosen.map(|(_, at)| at))).collect()
}

/// The letter after the single `&` of a caption (`&&` is a literal ampersand).
fn mnemonic_of(caption: &str) -> Option<char> {
    let mut chars = caption.chars();
    while let Some(c) = chars.next() {
        if c == '&' {
            match chars.next() {
                Some('&') => {}
                other => return other,
            }
        }
    }
    None
}

/// `title` with the mnemonic letter of `previous` (an earlier caption of the same item, with its
/// `&`), so a title that changes (the undo name) keeps its underlined letter.
pub fn reapply(previous: &str, title: &str) -> String {
    let at = mnemonic_of(previous)
        .map(fold)
        .and_then(|wanted| title.chars().position(|c| c.is_alphanumeric() && fold(c) == wanted));
    caption(title, at)
}

/// The caption of recent-list entry `n` (0-based): numbered like Windows' own recent lists, with
/// the digit as the mnemonic for the first nine.
pub fn numbered(n: usize, title: &str) -> String {
    let shown = caption(title, None);
    if n < 9 { format!("&{} {shown}", n + 1) } else { format!("{} {shown}", n + 1) }
}
