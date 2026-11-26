/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

'use strict';

var flowDotJsGlobal =
  typeof globalThis !== 'undefined'
    ? globalThis
    : typeof self !== 'undefined'
      ? self
      : typeof window !== 'undefined'
        ? window
        : {};

var crypto =
  flowDotJsGlobal.crypto != null &&
  typeof flowDotJsGlobal.crypto.getRandomValues === 'function'
    ? flowDotJsGlobal.crypto
    : {
        getRandomValues(array) {
          if (typeof require === 'function') {
            const nodeCrypto = require('crypto');
            if (
              nodeCrypto.webcrypto != null &&
              typeof nodeCrypto.webcrypto.getRandomValues === 'function'
            ) {
              return nodeCrypto.webcrypto.getRandomValues(array);
            }
            if (typeof nodeCrypto.randomFillSync === 'function') {
              return nodeCrypto.randomFillSync(array);
            }
          }
          throw new Error('flow-dot-js wasm requires crypto.getRandomValues');
        },
      };

let flowDotJsWasmModule;
let flowDotJsAlloc;
let flowDotJsFree;
let flowDotJsStringFree;
let flowDotJsCall;
let flowDotJsReady;

function getFlowDotJsGlobal() {
  return flowDotJsGlobal;
}

function getFlowDotJsExports() {
  if (typeof module === 'object' && module.exports != null) {
    return module.exports;
  }
  const globalObject = getFlowDotJsGlobal();
  const flow = {};
  globalObject.flow = flow;
  return flow;
}

function encodeUtf8(value) {
  if (typeof TextEncoder !== 'undefined') {
    return new TextEncoder().encode(value);
  }
  const utf8 = unescape(encodeURIComponent(value));
  const bytes = new Uint8Array(utf8.length);
  for (let i = 0; i < utf8.length; i++) {
    bytes[i] = utf8.charCodeAt(i);
  }
  return bytes;
}

function findWasmExport(wasmModule, name) {
  const underscoredName = `_${name}`;
  if (wasmModule != null) {
    if (typeof wasmModule[underscoredName] === 'function') {
      return wasmModule[underscoredName];
    }
    if (typeof wasmModule[name] === 'function') {
      return wasmModule[name];
    }
  }
  if (typeof wasmExports === 'object' && wasmExports != null) {
    if (typeof wasmExports[underscoredName] === 'function') {
      return wasmExports[underscoredName];
    }
    if (typeof wasmExports[name] === 'function') {
      return wasmExports[name];
    }
  }
  return null;
}

function getWasmExport(wasmModule, name) {
  const wasmExport = findWasmExport(wasmModule, name);
  if (wasmExport != null) {
    return wasmExport;
  }
  throw new Error(`flow.js wasm export ${name} is unavailable`);
}

function updateFlowDotJsMemoryViews() {
  if (typeof updateMemoryViews === 'function') {
    updateMemoryViews();
  }
}

function prepareFlowDotJsFallbackStack() {
  // The non-modular Buck/Emscripten output leaves the default 64KB stack.
  if (
    typeof wasmMemory !== 'object' ||
    wasmMemory == null ||
    typeof stackRestore !== 'function'
  ) {
    return;
  }
  updateFlowDotJsMemoryViews();
  stackRestore(Math.floor((wasmMemory.buffer.byteLength - 16) / 16) * 16);
}

function installWasmModule(wasmModule) {
  flowDotJsWasmModule = wasmModule;
  flowDotJsAlloc = findWasmExport(wasmModule, 'flowDotJsAlloc');
  flowDotJsFree = findWasmExport(wasmModule, 'flowDotJsFree');
  flowDotJsStringFree = findWasmExport(wasmModule, 'flowDotJsStringFree');
  flowDotJsCall = findWasmExport(wasmModule, 'flowDotJsCall');
  if (
    flowDotJsAlloc == null ||
    flowDotJsFree == null ||
    flowDotJsStringFree == null ||
    flowDotJsCall == null
  ) {
    const flowDotJsDispatch = getWasmExport(wasmModule, 'main');
    flowDotJsAlloc = size => {
      prepareFlowDotJsFallbackStack();
      return flowDotJsDispatch(0, size, 0);
    };
    flowDotJsFree = (_ptr, _size) => {};
    flowDotJsStringFree = ptr => {
      prepareFlowDotJsFallbackStack();
      flowDotJsDispatch(2, ptr, 0);
    };
    flowDotJsCall = (ptr, len) => {
      prepareFlowDotJsFallbackStack();
      return flowDotJsDispatch(3, ptr, len);
    };
  }
  flowDotJsExports.flowVersion = callRust('flowVersion', {});
  return flowDotJsExports;
}

