/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

declare type FlowLoc = {
  source: string,
  start: {line: number, column: number},
  end: {line: number, column: number},
};

declare type FlowJsErrorMessage = {
  loc: FlowLoc,
  context: string,
  type: string,
  descr: string,
};

declare type FlowJsErrorMessageInformation = {
  message: ReadonlyArray<FlowJsErrorMessage>,
  children: ReadonlyArray<FlowJsErrorMessageInformation>,
};

declare type FlowJsError = {
  level: string,
  message: ReadonlyArray<FlowJsErrorMessage>,
  extra: ReadonlyArray<FlowJsErrorMessageInformation>,
  error_codes?: ReadonlyArray<string>,
};

declare type FlowJsParseOptions = {
  esproposal_class_instance_fields: boolean,
  esproposal_class_static_fields: boolean,
  esproposal_decorators: boolean,
  esproposal_export_star_as: boolean,
  esproposal_optional_chaining: boolean,
  esproposal_nullish_coalescing: boolean,
  components?: boolean,
  enums?: boolean,
  types?: boolean,
  pattern_matching?: boolean,
  records?: boolean,
};

declare type FlowJsConfigSchema = Array<
  | {
      key: string,
      kind: 'option' | 'lint',
      type: 'enum',
      choices: Array<string>,
      default: string,
      desc?: string,
    }
  | {
      key: string,
      kind: 'option' | 'lint',
      type: 'bool',
      default: boolean,
      desc?: string,
    },
>;

declare type FlowJs = {
  flowVersion: string,
  ready?: Promise<unknown>,
  configSchema?: string,
  registerFile(filename: string, body: string): unknown,
  initBuiltins(filenames: ReadonlyArray<string>): unknown,
  checkContent(
    filename: string,
    body: string,
    options: {[key: string]: unknown},
  ): ReadonlyArray<FlowJsError>,
  autocomplete(
    filename: string,
    body: string,
    line: number,
    col: number,
    options: {[key: string]: unknown},
  ): any,
  getDef(
    filename: string,
    body: string,
    line: number,
    col: number,
    options: {[key: string]: unknown},
  ): ReadonlyArray<FlowLoc>,
  typeAtPos(
    filename: string,
    body: string,
    line: number,
    col: number,
    options: {[key: string]: unknown},
  ): null | string | Array<{type: 'flow' | 'markdown', value: string}>,
  semanticDecorations(
    filename: string,
    body: string,
    options: {[key: string]: unknown},
  ): {decorations: ReadonlyArray<{kind: 'refined-value', range: any}>},
  signatureHelp(
    filename: string,
    body: string,
    line: number,
    col: number,
    options: {[key: string]: unknown},
  ): any,
  parse(body: string, options: FlowJsParseOptions): interface {},
};
