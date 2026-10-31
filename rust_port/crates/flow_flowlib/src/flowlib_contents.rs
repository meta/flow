/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::sync::LazyLock;

use flow_common_xx as xx;

use crate::tslib_contents;

macro_rules! flowlib_file {
    ($name:ident, $filename:literal) => {
        #[cfg(fbcode_build)]
        const $name: &str = include_str!(concat!("flowlib/", $filename));
        #[cfg(not(fbcode_build))]
        const $name: &str = include_str!(concat!("../../../../lib/", $filename));
    };
}

flowlib_file!(LIB_ES5_JS, "lib.es5.js");
flowlib_file!(MISC_JS, "misc.js");
flowlib_file!(REACT_JS, "react.js");
flowlib_file!(DOM_EXTRA_TO_BE_REMOVED_JS, "dom_extra_to_be_removed.js");

/// A library from `tslib/` with the Flow-specific changes in `tslib_patches/<name>.diff` applied.
macro_rules! tslib_patched_file {
    ($vis:vis $name:ident, $filename:literal) => {
        $vis static $name: LazyLock<String> = LazyLock::new(|| {
            apply_patch(
                tslib_contents::$name,
                include_str!(concat!("tslib_patches/", $filename, ".diff")),
            )
            .unwrap_or_else(|| {
                panic!(concat!(
                    "tslib_patches/",
                    $filename,
                    ".diff should apply to tslib/",
                    $filename,
                    ".d.ts"
                ))
            })
        });
    };
}

tslib_patched_file!(LIB_ES2015_COLLECTION_D_TS, "lib.es2015.collection");
tslib_patched_file!(LIB_ES2015_CORE_D_TS, "lib.es2015.core");
tslib_patched_file!(LIB_ES2015_ITERABLE_D_TS, "lib.es2015.iterable");
tslib_patched_file!(LIB_ES2015_PROMISE_D_TS, "lib.es2015.promise");
tslib_patched_file!(LIB_ES2016_ARRAY_INCLUDE_D_TS, "lib.es2016.array.include");
tslib_patched_file!(LIB_ES2019_ARRAY_D_TS, "lib.es2019.array");
tslib_patched_file!(LIB_ES2020_PROMISE_D_TS, "lib.es2020.promise");
tslib_patched_file!(LIB_ESNEXT_COLLECTION_D_TS, "lib.esnext.collection");
tslib_patched_file!(LIB_ESNEXT_ITERATOR_D_TS, "lib.esnext.iterator");
tslib_patched_file!(pub(super) REACT_DOM_D_TS, "react-dom");