function loadWasm() {
  if (flowDotJsReady != null) {
    return flowDotJsReady;
  }
  if (typeof flow_dot_js_wasm === 'function') {
    const maybeWasmModule = flow_dot_js_wasm({
      quit(_status, toThrow) {
        throw toThrow;
      },
    });
    if (maybeWasmModule != null && typeof maybeWasmModule.then === 'function') {
      flowDotJsReady = maybeWasmModule.then(installWasmModule);
    } else {
      flowDotJsReady = Promise.resolve(installWasmModule(maybeWasmModule));
    }
  } else {
    flowDotJsReady = new Promise((resolve, reject) => {
      if (typeof Module !== 'object' || Module == null) {
        reject(new Error('flow.js wasm loaded without an Emscripten Module'));
        return;
      }
      let installed = false;
      const finish = () => {
        if (installed) {
          return;
        }
        installed = true;
        try {
          resolve(installWasmModule(Module));
        } catch (error) {
          reject(error);
        }
      };
      if (Module.calledRun) {
        finish();
      } else if (typeof addOnInit === 'function') {
        addOnInit(finish);
      } else {
        const onRuntimeInitialized = Module.onRuntimeInitialized;
        Module.onRuntimeInitialized = function () {
          if (typeof onRuntimeInitialized === 'function') {
            onRuntimeInitialized();
          }
          finish();
        };
      }
    });
  }
  return flowDotJsReady;
}

function initWasm() {
  if (flowDotJsWasmModule != null) {
    return;
  }
  loadWasm();
  if (flowDotJsWasmModule == null) {
    throw new Error('flow.js wasm is still loading; await flow.ready first.');
  }
}

function callRust(method, params) {
  initWasm();
  const request = encodeUtf8(JSON.stringify({method, params}));
  const requestPtr = flowDotJsAlloc(request.length);
  updateFlowDotJsMemoryViews();
  let responsePtr = 0;
  try {
    flowDotJsWasmModule.HEAPU8.set(request, requestPtr);
    responsePtr = flowDotJsCall(requestPtr, request.length);
    updateFlowDotJsMemoryViews();
    const response = JSON.parse(decodeHeapString(responsePtr));
    if (!response.ok) {
      throw new Error(response.error);
    }
    return response.value;
  } finally {
    if (responsePtr !== 0) {
      flowDotJsStringFree(responsePtr);
    }
    flowDotJsFree(requestPtr, request.length);
  }
}

function decodeHeapString(ptr) {
  const heap = flowDotJsWasmModule.HEAPU8;
  let end = ptr;
  while (heap[end] !== 0) {
    end++;
  }
  const bytes = heap.subarray(ptr, end);
  if (typeof TextDecoder !== 'undefined') {
    return new TextDecoder().decode(bytes);
  }
  let binary = '';
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i]);
  }
  return decodeURIComponent(escape(binary));
}

function normalizeLoc(loc) {
  return {
    source: loc.source,
    type: loc.type,
    start: {
      line: loc.start.line,
      column: loc.start.column,
    },
    end: {
      line: loc.end.line,
      column: loc.end.column,
    },
  };
}

function orderRange(range) {
  return {
    startLineNumber: range.startLineNumber,
    startColumn: range.startColumn,
    endLineNumber: range.endLineNumber,
    endColumn: range.endColumn,
  };
}

function orderCompletionItem(item) {
  const result = {
    additionalTextEdits: (item.additionalTextEdits ?? []).map(edit => ({
      text: edit.text,
      range: orderRange(edit.range),
    })),
  };
  if (item.insertText != null) {
    result.insertText = item.insertText;
  }
  if (item.range != null) {
    const insert = orderRange(item.range.insert);
    const replace = orderRange(item.range.replace);
    if (
      insert.endLineNumber === replace.endLineNumber &&
      insert.endColumn === replace.endColumn
    ) {
      insert.endColumn += 1;
    }
    result.range = {
      insert,
      replace,
    };
  }
  if (item.detail != null) {
    result.detail = item.detail;
  }
  if (item.documentation != null) {
    result.documentation = item.documentation;
  }
  if (item.kind != null) {
    result.kind = item.kind;
  }
  result.label = item.label;
  return result;
}

function orderTokenLoc(loc) {
  return {
    start: {
      line: loc.start.line,
      column: loc.start.column,
    },
    end: {
      line: loc.end.line,
      column: loc.end.column,
    },
  };
}

function orderToken(token) {
  const result = {
    type: token.type,
    context: token.context,
    loc: orderTokenLoc(token.loc),
    range: token.range,
  };
  if (token.value != null) {
    result.value = token.value;
  }
  return result;
}

const flowDotJsExports = getFlowDotJsExports();

flowDotJsExports.ready = loadWasm();

