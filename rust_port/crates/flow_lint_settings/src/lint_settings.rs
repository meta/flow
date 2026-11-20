/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::HashSet;

use dupe::Dupe;
use flow_parser::loc::Loc;

use crate::lints::LintKind;
use crate::severity::Severity;

#[derive(Debug, Clone)]
pub struct LintSettings<T: Copy> {
    // The default value associated with a lint if the lint kind isn't found in the map
    default_value: T,
    // Values for lints that have been explicitly set
    // The Loc is for settings defined in comments, and is used to find unused lint
    // suppressions. The location is set to None for settings coming from the flowconfig or --lints.
    explicit_values: BTreeMap<LintKind, (T, Option<Loc>)>,
}

const IGNORED_BY_ALL: &[LintKind] = &[
    LintKind::AmbiguousObjectType,
    LintKind::UninitializedInstanceProperty,
];

impl<T: Copy> LintSettings<T> {
    fn default_explicit_values() -> BTreeMap<LintKind, (Severity, Option<Loc>)> {
        let mut map = BTreeMap::new();
        map.insert(LintKind::UnsafeObjectAssign, (Severity::Err, None));
        map
    }

    fn config_default(kind: LintKind) -> (Severity, Option<Loc>) {
        Self::default_explicit_values()
            .get(&kind)
            .cloned()
            .unwrap_or((Severity::Off, None))
    }

    pub fn set_value(&mut self, key: LintKind, value: (T, Option<Loc>)) {
        self.explicit_values.insert(key, value);
    }

    pub fn set_all(&mut self, entries: Vec<(LintKind, (T, Option<Loc>))>) {
        for (key, value) in entries {
            self.set_value(key, value);
        }
    }

    pub fn get_default(&self) -> &T {
        &self.default_value
    }

    pub fn get_value(&self, lint_kind: LintKind) -> &T {
        self.explicit_values
            .get(&lint_kind)
            .map(|(v, _)| v)
            .unwrap_or(&self.default_value)
    }

    pub fn get_loc(&self, lint_kind: LintKind) -> Option<Loc> {
        self.explicit_values
            .get(&lint_kind)
            .and_then(|(_, loc)| loc.clone())
    }

    pub fn is_explicit(&self, lint_kind: LintKind) -> bool {
        self.explicit_values.contains_key(&lint_kind)
    }

    pub fn iter<F>(&self, mut f: F)
    where
        F: FnMut(&LintKind, &(T, Option<Loc>)),
    {
        self.explicit_values.iter().for_each(|(k, v)| f(k, v));
    }

    pub fn fold<F, Acc>(&self, mut f: F, init: Acc) -> Acc
    where
        F: FnMut(LintKind, &(T, Option<Loc>), Acc) -> Acc,
    {
        self.explicit_values
            .iter()
            .fold(init, |acc, (&kind, value)| f(kind, value, acc))
    }

    pub fn replace_with_mapped<F>(&mut self, f: F)
    where
        F: Fn(&(T, Option<Loc>)) -> (T, Option<Loc>),
    {
        self.explicit_values = self
            .explicit_values
            .iter()
            .map(|(&k, v)| (k, f(v)))
            .collect();
    }

    // SEVERITY-SPECIFIC FUNCTIONS

    pub fn empty_severities() -> LintSettings<Severity> {
        LintSettings {
            default_value: Severity::Off,
            explicit_values: BTreeMap::new(),
        }
    }

    pub fn default_severities() -> LintSettings<Severity> {
        LintSettings {
            default_value: Severity::Off,
            explicit_values: Self::default_explicit_values(),
        }
    }
}

enum ParseResult {
    AllSetting(LintSettings<Severity>),
    EntryList(Vec<LintKind>, (Severity, Option<Loc>)),
}

impl Default for LintSettings<Severity> {
    fn default() -> Self {
        Self::empty_severities()
    }
}

impl LintSettings<Severity> {
    pub fn of_default(default_value: Severity) -> LintSettings<Severity> {
        let mut explicit_values = BTreeMap::new();
        for kind in IGNORED_BY_ALL {
            explicit_values.insert(*kind, (Self::config_default(*kind).0, None));
        }
        LintSettings {
            default_value,
            explicit_values,
        }
    }

    pub fn is_enabled(&self, lint_kind: LintKind) -> bool {
        match self.get_value(lint_kind) {
            Severity::Err | Severity::Warn => true,
            Severity::Off => false,
        }
    }

