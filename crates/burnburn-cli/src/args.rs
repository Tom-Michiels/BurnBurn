//! Minimal argument parsing: `<command> [positionals] [--flag value | --switch]`.

use std::collections::BTreeMap;

use crate::CliError;

/// Switches that take no value.
const SWITCHES: &[&str] = &["allow-machine-contact", "json"];

#[derive(Debug, Default)]
pub struct Args {
    pub command: String,
    pub positional: Vec<String>,
    flags: BTreeMap<String, String>,
    switches: Vec<String>,
}

impl Args {
    pub fn parse(argv: &[String]) -> Result<Args, CliError> {
        let mut it = argv.iter();
        let command = it.next().cloned().ok_or(CliError::Usage("missing command".into()))?;
        let mut a = Args { command, ..Args::default() };
        while let Some(arg) = it.next() {
            if let Some(name) = arg.strip_prefix("--") {
                if SWITCHES.contains(&name) {
                    a.switches.push(name.to_owned());
                    continue;
                }
                let value = it.next().ok_or_else(|| CliError::Usage(format!("--{name} needs a value")))?;
                if a.flags.insert(name.to_owned(), value.clone()).is_some() {
                    return Err(CliError::Usage(format!("--{name} given twice")));
                }
            } else {
                a.positional.push(arg.clone());
            }
        }
        Ok(a)
    }

    pub fn flag(&self, name: &str) -> Option<&str> {
        self.flags.get(name).map(String::as_str)
    }

    pub fn required(&self, name: &str) -> Result<&str, CliError> {
        self.flag(name).ok_or_else(|| CliError::Usage(format!("--{name} is required")))
    }

    pub fn switch(&self, name: &str) -> bool {
        self.switches.iter().any(|s| s == name)
    }

    pub fn one_positional(&self, what: &str) -> Result<&str, CliError> {
        match self.positional.as_slice() {
            [one] => Ok(one),
            _ => Err(CliError::Usage(format!("expected exactly one {what}"))),
        }
    }
}