/// The embedded flowlib file contents as (filename, contents) pairs.
///
/// Files taken from `tslib/`, verbatim or patched, keep a `.js` name so they are checked as Flow
/// libdefs: a `.d.ts` partial would make every interface it merges into TS-loose.
pub(super) static COMMON_CONTENTS: LazyLock<Vec<(&str, &str)>> = LazyLock::new(|| {
    vec![
        ("lib.decorators.js", tslib_contents::LIB_DECORATORS_D_TS),
        (
            "lib.decorators.legacy.js",
            tslib_contents::LIB_DECORATORS_LEGACY_D_TS,
        ),
        (
            "lib.es2015.collection.js",
            LIB_ES2015_COLLECTION_D_TS.as_str(),
        ),
        ("lib.es2015.core.js", LIB_ES2015_CORE_D_TS.as_str()),
        (
            "lib.es2015.generator.js",
            tslib_contents::LIB_ES2015_GENERATOR_D_TS,
        ),
        ("lib.es2015.iterable.js", LIB_ES2015_ITERABLE_D_TS.as_str()),
        ("lib.es2015.promise.js", LIB_ES2015_PROMISE_D_TS.as_str()),
        ("lib.es2015.proxy.js", tslib_contents::LIB_ES2015_PROXY_D_TS),
        (
            "lib.es2015.reflect.js",
            tslib_contents::LIB_ES2015_REFLECT_D_TS,
        ),
        (
            "lib.es2015.symbol.js",
            tslib_contents::LIB_ES2015_SYMBOL_D_TS,
        ),
        (
            "lib.es2015.symbol.wellknown.js",
            tslib_contents::LIB_ES2015_SYMBOL_WELLKNOWN_D_TS,
        ),
        (
            "lib.es2016.array.include.js",
            LIB_ES2016_ARRAY_INCLUDE_D_TS.as_str(),
        ),
        ("lib.es2016.intl.js", tslib_contents::LIB_ES2016_INTL_D_TS),
        (
            "lib.es2017.arraybuffer.js",
            tslib_contents::LIB_ES2017_ARRAYBUFFER_D_TS,
        ),
        ("lib.es2017.date.js", tslib_contents::LIB_ES2017_DATE_D_TS),
        ("lib.es2017.intl.js", tslib_contents::LIB_ES2017_INTL_D_TS),
        (
            "lib.es2017.object.js",
            tslib_contents::LIB_ES2017_OBJECT_D_TS,
        ),
        (
            "lib.es2017.sharedmemory.js",
            tslib_contents::LIB_ES2017_SHAREDMEMORY_D_TS,
        ),
        (
            "lib.es2017.string.js",
            tslib_contents::LIB_ES2017_STRING_D_TS,
        ),
        (
            "lib.es2017.typedarrays.js",
            tslib_contents::LIB_ES2017_TYPEDARRAYS_D_TS,
        ),
        (
            "lib.es2018.asyncgenerator.js",
            tslib_contents::LIB_ES2018_ASYNCGENERATOR_D_TS,
        ),
        (
            "lib.es2018.asynciterable.js",
            tslib_contents::LIB_ES2018_ASYNCITERABLE_D_TS,
        ),
        ("lib.es2018.intl.js", tslib_contents::LIB_ES2018_INTL_D_TS),
        (
            "lib.es2018.promise.js",
            tslib_contents::LIB_ES2018_PROMISE_D_TS,
        ),
        (
            "lib.es2018.regexp.js",
            tslib_contents::LIB_ES2018_REGEXP_D_TS,
        ),
        ("lib.es2019.array.js", LIB_ES2019_ARRAY_D_TS.as_str()),
        ("lib.es2019.intl.js", tslib_contents::LIB_ES2019_INTL_D_TS),
        (
            "lib.es2019.object.js",
            tslib_contents::LIB_ES2019_OBJECT_D_TS,
        ),
        (
            "lib.es2019.string.js",
            tslib_contents::LIB_ES2019_STRING_D_TS,
        ),
        (
            "lib.es2019.symbol.js",
            tslib_contents::LIB_ES2019_SYMBOL_D_TS,
        ),
        (
            "lib.es2020.bigint.js",
            tslib_contents::LIB_ES2020_BIGINT_D_TS,
        ),
        ("lib.es2020.date.js", tslib_contents::LIB_ES2020_DATE_D_TS),
        ("lib.es2020.intl.js", tslib_contents::LIB_ES2020_INTL_D_TS),
        (
            "lib.es2020.number.js",
            tslib_contents::LIB_ES2020_NUMBER_D_TS,
        ),
        ("lib.es2020.promise.js", LIB_ES2020_PROMISE_D_TS.as_str()),
        (
            "lib.es2020.sharedmemory.js",
            tslib_contents::LIB_ES2020_SHAREDMEMORY_D_TS,
        ),
        (
            "lib.es2020.string.js",
            tslib_contents::LIB_ES2020_STRING_D_TS,
        ),
        (
            "lib.es2020.symbol.wellknown.js",
            tslib_contents::LIB_ES2020_SYMBOL_WELLKNOWN_D_TS,
        ),
        ("lib.es2021.intl.js", tslib_contents::LIB_ES2021_INTL_D_TS),
        (
            "lib.es2021.promise.js",
            tslib_contents::LIB_ES2021_PROMISE_D_TS,
        ),
        (
            "lib.es2021.string.js",
            tslib_contents::LIB_ES2021_STRING_D_TS,
        ),
        (
            "lib.es2021.weakref.js",
            tslib_contents::LIB_ES2021_WEAKREF_D_TS,
        ),
        ("lib.es2022.array.js", tslib_contents::LIB_ES2022_ARRAY_D_TS),
        ("lib.es2022.error.js", tslib_contents::LIB_ES2022_ERROR_D_TS),
        ("lib.es2022.intl.js", tslib_contents::LIB_ES2022_INTL_D_TS),
        (
            "lib.es2022.object.js",
            tslib_contents::LIB_ES2022_OBJECT_D_TS,
        ),
        (
            "lib.es2022.regexp.js",
            tslib_contents::LIB_ES2022_REGEXP_D_TS,
        ),
        (
            "lib.es2022.string.js",
            tslib_contents::LIB_ES2022_STRING_D_TS,
        ),
        ("lib.es2023.array.js", tslib_contents::LIB_ES2023_ARRAY_D_TS),
        (
            "lib.es2023.collection.js",
            tslib_contents::LIB_ES2023_COLLECTION_D_TS,
        ),
        ("lib.es2023.intl.js", tslib_contents::LIB_ES2023_INTL_D_TS),
        (
            "lib.es2024.arraybuffer.js",
            tslib_contents::LIB_ES2024_ARRAYBUFFER_JS,
        ),
        (
            "lib.es2024.collection.js",
            tslib_contents::LIB_ES2024_COLLECTION_D_TS,
        ),
        (
            "lib.es2024.object.js",
            tslib_contents::LIB_ES2024_OBJECT_D_TS,
        ),
        (
            "lib.es2024.promise.js",
            tslib_contents::LIB_ES2024_PROMISE_D_TS,
        ),
        (
            "lib.es2024.regexp.js",
            tslib_contents::LIB_ES2024_REGEXP_D_TS,
        ),
        (
            "lib.es2024.sharedmemory.js",
            tslib_contents::LIB_ES2024_SHAREDMEMORY_JS,
        ),
        (
            "lib.es2024.string.js",
            tslib_contents::LIB_ES2024_STRING_D_TS,
        ),
        ("lib.es5.js", LIB_ES5_JS),
        ("lib.esnext.array.js", tslib_contents::LIB_ESNEXT_ARRAY_D_TS),
        (
            "lib.esnext.collection.js",
            LIB_ESNEXT_COLLECTION_D_TS.as_str(),
        ),
        (
            "lib.esnext.decorators.js",
            tslib_contents::LIB_ESNEXT_DECORATORS_D_TS,
        ),
        (
            "lib.esnext.disposable.js",
            tslib_contents::LIB_ESNEXT_DISPOSABLE_D_TS,
        ),
        ("lib.esnext.error.js", tslib_contents::LIB_ESNEXT_ERROR_D_TS),
        (
            "lib.esnext.float16.js",
            tslib_contents::LIB_ESNEXT_FLOAT16_D_TS,
        ),
        ("lib.esnext.intl.js", tslib_contents::LIB_ESNEXT_INTL_D_TS),
        ("lib.esnext.iterator.js", LIB_ESNEXT_ITERATOR_D_TS.as_str()),
        (
            "lib.esnext.promise.js",
            tslib_contents::LIB_ESNEXT_PROMISE_D_TS,
        ),
        (
            "lib.esnext.sharedmemory.js",
            tslib_contents::LIB_ESNEXT_SHAREDMEMORY_D_TS,
        ),
        ("misc.js", MISC_JS),
        ("react.js", REACT_JS),
    ]
});

