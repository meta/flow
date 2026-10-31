/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::sync::LazyLock;

use flow_common_xx as xx;

use crate::flowlib_contents;
use crate::tslib_contents;

pub static CONTENTS: LazyLock<Vec<(&str, &str)>> = LazyLock::new(|| {
    let mut contents = flowlib_contents::COMMON_CONTENTS.to_vec();
    contents.extend([
        ("csstype.d.ts", tslib_contents::CSSTYPE_D_TS),
        (
            "lib.dom.asynciterable.d.ts",
            tslib_contents::LIB_DOM_ASYNCITERABLE_D_TS,
        ),
        ("lib.dom.d.ts", tslib_contents::LIB_DOM_D_TS),
        (
            "lib.dom.iterable.d.ts",
            tslib_contents::LIB_DOM_ITERABLE_D_TS,
        ),
        ("react-dom.d.ts", flowlib_contents::REACT_DOM_D_TS.as_str()),
        ("react.global.d.ts", tslib_contents::REACT_GLOBAL_D_TS),
    ]);
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
