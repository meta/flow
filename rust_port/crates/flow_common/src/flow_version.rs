/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

const RELEASE_VERSION: &str = "0.340.0";

#[cfg(fbcode_build)]
static VERSION: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    let build_date = build_date_yyyymmdd();
    let revision = build_info::BuildInfo::get_revision();
    let revision = if revision.is_empty() {
        "unknown"
    } else {
        revision
    };
    format!("{RELEASE_VERSION}-{build_date}_{revision}")
});

#[cfg(fbcode_build)]
fn build_date_yyyymmdd() -> String {
    let time_iso8601 = build_info::BuildInfo::get_time_iso8601();
    match (
        time_iso8601.get(0..4),
        time_iso8601.get(5..7),
        time_iso8601.get(8..10),
    ) {
        (Some(year), Some(month), Some(day)) => format!("{year}{month}{day}"),
        _ => "00000000".to_string(),
    }
}

#[cfg(fbcode_build)]
pub fn version() -> &'static str {
    VERSION.as_str()
}

#[cfg(not(fbcode_build))]
pub fn version() -> &'static str {
    RELEASE_VERSION
}
