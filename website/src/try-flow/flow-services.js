/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

import {load as initFlowLocally} from './flow-loader';

const PARSE_OPTIONS: FlowJsParseOptions = {
  esproposal_class_instance_fields: true,
  esproposal_class_static_fields: true,
  esproposal_decorators: true,
  esproposal_export_star_as: true,
  esproposal_optional_chaining: true,
  esproposal_nullish_coalescing: true,
  enums: true,
  components: true,
  types: true,
  pattern_matching: true,
  records: true,
};

export default class FlowJsServices {
  _flow: FlowJs;
  schema: ?FlowJsConfigSchema;
  supportsParse: boolean;
  // Invariant: config is always legal under the current _flow.
  config: {[key: string]: unknown};

  static init(
    withBaseUrl: (v: string) => string,
    version: string,
  ): Promise<FlowJsServices> {
    return initFlowLocally(withBaseUrl, version).then(
      flow => new FlowJsServices(flow),
    );
  }

  constructor(flow: FlowJs) {
    this._flow = flow;
    this.schema = JSON.parse(flow.configSchema || 'null');
    this.supportsParse = this._flow.parse != null;
    this.config = this.schema
      ? Object.fromEntries(this.schema.map(item => [item.key, item.default]))
      : {};
  }

  // Returns a new service object so that FlowJsServices can be used in states.
  withUpdatedConfig(config: ?{[key: string]: unknown}): FlowJsServices {
    const schema = this.schema;
    // When the current version of Flow doesn't have schema, then config is meaningless.
    if (schema == null || config == null) {
      return this;
    }
    // Check every config item satisfies schema shape.
    let passedValidation = Object.entries(config).every(([key, value]) => {
      const item = schema.find(item => item.key === key);
      if (item == null) return false;
      // $FlowFixMe[incompatible-type] value is mixed
      if (item.type === 'enum' && item.choices.includes(value)) {
        return true;
      } else if (item.type === 'bool' && typeof value === 'boolean') {
        return true;
      } else {
        return false;
      }
    });
    // Check all schema items are present in the config
    passedValidation =
      passedValidation && schema.every(item => config[item.key] != null);
    if (passedValidation) {
      const newService = new FlowJsServices(this._flow);
      newService.config = config;
      return newService;
    } else {
      // Whenever config validation fails, we don't change.
      return this;
    }
  }

  checkContent(filename: string, body: string): ReadonlyArray<FlowJsError> {
    return this._flow.checkContent(filename, body, this.config);
  }

  autocomplete(filename: string, body: string, line: number, col: number): any {
    return this._flow.autocomplete(filename, body, line, col, this.config);
  }

  getDef(
    filename: string,
    body: string,
    line: number,
    col: number,
  ): ReadonlyArray<FlowLoc> {
    return this._flow.getDef(filename, body, line, col, this.config);
  }

  typeAtPos(
    filename: string,
    body: string,
    line: number,
    col: number,
  ): null | string | Array<{type: 'flow' | 'markdown', value: string}> {
    return this._flow.typeAtPos(filename, body, line, col, this.config);
  }

  semanticDecorations(
    filename: string,
    body: string,
  ): ?{decorations: ReadonlyArray<{kind: 'refined-value', range: any}>} {
    return this._flow.semanticDecorations?.(filename, body, this.config);
  }

  signatureHelp(
    filename: string,
    body: string,
    line: number,
    col: number,
  ): any {
    return this._flow.signatureHelp(filename, body, line, col, this.config);
  }

  parseAstToJsonString(body: string): interface {} {
    return this._flow.parse(body, PARSE_OPTIONS);
  }
}
