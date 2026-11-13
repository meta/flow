/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

mod cache;

use std::sync::Arc;

pub use cache::ReaderCache;
use dupe::Dupe;
use flow_aloc::PackedALocTable;
use flow_common::docblock::Docblock;
use flow_imports_exports::exports::Exports;
use flow_imports_exports::imports::Imports;
use flow_parser::ast::Program;
use flow_parser::file_key::FileKey;
use flow_parser::loc::DESERIALIZE_FILE_KEY;
use flow_parser::loc::Loc;
use flow_parser_utils::file_sig::FileSig;
use flow_type_sig::packed_type_sig::Module;
use flow_type_sig::signature_error::TolerableError;

fn compress(data: &[u8]) -> Vec<u8> {
    lz4_flex::compress_prepend_size(data)
}

fn decompress(data: &[u8]) -> Vec<u8> {
    lz4_flex::decompress_size_prepended(data).expect("failed to decompress")
}

fn set_file_key(file_key: &FileKey) {
    DESERIALIZE_FILE_KEY.with(|k| *k.borrow_mut() = Some(file_key.dupe()));
}

fn clear_file_key() {
    DESERIALIZE_FILE_KEY.with(|k| *k.borrow_mut() = None);
}

pub fn serialize_ast(ast: &Program<Loc, Loc>) -> Vec<u8> {
    let bytes = bincode::serde::encode_to_vec(ast, bincode::config::standard())
        .expect("failed to serialize AST");
    compress(&bytes)
}

pub fn deserialize_ast(file_key: &FileKey, bytes: &[u8]) -> Arc<Program<Loc, Loc>> {
    let decompressed = decompress(bytes);
    set_file_key(file_key);
    let (ast, _): (Program<Loc, Loc>, _) =
        bincode::serde::decode_from_slice(&decompressed, bincode::config::standard())
            .expect("failed to deserialize AST");
    clear_file_key();
    Arc::new(ast)
}

pub fn serialize_docblock(docblock: &Docblock) -> Vec<u8> {
    let bytes = bincode::serde::encode_to_vec(docblock, bincode::config::standard())
        .expect("failed to serialize Docblock");
    compress(&bytes)
}

pub fn deserialize_docblock(file_key: &FileKey, bytes: &[u8]) -> Arc<Docblock> {
    let decompressed = decompress(bytes);
    set_file_key(file_key);
    let (docblock, _): (Docblock, _) =
        bincode::serde::decode_from_slice(&decompressed, bincode::config::standard())
            .expect("failed to deserialize Docblock");
    clear_file_key();
    Arc::new(docblock)
}

pub fn serialize_aloc_table(table: &PackedALocTable) -> Vec<u8> {
    let bytes = bincode::serde::encode_to_vec(table, bincode::config::standard())
        .expect("failed to serialize PackedALocTable");
    compress(&bytes)
}

pub fn deserialize_aloc_table(bytes: &[u8]) -> Arc<PackedALocTable> {
    let decompressed = decompress(bytes);
    let (table, _): (PackedALocTable, _) =
        bincode::serde::decode_from_slice(&decompressed, bincode::config::standard())
            .expect("failed to deserialize PackedALocTable");
    Arc::new(table)
}

pub fn serialize_type_sig(module: &Module<Loc>) -> Vec<u8> {
    let bytes = bincode::serde::encode_to_vec(module, bincode::config::standard())
        .expect("failed to serialize type sig Module");
    compress(&bytes)
}

pub fn deserialize_type_sig(file_key: &FileKey, bytes: &[u8]) -> Arc<Module<Loc>> {
    let decompressed = decompress(bytes);
    set_file_key(file_key);
    let (module, _): (Module<Loc>, _) =
        bincode::serde::decode_from_slice(&decompressed, bincode::config::standard())
            .expect("failed to deserialize type sig Module");
    clear_file_key();
    Arc::new(module)
}

pub fn serialize_file_sig(file_sig: &FileSig) -> Vec<u8> {
    let bytes = bincode::serde::encode_to_vec(file_sig, bincode::config::standard())
        .expect("failed to serialize FileSig");
    compress(&bytes)
}

pub fn deserialize_file_sig(file_key: &FileKey, bytes: &[u8]) -> Arc<FileSig> {
    let decompressed = decompress(bytes);
    set_file_key(file_key);
    let (file_sig, _): (FileSig, _) =
        bincode::serde::decode_from_slice(&decompressed, bincode::config::standard())
            .expect("failed to deserialize FileSig");
    clear_file_key();
    Arc::new(file_sig)
}

pub fn serialize_exports(exports: &Exports) -> Vec<u8> {
    let bytes = bincode::serde::encode_to_vec(exports, bincode::config::standard())
        .expect("failed to serialize Exports");
    compress(&bytes)
}

pub fn deserialize_exports(bytes: &[u8]) -> Arc<Exports> {
    let decompressed = decompress(bytes);
    let (exports, _): (Exports, _) =
        bincode::serde::decode_from_slice(&decompressed, bincode::config::standard())
            .expect("failed to deserialize Exports");
    Arc::new(exports)
}

pub fn serialize_imports(imports: &Imports) -> Vec<u8> {
    let bytes = bincode::serde::encode_to_vec(imports, bincode::config::standard())
        .expect("failed to serialize Imports");
    compress(&bytes)
}

pub fn deserialize_imports(bytes: &[u8]) -> Arc<Imports> {
    let decompressed = decompress(bytes);
    let (imports, _): (Imports, _) =
        bincode::serde::decode_from_slice(&decompressed, bincode::config::standard())
            .expect("failed to deserialize Imports");
    Arc::new(imports)
}

pub fn serialize_file_sig_with_errors(
    file_sig: &FileSig,
    errors: &[TolerableError<Loc>],
) -> Vec<u8> {
    let bytes = bincode::serde::encode_to_vec((file_sig, errors), bincode::config::standard())
        .expect("failed to serialize FileSig with errors");
    compress(&bytes)
}

pub fn deserialize_file_sig_with_errors(
    file_key: &FileKey,
    bytes: &[u8],
) -> (Arc<FileSig>, Arc<[TolerableError<Loc>]>) {
    let decompressed = decompress(bytes);
    set_file_key(file_key);
    let ((file_sig, errors), _): ((FileSig, Vec<TolerableError<Loc>>), _) =
        bincode::serde::decode_from_slice(&decompressed, bincode::config::standard())
            .expect("failed to deserialize FileSig with errors");
    clear_file_key();
    (Arc::new(file_sig), Arc::from(errors))
}