    /// Takes a base LintSettings and a list of labeled lines and returns the corresponding
    /// severity LintSettings from applying the new lines on top of the base settings if
    /// successful. Otherwise, returns an error message along with the label of the
    /// line it failed on.
    pub fn of_lines(
        base_settings: LintSettings<Severity>,
        lint_lines: Vec<(u32, String)>,
    ) -> Result<(LintSettings<Severity>, Vec<Warning>), Error> {
        fn parse_value(line: u32, value: &str) -> Result<Severity, Error> {
            match Severity::severity_of_str(value) {
                Some(severity) => Ok(severity),
                None => Err(Error {
                    line,
                    message:
                        "Invalid setting encountered. Valid settings are error, warn, and off."
                            .to_string(),
                }),
            }
        }

        fn parse_line(loc: &Loc, lint_number: u32, line: &str) -> Result<ParseResult, Error> {
            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() != 2 {
                return Err(Error {
                    line: lint_number,
                    message:
                        "Malformed lint rule. Properly formed rules contain a single '=' character."
                            .to_string(),
                });
            }

            let left = parts[0].trim();
            let right = parts[1].trim();

            let value = parse_value(lint_number, right)?;

            if left == "all" {
                Ok(ParseResult::AllSetting(LintSettings::of_default(value)))
            } else {
                match LintKind::parse_from_str(left) {
                    Some(kinds) => Ok(ParseResult::EntryList(kinds, (value, Some(loc.clone())))),
                    None => Err(Error {
                        line: lint_number,
                        message: format!("Invalid lint rule \"{}\" encountered.", left),
                    }),
                }
            }
        }

        fn add_value(
            keys: Vec<LintKind>,
            value: (Severity, Option<Loc>),
            settings: &mut LintSettings<Severity>,
        ) -> Result<(), String> {
            let all_redundant = keys.iter().copied().all(|key| {
                let v = settings.get_value(key);
                v == &value.0 && v != &LintSettings::<Severity>::config_default(key).0
            });

            if all_redundant {
                Err(
                    "Redundant argument. This argument doesn't change any lint settings."
                        .to_string(),
                )
            } else {
                for key in keys {
                    settings.set_value(key, value.dupe());
                }
                Ok(())
            }
        }

        fn loc_of_line(line: u32) -> Loc {
            Loc::mk(None, line as i32, 0, line as i32 + 1, 0)
        }

        // Artificially locate the lines to detect unused lines
        let located_lines: Vec<(Loc, (u32, String))> = lint_lines
            .into_iter()
            .map(|(line_number, line)| (loc_of_line(line_number), (line_number, line)))
            .collect();

        let mut settings = base_settings;
        let mut warnings = Vec::new();
        let mut changed = false;
        for (loc, (lint_number, line)) in &located_lines {
            match parse_line(loc, *lint_number, line) {
                Ok(ParseResult::EntryList(keys, value)) => {
                    match add_value(keys, value, &mut settings) {
                        Ok(()) => {}
                        Err(msg) => warnings.push(Warning {
                            line: *lint_number,
                            message: msg,
                        }),
                    }
                    changed = true;
                }
                Ok(ParseResult::AllSetting(value)) => {
                    if changed {
                        return Err(Error {
                            line: *lint_number,
                            message: "\"all\" is only allowed as the first setting. Settings are order-sensitive."
                                .to_owned(),
                        });
                    }
                    settings = value;
                    changed = true;
                }
                Err(Error {
                    line: l,
                    message: m,
                }) => warnings.push(Warning {
                    line: l,
                    message: m,
                }),
            }
        }

        let mut used_locs = HashSet::new();
        settings.iter(|_kind, (_enabled, loc)| {
            if let Some(loc) = loc {
                used_locs.insert(loc.dupe());
            }
        });
        for Warning {
            line,
            message: _warning,
        } in &warnings
        {
            used_locs.insert(loc_of_line(*line));
        }
        let mut first_unused: Option<u32> = None;
        for (art_loc, (label, line)) in &located_lines {
            if first_unused.is_some() {
                break;
            }
            let trimmed = line.trim();
            if !used_locs.contains(art_loc) && !trimmed.starts_with("all") {
                first_unused = Some(*label);
            }
        }
        if let Some(label) = first_unused {
            let warning = Warning {
                line: label,
                message: "Redundant argument. The values set by this argument are completely overwritten."
                    .to_owned(),
            };
            warnings.insert(0, warning);
        }
        settings.replace_with_mapped(|(enabled, _loc)| (*enabled, None));

        Ok((settings, warnings))
    }

    pub fn to_debug_string(&self) -> String {
        let mut s = format!("all={}", self.get_default().as_str());
        self.iter(|kind, (severity, _)| {
            s.push_str(&format!(", {}={}", kind.as_str(), severity.as_str()));
        });
        s
    }
}

pub struct Warning {
    pub line: u32,
    pub message: String,
}

pub struct Error {
    pub line: u32,
    pub message: String,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub enum LintParseError {
    InvalidSetting,
    MalformedArgument,
    NakedComment,
    NonexistentRule,
    OverwrittenArgument,
    RedundantArgument,
}
