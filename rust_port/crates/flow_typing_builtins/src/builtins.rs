/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeSet;
use std::rc::Rc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_aloc::ALoc;
use flow_aloc::ALocFuzzyMap;
use flow_common::flow_import_specifier::Userland;
use flow_common::reason::Reason;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_lazy::ContextFamily;
use flow_typing_type::type_::ModuleType;
use flow_typing_type::type_::Type;

pub type LazyVal<CX> = Rc<flow_lazy::Lazy<CX, (ALoc, Type)>>;

pub type LazyModuleType<CX> = Rc<flow_lazy::Lazy<CX, ModuleType>>;

pub type LazyModule<CX> = Rc<flow_lazy::Lazy<CX, (Reason, LazyModuleType<CX>)>>;

/// Reads a builtin into the context that asked for it.
pub type ReadBuiltin<CX, B, T> =
    Rc<dyn for<'a> Fn(&mut <CX as ContextFamily>::Context<'a>, &B) -> T>;

pub struct Builtins<CX: ContextFamily> {
    original_global_values: FlowOrdMap<FlowSmolStr, LazyVal<CX>>,
    original_global_types: FlowOrdMap<FlowSmolStr, LazyVal<CX>>,
    original_global_modules: FlowOrdMap<FlowSmolStr, LazyModule<CX>>,
    /// UMD globals declared with `export as namespace`. They never shadow the globals above.
    original_global_umd_values: FlowOrdMap<FlowSmolStr, LazyVal<CX>>,
    read_value: ReadBuiltin<CX, LazyVal<CX>, (ALoc, Type)>,
    read_module: ReadBuiltin<CX, LazyModule<CX>, (Reason, ModuleType)>,
    /// The qualified names of the members of library namespaces, keyed by their definitions.
    pub namespace_members: Rc<ALocFuzzyMap<FlowSmolStr>>,
}

impl<CX: ContextFamily> std::fmt::Debug for Builtins<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Builtins")
            .field(
                "original_global_values",
                &format!("<{} entries>", self.original_global_values.len()),
            )
            .field(
                "original_global_types",
                &format!("<{} entries>", self.original_global_types.len()),
            )
            .field(
                "original_global_modules",
                &format!("<{} entries>", self.original_global_modules.len()),
            )
            .field(
                "original_global_umd_values",
                &format!("<{} entries>", self.original_global_umd_values.len()),
            )
            .finish()
    }
}

impl<CX: ContextFamily> Builtins<CX> {
    pub fn builtin_ordinary_name_set(&self) -> BTreeSet<FlowSmolStr> {
        let mut result = BTreeSet::new();
        for k in self.original_global_values.keys() {
            result.insert(k.dupe());
        }
        for k in self.original_global_types.keys() {
            result.insert(k.dupe());
        }
        result
    }

    pub fn builtin_modules_set(&self) -> BTreeSet<FlowSmolStr> {
        self.original_global_modules.keys().duped().collect()
    }

    /// Reads the global value `name` into `cx`. Each read forces and copies it again, so the
    /// context memoizes the result.
    pub fn read_value_opt(&self, cx: &mut CX::Context<'_>, name: &str) -> Option<(ALoc, Type)> {
        let lazy_val = self.original_global_values.get(name)?;
        Some((self.read_value)(cx, lazy_val))
    }

    /// Reads the global type `name` into `cx`, like [Builtins::read_value_opt].
    pub fn read_type_opt(&self, cx: &mut CX::Context<'_>, name: &str) -> Option<(ALoc, Type)> {
        let lazy_val = self.original_global_types.get(name)?;
        Some((self.read_value)(cx, lazy_val))
    }

    /// Reads the UMD global `name` into `cx`, like [Builtins::read_value_opt].
    pub fn read_umd_value_opt(&self, cx: &mut CX::Context<'_>, name: &str) -> Option<(ALoc, Type)> {
        let lazy_val = self.original_global_umd_values.get(name)?;
        Some((self.read_value)(cx, lazy_val))
    }

    /// Reads the global module `name` into `cx`, like [Builtins::read_value_opt].
    pub fn read_module_opt(
        &self,
        cx: &mut CX::Context<'_>,
        name: &Userland,
    ) -> Option<(Reason, ModuleType)> {
        let lazy_module = self.original_global_modules.get(name.as_str())?;
        Some((self.read_module)(cx, lazy_module))
    }

    /// Builtins read in the context that forces them.
    pub fn of_name_map(
        values: FlowOrdMap<FlowSmolStr, LazyVal<CX>>,
        types: FlowOrdMap<FlowSmolStr, LazyVal<CX>>,
        modules: FlowOrdMap<FlowSmolStr, LazyModule<CX>>,
    ) -> Self {
        Self::of_name_map_with_readers(
            Rc::new(|cx: &mut CX::Context<'_>, lazy_val: &LazyVal<CX>| {
                lazy_val.get_forced(cx).clone()
            }),
            Rc::new(|cx: &mut CX::Context<'_>, lazy_module: &LazyModule<CX>| {
                let (r, lazy_module_type) = lazy_module.get_forced(cx);
                (r.dupe(), lazy_module_type.get_forced(cx).dupe())
            }),
            values,
            types,
            modules,
            FlowOrdMap::new(),
            Rc::default(),
        )
    }

    /// Builtins whose lazy values belong to another context, which `read_value` and
    /// `read_module` force and copy into the context asking for them.
    pub fn of_name_map_with_readers(
        read_value: ReadBuiltin<CX, LazyVal<CX>, (ALoc, Type)>,
        read_module: ReadBuiltin<CX, LazyModule<CX>, (Reason, ModuleType)>,
        values: FlowOrdMap<FlowSmolStr, LazyVal<CX>>,
        types: FlowOrdMap<FlowSmolStr, LazyVal<CX>>,
        modules: FlowOrdMap<FlowSmolStr, LazyModule<CX>>,
        umd_values: FlowOrdMap<FlowSmolStr, LazyVal<CX>>,
        namespace_members: Rc<ALocFuzzyMap<FlowSmolStr>>,
    ) -> Self {
        Builtins {
            original_global_values: values,
            original_global_types: types,
            original_global_modules: modules,
            original_global_umd_values: umd_values,
            read_value,
            read_module,
            namespace_members,
        }
    }

    pub fn empty() -> Self {
        Self::of_name_map(FlowOrdMap::new(), FlowOrdMap::new(), FlowOrdMap::new())
    }
}

impl<CX: ContextFamily> Default for Builtins<CX> {
    fn default() -> Self {
        Self::empty()
    }
}
