//! Colour roles from `palette.toml`: each role has a stable `source` hex (what the SVG masters
//! contain), a `light` and a `dark` display value. Accents and colour-vision variants of the
//! SolidCraft palette are not used by BurnBurn.

use crate::{IconError, Theme};

#[derive(Debug, Clone)]
struct Role {
    name: String,
    source: [u8; 3],
    light: [u8; 3],
    dark: [u8; 3],
}

/// The colour roles of the icon set.
#[derive(Debug, Clone)]
pub struct Palette {
    roles: Vec<Role>,
    line: usize,
    gap: usize,
    guide: usize,
}

fn hex(v: Option<&toml::Value>, what: &str) -> Result<[u8; 3], IconError> {
    let s = v.and_then(toml::Value::as_str).ok_or_else(|| IconError::Palette(format!("{what}: missing hex colour")))?;
    let digits = s.strip_prefix('#').filter(|d| d.len() == 6 && d.bytes().all(|b| b.is_ascii_hexdigit()));
    let digits = digits.ok_or_else(|| IconError::Palette(format!("{what}: `{s}` is not #rrggbb")))?;
    let byte = |i: usize| {
        digits
            .get(i..i + 2)
            .and_then(|h| u8::from_str_radix(h, 16).ok())
            .ok_or_else(|| IconError::Palette(format!("{what}: `{s}`")))
    };
    Ok([byte(0)?, byte(2)?, byte(4)?])
}

impl Palette {
    /// Parse the `[roles.*]` tables of a `palette.toml`. The roles `line` (for `currentColor`),
    /// `gap` (knock-out) and `guide` (dashed references) are required.
    pub fn parse(src: &str) -> Result<Palette, IconError> {
        let table: toml::Table = src.parse().map_err(|e: toml::de::Error| IconError::Palette(e.to_string()))?;
        let roles = table
            .get("roles")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| IconError::Palette("no [roles] table".into()))?;
        let mut out = Vec::with_capacity(roles.len());
        for (name, role) in roles {
            let light = hex(role.get("light"), &format!("role {name} light"))?;
            let dark = hex(role.get("dark"), &format!("role {name} dark"))?;
            let source = hex(role.get("source").or_else(|| role.get("light")), &format!("role {name} source"))?;
            out.push(Role { name: name.clone(), source, light, dark });
        }
        let find = |wanted: &str| {
            out.iter()
                .position(|r| r.name == wanted)
                .ok_or_else(|| IconError::Palette(format!("required role `{wanted}` is missing")))
        };
        let (line, gap, guide) = (find("line")?, find("gap")?, find("guide")?);
        Ok(Palette { roles: out, line, gap, guide })
    }

    pub(crate) fn line_role(&self) -> usize {
        self.line
    }
    pub(crate) fn gap_role(&self) -> usize {
        self.gap
    }
    pub(crate) fn guide_role(&self) -> usize {
        self.guide
    }

    pub(crate) fn name(&self, role: usize) -> Option<&str> {
        self.roles.get(role).map(|r| r.name.as_str())
    }

    /// The role whose stable source hex (`#rrggbb`, lowercase) is `hex_lower`. When several roles
    /// share a source the first in name order wins (as in SolidCraft).
    pub(crate) fn role_for_source(&self, hex_lower: &str) -> Option<usize> {
        let digits = hex_lower.strip_prefix('#').filter(|d| d.len() == 6)?;
        let rgb = [0, 2, 4].map(|i| digits.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()));
        let [Some(r), Some(g), Some(b)] = rgb else { return None };
        self.roles.iter().position(|role| role.source == [r, g, b])
    }

    pub(crate) fn rgb(&self, role: usize, theme: Theme) -> Option<[u8; 3]> {
        self.roles.get(role).map(|r| if theme == Theme::Dark { r.dark } else { r.light })
    }
}