pub static CONTENTS: LazyLock<Vec<(&str, &str)>> = LazyLock::new(|| {
    let mut contents = COMMON_CONTENTS.to_vec();
    contents.push(("dom_extra_to_be_removed.js", DOM_EXTRA_TO_BE_REMOVED_JS));
    contents
});

pub static HASH: LazyLock<String> = LazyLock::new(|| {
    let mut state = xx::State::new(0);
    for (file, contents) in CONTENTS.iter() {
        state.update(file.as_bytes());
        state.update(contents.as_bytes());
    }
    format!("{:016x}", state.digest())
});

/// Applies a unified diff to the text it was made from. Every hunk must match exactly at the lines
/// it names, so a diff made against another version of the text fails instead of applying loosely.
fn apply_patch(original: &str, diff: &str) -> Option<String> {
    let mut patched = String::with_capacity(original.len());
    let original: Vec<&str> = original.split_inclusive('\n').collect();
    let mut next = 0;
    let mut lines = diff
        .split_inclusive('\n')
        .skip_while(|line| !line.starts_with("@@"))
        .peekable();
    while let Some(header) = lines.next() {
        let (old_start, old_len) = parse_hunk_header(header)?;
        let start = if old_len == 0 {
            old_start
        } else {
            old_start.checked_sub(1)?
        };
        patched.extend(original.get(next..start)?.iter().copied());
        next = start;
        while let Some(line) = lines.next_if(|line| !line.starts_with("@@")) {
            match line.split_at_checked(1)? {
                (" ", text) => {
                    if *original.get(next)? != text {
                        return None;
                    }
                    patched.push_str(text);
                    next += 1;
                }
                ("-", text) => {
                    if *original.get(next)? != text {
                        return None;
                    }
                    next += 1;
                }
                ("+", text) => patched.push_str(text),
                _ => return None,
            }
        }
    }
    patched.extend(original.get(next..)?.iter().copied());
    Some(patched)
}

