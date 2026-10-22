//! The CLI's machine profile file (JSON). The format lives in `burnburn_presentation::profile_file`
//! so the hosts read the same file; this module only gives its errors the CLI's type.

pub use burnburn_presentation::profile_file::{Backend, ProfileFile};

use crate::CliError;

pub fn parse_profile(bytes: &[u8]) -> Result<ProfileFile, CliError> {
    burnburn_presentation::profile_file::parse_profile(bytes).map_err(|e| CliError::Profile(e.0))
}