flowDotJsExports.configSchema = `[
  {
    "key": "babel_loose_array_spread",
    "kind": "option",
    "type": "bool",
    "default": false,
    "desc": "Only allow array spread with arrays, not arbitrary iterables."
  },
  {
    "key": "enums",
    "kind": "option",
    "type": "bool",
    "default": true,
    "desc": "Enable support for Flow Enums."
  },
  {
    "key": "experimental.const_params",
    "kind": "option",
    "type": "bool",
    "default": false,
    "desc": "Treat all function parameters as const bindings, banning reassignment."
  },
  {
    "key": "experimental.ts_syntax",
    "kind": "option",
    "type": "bool",
    "default": true,
    "desc": "Make Flow accept and translate some TS syntax automatically."
  },
  {
    "key": "react.runtime",
    "kind": "option",
    "type": "enum",
    "choices": ["classic", "automatic"],
    "default": "automatic",
    "desc": "Selecting 'automatic' enables auto-importing of React functions required for JSX."
  },
  {
    "key": "use_unknown_in_catch_variables",
    "kind": "option",
    "type": "bool",
    "default": false,
    "desc": "Changes the default type of 'catch' variables from 'any' to 'mixed'."
  },
  {
    "key": "dev_only.type_repr",
    "kind": "option",
    "type": "bool",
    "default": false,
    "desc": "Show the underlying type representation for debugging purposes."
  },
  {
    "key": "deprecated-type",
    "kind": "lint",
    "type": "bool",
    "default": false,
    "desc": "Error on the deprecated type annotations."
  },
  {
    "key": "sketchy-null",
    "kind": "lint",
    "type": "bool",
    "default": false,
    "desc": "Error on conditional checks that can be either 'null'/'undefined' or falsy."
  },
  {
    "key": "sketchy-number",
    "kind": "lint",
    "type": "bool",
    "default": false,
    "desc": "Error when a number appears in the left hand side of an '&&' expression."
  },
  {
    "key": "unclear-type",
    "kind": "lint",
    "type": "bool",
    "default": false,
    "desc": "Error on the unsafe 'any', 'Object', and 'Function' type annotations."
  },
  {
    "key": "unnecessary-assertion",
    "kind": "lint",
    "type": "bool",
    "default": false,
    "desc": "Error when a usage of 'invariant' or another assertion function is unnecessary."
  },
  {
    "key": "unnecessary-optional-chain",
    "kind": "lint",
    "type": "bool",
    "default": false,
    "desc": "Error when an optional chain '?.' is unnecessary."
  },
  {
    "key": "unsafe-getters-setters",
    "kind": "lint",
    "type": "bool",
    "default": false,
    "desc": "Error on usage of object getters and setters."
  },
  {
    "key": "unused-promise",
    "kind": "lint",
    "type": "bool",
    "default": false,
    "desc": "Error when a promise is unused."
  },
  {
    "key": "experimental.assert_operator",
    "kind": "option",
    "type": "bool",
    "default": false,
    "desc": "Allow use of the \`!\` operator to assert non-nullability."
  }
]`;

flowDotJsExports.registerFile = function registerFile(name, content) {
  return callRust('registerFile', {name, content});
};

flowDotJsExports.initBuiltins = function initBuiltins(filenames) {
  return callRust('initBuiltins', {filenames});
};

flowDotJsExports.check = function check(filename, config) {
  return callRust('check', {filename, config});
};

flowDotJsExports.checkContent = function checkContent(
  filename,
  content,
  config,
) {
  return callRust('checkContent', {filename, content, config});
};

flowDotJsExports.dumpTypes = function dumpTypes(filename, content, config) {
  return callRust('dumpTypes', {filename, content, config});
};

flowDotJsExports.dumpTypesForTool = function dumpTypesForTool(
  filename,
  content,
  config,
) {
  return callRust('dumpTypesForTool', {filename, content, config});
};

flowDotJsExports.jsOfOcamlVersion = 'rust-wasm';
flowDotJsExports.flowVersion = 'rust-wasm';

flowDotJsExports.parse = function parse(content, options = {}) {
  const result = callRust('parse', {content, options});
  if (Array.isArray(result.tokens)) {
    result.tokens = result.tokens.map(orderToken);
  }
  return result;
};

flowDotJsExports.autocomplete = function autocomplete(
  filename,
  content,
  line,
  col,
  config,
) {
  const result = callRust('autocomplete', {
    filename,
    content,
    line,
    col,
    config,
  });
  return {
    incomplete: result.incomplete,
    suggestions: result.suggestions.map(orderCompletionItem),
  };
};

flowDotJsExports.getDef = function getDef(
  filename,
  content,
  line,
  col,
  config,
) {
  return callRust('getDef', {filename, content, line, col, config}).map(
    normalizeLoc,
  );
};

flowDotJsExports.semanticDecorations = function semanticDecorations(
  filename,
  content,
  config,
) {
  return callRust('semanticDecorations', {filename, content, config});
};

flowDotJsExports.signatureHelp = function signatureHelp(
  filename,
  content,
  line,
  col,
  config,
) {
  return callRust('signatureHelp', {filename, content, line, col, config});
};

flowDotJsExports.typeAtPos = function typeAtPos(
  filename,
  content,
  line,
  col,
  config,
) {
  return callRust('typeAtPos', {filename, content, line, col, config});
};
