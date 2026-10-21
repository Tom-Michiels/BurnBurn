//! Human-readable descriptions of GRBL `error:N` and `ALARM:N` codes.
//!
//! The wording paraphrases the public GRBL 1.1 code tables (written from memory, not checked
//! against a firmware build); an unlisted code is `None`, never guessed.

pub fn error_description(code: u8) -> Option<&'static str> {
    Some(match code {
        1 => "G-code words consist of a letter and a value; letter was not found",
        2 => "Numeric value format is not valid or missing an expected value",
        3 => "Grbl '$' system command was not recognized or supported",
        4 => "Negative value received for an expected positive value",
        5 => "Homing cycle is not enabled via settings",
        6 => "Minimum step pulse time must be greater than 3 microseconds",
        7 => "EEPROM read failed; default values restored",
        8 => "Grbl '$' command cannot be used unless Grbl is idle",
        9 => "G-code locked out during alarm or jog state",
        10 => "Soft limits cannot be enabled without homing also enabled",
        11 => "Max characters per line exceeded; line was not processed and executed",
        12 => "Setting value exceeds the maximum step rate supported",
        13 => "Safety door detected as opened and door state initiated",
        14 => "Build info or startup line exceeded EEPROM line length limit",
        15 => "Jog target exceeds machine travel; command ignored",
        16 => "Jog command with no '=' or contains prohibited g-code",
        17 => "Laser mode requires PWM output",
        20 => "Unsupported or invalid g-code command",
        21 => "More than one g-code command from the same modal group found in the block",
        22 => "Feed rate has not yet been set or is undefined",
        23 => "G-code command in block requires an integer value",
        24 => "Two g-code commands that both require the use of the XYZ axis words were detected",
        25 => "A g-code word was repeated in the block",
        26 => "A g-code command implicitly or explicitly requires XYZ axis words, but none were detected",
        27 => "N line number value is not within the valid range",
        28 => "A g-code command was sent, but is missing some required P or L value words",
        29 => "Grbl supports six work coordinate systems G54-G59; G59.1, G59.2 and G59.3 are not supported",
        30 => "The G53 g-code command requires either a G0 seek or G1 feed motion mode to be active",
        31 => "There are unused axis words in the block and G80 motion mode cancel is active",
        32 => "A G2 or G3 arc was commanded but there are no XYZ axis words in the selected plane",
        33 => "The motion command has an invalid target",
        34 => "Arc radius value is invalid",
        35 => "A G2 or G3 arc, traced with the offset definition, is missing the IJK offset word",
        36 => "There are unused, leftover g-code words that aren't used by any command in the block",
        37 => "The G43.1 dynamic tool length offset command cannot apply an offset to an axis other than configured",
        38 => "Tool number greater than max supported value",
        _ => return None,
    })
}

pub fn alarm_description(code: u8) -> Option<&'static str> {
    Some(match code {
        1 => "Hard limit triggered; position is likely lost",
        2 => "G-code motion target exceeds machine travel; position retained",
        3 => "Reset while in motion; position is likely lost",
        4 => "Probe fail; probe is not in the expected initial state",
        5 => "Probe fail; probe did not contact the workpiece",
        6 => "Homing fail; reset during active homing cycle",
        7 => "Homing fail; safety door was opened during homing",
        8 => "Homing fail; cycle failed to clear limit switch when pulling off",
        9 => "Homing fail; could not find limit switch within search distance",
        _ => return None,
    })
}
