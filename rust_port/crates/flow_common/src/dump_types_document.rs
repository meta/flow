/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! The JSON document a type dump prints, shared by `dump-types --for-tool --json` and
//! `fox batch-dump-types` artifacts.

use serde_json::Map;
use serde_json::Value;

/// The modifiers that change the document's shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DocumentOptions {
    /// Each entry's `type` is the type's JSON itself, not that JSON in a string.
    pub json_direct: bool,
    /// Print the `schema` object even when no modifier needs it.
    pub force_schema: bool,
}

impl DocumentOptions {
    /// Every field at its value, defaults included. `None` keeps the shape older readers expect.
    fn schema(self) -> Option<Value> {
        (self.json_direct || self.force_schema)
            .then(|| serde_json::json!({ "json_direct": self.json_direct }))
    }

    /// An entry's `type`, from the JSON the checker printed for it.
    pub fn type_value(self, type_json: &str) -> Result<Value, serde_json::Error> {
        if self.json_direct {
            serde_json::from_str(type_json)
        } else {
            Ok(Value::String(type_json.to_owned()))
        }
    }

    /// The bare array of entries, or, with a schema or a `$defs` table, an object holding them.
    pub fn document(self, types: Value, defs: Option<Value>) -> Value {
        let schema = self.schema();
        if schema.is_none() && defs.is_none() {
            return types;
        }
        let mut document = Map::new();
        if let Some(schema) = schema {
            document.insert("schema".to_owned(), schema);
        }
        document.insert("types".to_owned(), types);
        if let Some(defs) = defs {
            document.insert("$defs".to_owned(), defs);
        }
        Value::Object(document)
    }
}
