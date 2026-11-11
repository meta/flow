/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use dupe::Dupe;
use flow_common::options::Options;
use flow_common::relay_options;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::file_key::FileKey;
use flow_parser::file_key::has_dts_ext;
use flow_parser::loc::Loc;

#[derive(Debug)]
pub struct TypeSigOptions {
    pub munge: bool,
    pub facebook_key_mirror: bool,
    pub facebook_fbt: Option<FlowSmolStr>,
    pub enable_custom_error: bool,
    pub enable_enums: bool,
    pub enable_component_syntax: bool,
    pub component_syntax_enabled_in_config: bool,
    pub enable_ts_syntax: bool,
    pub hook_compatibility: bool,
    pub enable_records: bool,
    pub enable_relay_integration: bool,
    pub relay_integration_module_prefix: Option<String>,
    pub for_builtins: bool,
    pub locs_to_dirtify: Vec<Loc>,
    pub is_ts_file: bool,
    // True for ambient TS files (.d.ts), where uninitialized enum members are
    // treated as computed rather than auto-numbered literals.
    pub is_dts_file: bool,
    pub tslib_syntax: bool,
    pub is_lib_file: bool,
}

impl TypeSigOptions {
    pub fn of_options(
        options: &Options,
        prevent_munge: bool,
        locs_to_dirtify: Vec<Loc>,
        file: &FileKey,
        is_lib_file: bool,
    ) -> Self {
        let munge = options.munge_underscores && !prevent_munge;
        let enable_component_syntax = options.component_syntax || is_lib_file;
        // NOTE: This is a Facebook-specific hack that makes the signature verifier and generator
        // recognize and process a custom `keyMirror` function that makes an enum out of the keys
        // of an object.
        let facebook_key_mirror = true;
        let enable_relay_integration = options.enable_relay_integration;
        let relay_integration_excludes = &options.relay_integration_excludes;
        let relay_integration_module_prefix = options.relay_integration_module_prefix.dupe();
        let relay_integration_module_prefix_includes =
            &options.relay_integration_module_prefix_includes;
        let enable_relay_integration = enable_relay_integration
            && relay_options::enabled_for_file(relay_integration_excludes, file);
        let relay_integration_module_prefix = relay_options::module_prefix_for_file(
            relay_integration_module_prefix_includes,
            file,
            relay_integration_module_prefix.as_deref(),
        );
        Self {
            munge,
            facebook_key_mirror,
            enable_relay_integration,
            relay_integration_module_prefix,
            locs_to_dirtify,
            hook_compatibility: options.hook_compatibility_in_file(file),
            facebook_fbt: options.facebook_fbt.dupe(),
            enable_custom_error: options.enable_custom_error,
            // TS enums (.ts/.d.ts) are a distinct feature from Flow Enums and are always
            // available, independent of the `enums` option which gates Flow Enums.
            enable_enums: options.enums || flow_common::files::has_ts_ext(file),
            enable_component_syntax,
            component_syntax_enabled_in_config: options.component_syntax,
            enable_ts_syntax: options.ts_syntax || flow_common::files::has_ts_ext(file),
            enable_records: options.enable_records,
            for_builtins: false,
            is_ts_file: flow_common::files::has_ts_ext(file),
            is_dts_file: has_dts_ext(file.as_str()),
            tslib_syntax: options.tslib_syntax || flow_common::files::has_ts_ext(file),
            is_lib_file,
        }
    }

    pub fn builtin_options(options: &Options) -> Self {
        // NOTE: This is a Facebook-specific hack that makes the signature verifier and generator
        // recognize and process a custom `keyMirror` function that makes an enum out of the keys
        // of an object.
        let facebook_key_mirror = true;
        Self {
            munge: false,
            facebook_key_mirror,
            enable_relay_integration: false,
            relay_integration_module_prefix: None,
            facebook_fbt: options.facebook_fbt.dupe(),
            enable_custom_error: options.enable_custom_error,
            enable_enums: options.enums,
            enable_component_syntax: true,
            component_syntax_enabled_in_config: options.component_syntax,
            enable_ts_syntax: false,
            hook_compatibility: options.hook_compatibility,
            enable_records: options.enable_records,
            for_builtins: true,
            locs_to_dirtify: Vec::new(),
            is_ts_file: false,
            is_dts_file: false,
            tslib_syntax: true,
            is_lib_file: true,
        }
    }
}