/// Parses the old-file range of a hunk header, `@@ -start[,len] +start[,len] @@`.
fn parse_hunk_header(header: &str) -> Option<(usize, usize)> {
    let old_range = header.strip_prefix("@@ -")?.split(' ').next()?;
    let (start, len) = old_range.split_once(',').unwrap_or((old_range, "1"));
    Some((start.parse().ok()?, len.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tslib_patches_apply() {
        for (patched, original) in [
            (
                &*LIB_ES2015_COLLECTION_D_TS,
                tslib_contents::LIB_ES2015_COLLECTION_D_TS,
            ),
            (&*LIB_ES2015_CORE_D_TS, tslib_contents::LIB_ES2015_CORE_D_TS),
            (
                &*LIB_ES2015_ITERABLE_D_TS,
                tslib_contents::LIB_ES2015_ITERABLE_D_TS,
            ),
            (
                &*LIB_ES2015_PROMISE_D_TS,
                tslib_contents::LIB_ES2015_PROMISE_D_TS,
            ),
            (
                &*LIB_ES2016_ARRAY_INCLUDE_D_TS,
                tslib_contents::LIB_ES2016_ARRAY_INCLUDE_D_TS,
            ),
            (
                &*LIB_ES2019_ARRAY_D_TS,
                tslib_contents::LIB_ES2019_ARRAY_D_TS,
            ),
            (
                &*LIB_ES2020_PROMISE_D_TS,
                tslib_contents::LIB_ES2020_PROMISE_D_TS,
            ),
            (
                &*LIB_ESNEXT_COLLECTION_D_TS,
                tslib_contents::LIB_ESNEXT_COLLECTION_D_TS,
            ),
            (
                &*LIB_ESNEXT_ITERATOR_D_TS,
                tslib_contents::LIB_ESNEXT_ITERATOR_D_TS,
            ),
            (&*REACT_DOM_D_TS, tslib_contents::REACT_DOM_D_TS),
        ] {
            assert_ne!(patched, original, "a patch should change its library");
        }
    }

    #[test]
    fn apply_patch_rejects_mismatched_context() {
        let diff = "--- a\n+++ b\n@@ -1,2 +1,2 @@\n x\n-y\n+z\n";
        assert_eq!(apply_patch("x\ny\n", diff).as_deref(), Some("x\nz\n"));
        assert_eq!(
            apply_patch("w\ny\n", diff),
            None,
            "context `x` does not match"
        );
        assert_eq!(
            apply_patch("x\nq\n", diff),
            None,
            "removed `y` does not match"
        );
    }
}
