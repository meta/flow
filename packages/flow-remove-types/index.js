/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

var parse = require('flow-parser').parse;
var vlq = require('vlq');

/**
 * Given a string JavaScript source which contains Flow types, return a string
 * which has removed those types.
 *
 * Options:
 *
 *   - all: (default: false)
 *     If true, bypasses looking for an @flow pragma comment before parsing.
 *
 *   - pretty: (default: false)
 *     If true, removes types completely rather than replacing with spaces.
 *     This may require using source maps.
 *
 *   - ignoreUninitializedFields: (default: false)
 *     If true, removes uninitialized class fields (`foo;`, `foo: string;`)
 *     completely rather than only removing the type. THIS IS NOT SPEC
 *     COMPLIANT! Instead, use `declare foo: string;` for type-only fields.
 *
 *   - removeEmptyImports: (default: false)
 *     If true, removes empty import statements that remain after removing
 *     all type and typeof imports (e.g. `import {} from 'some-module'`).
 *
 * Returns an object with two methods:
 *
 *   - .toString()
 *     Returns the transformed source code.
 *
 *   - .generateMap()
 *     Returns a v3 source map.
 */
module.exports = function flowRemoveTypes(source, options) {
  // Options
  var all = Boolean(options && options.all);
  if (options && options.checkPragma) {
    throw new Error(
      'flow-remove-types: the "checkPragma" option has been replaced by "all".',
    );
  }

  // If there's no @flow or @noflow flag, then expect no annotation.
  var pragmaStart = source.indexOf('@' + 'flow');
  var pragmaSize = 5;
  if (pragmaStart === -1) {
    pragmaStart = source.indexOf('@' + 'noflow');
    pragmaSize = 7;
    if (pragmaStart === -1 && !all) {
      return resultPrinter(options, source);
    }
  }

  // This parse configuration is intended to be as permissive as possible.
  var ast = parse(source, {flow: all ? 'all' : 'detect', tokens: true});

  var removedNodes = [];

  var context = {
    ast: ast,
    source: source,
    removedNodes: removedNodes,
    pretty: Boolean(options && options.pretty),
    ignoreUninitializedFields: Boolean(
      options && options.ignoreUninitializedFields,
    ),
    removeEmptyImports: Boolean(options && options.removeEmptyImports),
  };

  // Remove the flow pragma.
  if (pragmaStart !== -1) {
    var comments = getComments(ast);
    var pragmaIdx = findTokenIndex(comments, pragmaStart);
    if (pragmaIdx >= 0 && pragmaIdx < comments.length) {
      var pragmaType = comments[pragmaIdx].type;
      if (pragmaType === 'Line' || pragmaType === 'Block') {
        removedNodes.push(getPragmaNode(context, pragmaStart, pragmaSize));
      }
    }
  }

  // Remove all flow type definitions.
  visit(ast, context, removeFlowVisitor);

  return resultPrinter(options, source, removedNodes);
};

function resultPrinter(options, source, removedNodes) {
  // Options
  var pretty = Boolean(options && options.pretty);

  return {
    toString: function () {
      if (!removedNodes || removedNodes.length === 0) {
        return source;
      }

      var result = '';
      var lastPos = 0;

      // Step through the removed nodes, building up the resulting string.
      for (var i = 0; i < removedNodes.length; i++) {
        var node = removedNodes[i];
        result += source.slice(lastPos, startOf(node));
        lastPos = endOf(node);
        if (typeof node.__spliceValue === 'string') {
          result += node.__spliceValue;
        }
        if (!pretty) {
          var toReplace = source.slice(startOf(node), endOf(node));
          if (!node.loc || node.loc.start.line === node.loc.end.line) {
            result += space(toReplace.length);
          } else {
            var toReplaceLines = toReplace.split(LINE_RX);
            result += space(toReplaceLines[0].length);
            for (var j = 1; j < toReplaceLines.length; j += 2) {
              result += toReplaceLines[j] + space(toReplaceLines[j + 1].length);
            }
          }
        }
      }

      return (result += source.slice(lastPos));
    },
    generateMap: function () {
      return {
        version: 3,
        sources: ['source.js'],
        names: [],
        mappings: pretty ? generateSourceMappings(removedNodes) : '',
      };
    },
  };
}

var LINE_RX = /(\r\n?|\n|\u2028|\u2029)/;

var THIS_PARAM_MARKER = '__THIS_PARAM_MARKER__';

// A collection of methods for each AST type names which contain Flow types to
// be removed.
var removeFlowVisitor = {
  DeclareClass: removeNode,
  DeclareFunction: removeNode,
  DeclareInterface: removeNode,
  DeclareModule: removeNode,
  DeclareNamespace: removeNode,
  DeclareTypeAlias: removeNode,
  DeclareVariable: removeNode,
  InterfaceDeclaration: removeNode,
  TypeAlias: removeNode,
  TypePredicate: removeNodeIfNotCommentType,
  InferredPredicate: removeInferredPredicateNode,
  OpaqueType: removeNode,
  DeclareOpaqueType: removeNode,
  DeclareExportDeclaration: removeNode,
  AbstractMethodDefinition: removeAbstractMethod,
  AbstractPropertyDefinition: removeNode,
  NamespaceExportDeclaration: removeNode,

  ClassDeclaration: removeClassTypes,
  ClassExpression: removeClassTypes,

  MethodDefinition: removeOverrideModifier,

  AsExpression: removeTypeCast,
  SatisfiesExpression: removeTypeCast,

  AsConstExpression: function (context, node, ast) {
    var idx = findTokenIndexAtEndOfNode(ast.tokens, node.expression);
    removeNode(context, ast.tokens[idx + 1]); // `as` token
    removeNode(context, ast.tokens[idx + 2]); // `const` token
  },

  Identifier: function (context, node, ast) {
    if (node[THIS_PARAM_MARKER] === true) {
      removeNode(context, node);
      removeTrailingCommaNode(context, node);
      return false;
    }
    if (node.optional) {
      // Find the optional token.
      var idx = findTokenIndexAtStartOfNode(ast.tokens, node);
      do {
        idx++;
      } while (getLabel(ast.tokens[idx]) !== '?');
      removeNode(context, ast.tokens[idx]);
    }
  },

  FunctionDeclaration: function (context, node) {
    if (node.params && node.params.length) {
      if (
        node.params[0].type === 'Identifier' &&
        node.params[0].name === 'this'
      ) {
        node.params[0][THIS_PARAM_MARKER] = true;
      }
    }
  },

  FunctionExpression: function (context, node) {
    if (node.params && node.params.length) {
      if (
        node.params[0].type === 'Identifier' &&
        node.params[0].name === 'this'
      ) {
        node.params[0][THIS_PARAM_MARKER] = true;
      }
    }
  },

  PropertyDefinition: function (context, node) {
    if (node.declare || (context.ignoreUninitializedFields && !node.value)) {
      return removeNode(context, node);
    }
    removeOverrideModifier(context, node);
    if (node.variance != null) {
      removeNode(context, node.variance);
    }
    if (node.optional) {
      var tokens = context.ast.tokens;
      var idx = findTokenIndexAtEndOfNode(tokens, node.key);
      do {
        idx++;
      } while (getLabel(tokens[idx]) !== '?');
      removeNode(context, tokens[idx]);
    }
  },

  ExportNamedDeclaration: function (context, node) {
    if (node.exportKind === 'type' || node.exportKind === 'typeof') {
      return removeNode(context, node);
    }
    if (
      node.specifiers.length > 0 &&
      node.specifiers.every(function (specifier) {
        return (
          specifier.exportKind === 'type' || specifier.exportKind === 'typeof'
        );
      })
    ) {
      return removeNode(context, node);
    }
  },

  ExportSpecifier: function (context, node) {
    if (node.exportKind === 'type' || node.exportKind === 'typeof') {
      removeNode(context, node);
      removeTrailingCommaNode(context, node);
      return false;
    }
  },

  ExportAllDeclaration: function (context, node) {
    if (node.exportKind === 'type') {
      return removeNode(context, node);
    }
  },

  ExportDefaultDeclaration: function (context, node) {
    if (node.exportKind === 'type') {
      return removeNode(context, node);
    }
  },

  ImportEqualsDeclaration: function (context, node) {
    if (node.importKind === 'type' || node.importKind === 'typeof') {
      return removeNode(context, node);
    }
  },

  ImportDeclaration: function (context, node) {
    if (node.importKind === 'type' || node.importKind === 'typeof') {
      return removeNode(context, node);
    }

    if (
      context.removeEmptyImports &&
      node.importKind === 'value' &&
      node.specifiers.length > 0
    ) {
      if (node.specifiers[0].type !== 'ImportDefaultSpecifier') {
        // Remove import declaration if all specifiers are type imports
        for (var i = 0; i < node.specifiers.length; i++) {
          if (
            node.specifiers[i].importKind !== 'type' &&
            node.specifiers[i].importKind !== 'typeof'
          ) {
            return;
          }
        }
        return removeNode(context, node);
      } else if (node.specifiers.length > 1) {
        // Remove non-default specifiers completely
        // if all non-default specifiers are type imports
        for (var i = 1; i < node.specifiers.length; i++) {
          if (
            node.specifiers[i].importKind !== 'type' &&
            node.specifiers[i].importKind !== 'typeof'
          ) {
            return;
          }
        }

        var defaultSpecifier = node.specifiers[0];
        var firstSpecifier = node.specifiers[1];
        var lastSpecifier = node.specifiers[node.specifiers.length - 1];

        var idxStart = findTokenIndexAtStartOfNode(
          context.ast.tokens,
          firstSpecifier,
        );
        var idxEnd = findTokenIndexAtEndOfNode(
          context.ast.tokens,
          lastSpecifier,
        );

        removeTrailingCommaNode(context, defaultSpecifier);
        // remove opening brace
        removeNode(context, context.ast.tokens[idxStart - 1]);
        // remove trailing brace
        if (getLabel(context.ast.tokens[idxEnd + 1]) === '}') {
          removeNode(context, context.ast.tokens[idxEnd + 1]);
        } else {
          // last specifier has a trailing comma, skip it
          removeNode(context, context.ast.tokens[idxEnd + 2]);
        }
      }
    }
  },

  ImportSpecifier: function (context, node) {
    if (node.importKind === 'type' || node.importKind === 'typeof') {
      var ast = context.ast;

      // Flow quirk: Remove importKind which is outside the node
      var idxStart = findTokenIndexAtStartOfNode(ast.tokens, node);
      var maybeImportKind = ast.tokens[idxStart - 1];
      var maybeImportKindLabel = getLabel(maybeImportKind);
      if (
        maybeImportKindLabel === 'type' ||
        maybeImportKindLabel === 'typeof'
      ) {
        removeNode(context, maybeImportKind);
      }

      removeNode(context, node);
      removeTrailingCommaNode(context, node);
      return false;
    }
  },

  ArrowFunctionExpression: function (context, node) {
    // Naively erasing a multi-line return type from an arrow function will
    // leave a newline between the parameter list and the arrow, which is not
    // valid JS. Detect this here and move the arrow up to the correct line.

    if (context.pretty) {
      // Pretty-printing solves this naturally. Good, because our arrow-fudging
      // below doesn't play nice with source maps... Which are only created when
      // using --pretty.
      return;
    }
    var returnType = node.returnType;
    if (returnType) {
      var ast = context.ast;
      var paramEndIdx = findTokenIndexAtStartOfNode(ast.tokens, returnType);
      do {
        paramEndIdx--;
      } while (isComment(ast.tokens[paramEndIdx]));

      var arrowIdx = findTokenIndexAtEndOfNode(ast.tokens, returnType);
      while (getLabel(ast.tokens[arrowIdx]) !== '=>') {
        arrowIdx++;
      }

      if (
        ast.tokens[paramEndIdx].loc.end.line <
        ast.tokens[arrowIdx].loc.start.line
      ) {
        // Insert an arrow immediately after the parameter list.
        removeNode(
          context,
          getSpliceNodeAtPos(
            context,
            endOf(ast.tokens[paramEndIdx]),
            ast.tokens[paramEndIdx].loc.end,
            ' =>',
          ),
        );

        // Delete the original arrow token.
        removeNode(context, ast.tokens[arrowIdx]);
      }
    }
  },

  TypeAnnotation: function (context, node, ast, parent) {
    if (parent.default) {
      return;
    }
    return removeNodeIfNotCommentType(context, node, ast, parent);
  },

  TypeParameterDeclaration: function (context, node, ast, parent) {
    if (
      parent.type !== 'ComponentTypeAnnotation' &&
      parent.type !== 'FunctionTypeAnnotation'
    ) {
      return removeNode(context, node);
    }
  },

  TypeParameterInstantiation: function (context, node, ast, parent) {
    // prevent interference with asExpression removal
    if (parent.type !== 'GenericTypeAnnotation') {
      removeNode(context, node);
    }
  },
};

function removeTypeCast(context, node, ast) {
  var typeIdx = findTokenIndexAtStartOfNode(ast.tokens, node.typeAnnotation);
  removeNode(context, ast.tokens[typeIdx - 1]);
  removeNode(context, node.typeAnnotation);
  node.typeAnnotation = null;
}

function removeAbstractMethod(context, node) {
  removeNode(context, node);
  var tokens = context.ast.tokens;
  var nextToken = tokens[findTokenIndexAtEndOfNode(tokens, node) + 1];
  if (nextToken && getLabel(nextToken) === ';') {
    removeNode(context, nextToken);
  }
  return false;
}

function removeClassTypes(context, node, ast) {
  if (node.abstract) {
    var idx = findTokenIndexAtStartOfNode(ast.tokens, node);
    removeNode(context, ast.tokens[idx]);
  }
  removeImplementedInterfaces(context, node, ast);
}

function removeOverrideModifier(context, node) {
  if (node.override) {
    var tokens = context.ast.tokens;
    var idx = findTokenIndexAtStartOfNode(tokens, node);
    while (getLabel(tokens[idx]) !== 'override') {
      idx++;
    }
    removeNode(context, tokens[idx]);
  }
}

// If this class declaration or expression implements interfaces, remove
// the associated tokens.
function removeImplementedInterfaces(context, node, ast) {
  if (node.implements && node.implements.length > 0) {
    var first = node.implements[0];
    var last = node.implements[node.implements.length - 1];
    var idx = findTokenIndexAtStartOfNode(ast.tokens, first);
    do {
      idx--;
    } while (ast.tokens[idx].value !== 'implements');

    var lastIdx = findTokenIndexAtStartOfNode(ast.tokens, last);
    do {
      if (!isComment(ast.tokens[idx])) {
        // NOTE: ast.tokens has no comments in Flow
        removeNode(context, ast.tokens[idx]);
      }
    } while (idx++ !== lastIdx);
  }
}

// Append node to the list of removed nodes, ensuring the order of the nodes
// in the list.
function removeNode(context, node) {
  var removedNodes = context.removedNodes;
  var length = removedNodes.length;
  var index = length;

  // Check for line's leading and trailing space to be removed.
  var spaceNode = context.pretty ? getLeadingSpaceNode(context, node) : null;
  var lineNode = context.pretty ? getTrailingLineNode(context, node) : null;

  while (index > 0 && endOf(removedNodes[index - 1]) > startOf(node)) {
    index--;
  }

  if (index === length) {
    if (spaceNode) {
      removedNodes.push(spaceNode);
    }
    removedNodes.push(node);
    if (lineNode) {
      removedNodes.push(lineNode);
    }
  } else {
    if (lineNode) {
      if (spaceNode) {
        removedNodes.splice(index, 0, spaceNode, node, lineNode);
      } else {
        removedNodes.splice(index, 0, node, lineNode);
      }
    } else if (spaceNode) {
      removedNodes.splice(index, 0, spaceNode, node);
    } else {
      removedNodes.splice(index, 0, node);
    }
  }

  return false;
}

function removeNodeIfNotCommentType(context, node) {
  var source = context.source;
  var start = startOf(node);
  if (source[start] === '/') {
    return false;
  }
  return removeNode(context, node);
}

function removeInferredPredicateNode(context, node) {
  var tokens = context.ast.tokens;
  var priorTokenIdx = findTokenIndexAtStartOfNode(tokens, node) - 1;
  var token = tokens[priorTokenIdx];
  if (token && token.type === 'Punctuator' && token.value === ':') {
    removeNode(context, token);
  }
  return removeNode(context, node);
}

function removeTrailingCommaNode(context, node) {
  var ast = context.ast;

  // Remove trailing comma (potentially the next node)
  var idx = findTokenIndexAtEndOfNode(ast.tokens, node) + 1;

  while (isComment(ast.tokens[idx])) {
    // NOTE: ast.tokens has no comments in Flow
    idx++;
  }
  if (getLabel(ast.tokens[idx]) === ',') {
    removeNode(context, ast.tokens[idx]);
  }
  return false;
}

function getPragmaNode(context, start, size) {
  var source = context.source;
  var line = 1;
  var column = 0;
  for (var position = 0; position < start; position++) {
    var char = source[position];
    if (char === '\n') {
      line++;
      column = 0;
    } else if (char === '\r') {
      if (source[position + 1] === '\n') {
        position++;
      }
      line++;
      column = 0;
    } else {
      column++;
    }
  }
  return createNode({
    start: start,
    end: start + size,
    loc: {
      start: {line: line, column: column},
      end: {line: line, column: column + size},
    },
  });
}

function getLeadingSpaceNode(context, node) {
  var source = context.source;
  var end = startOf(node);
  var start = end;
  while (source[start - 1] === ' ' || source[start - 1] === '\t') {
    start--;
  }
  if (start !== end) {
    return createNode({
      start: start,
      end: end,
      loc: {start: node.loc.start, end: node.loc.start},
    });
  }
}

function getTrailingLineNode(context, node) {
  var source = context.source;
  var start = endOf(node);
  var end = start;
  while (source[end] === ' ' || source[end] === '\t') {
    end++;
  }

  // Remove all space including the line break if this token was alone on the line.
  if (source[end] === '\n' || source[end] === '\r') {
    if (source[end] === '\r' && source[end + 1] === '\n') {
      end++;
    }
    end++;

    if (isLastNodeRemovedFromLine(context, node)) {
      return createNode({
        start: start,
        end: end,
        loc: {start: node.loc.end, end: node.loc.end},
      });
    }
  }
}

// Creates a zero-width "node" with a value to splice at that position.
// WARNING: This is only safe to use when source maps are off!
function getSpliceNodeAtPos(context, pos, loc, value) {
  return createNode({
    start: pos,
    end: pos,
    loc: {start: loc, end: loc},
    __spliceValue: value,
  });
}

// Returns true if node is the last to be removed from a line.
function isLastNodeRemovedFromLine(context, node) {
  var tokens = context.ast.tokens;
  var priorTokenIdx = findTokenIndexAtStartOfNode(tokens, node) - 1;
  var token = tokens[priorTokenIdx];
  var line = node.loc.end.line;

  // Find previous token that was not removed on the same line.
  while (
    priorTokenIdx >= 0 &&
    token.loc.end.line === line &&
    isRemovedToken(context, token)
  ) {
    token = tokens[--priorTokenIdx];
  }

  // If there's no prior token (start of file), or the prior token is on another
  // line, this line must be fully removed.
  return !token || token.loc.end.line !== line;
}

// Returns true if the provided token was previously marked as removed.
function isRemovedToken(context, token) {
  var removedNodes = context.removedNodes;
  var nodeIdx = removedNodes.length - 1;

  // Find the last removed node which could possibly contain this token.
  while (nodeIdx >= 0 && startOf(removedNodes[nodeIdx]) > startOf(token)) {
    nodeIdx--;
  }

  var node = removedNodes[nodeIdx];

  // This token couldn't be removed if not contained within the removed node.
  if (nodeIdx === -1 || endOf(node) < endOf(token)) {
    return false;
  }

  // Iterate through the tokens contained by the removed node to find a match.
  var tokens = context.ast.tokens;
  var tokenIdx = findTokenIndexAtStartOfNode(tokens, node);
  while (endOf(tokens[tokenIdx]) <= endOf(node)) {
    if (token === tokens[tokenIdx]) {
      return true;
    }
    tokenIdx++;
  }

  return false;
}

// Given the AST output from the parser, walk through in a depth-first order,
// calling methods on the given visitor, providing context as the first argument.
function visit(ast, context, visitor) {
  var stack;
  var parent;
  var keys = [];
  var index = -1;

  do {
    index++;
    if (stack && index === keys.length) {
      parent = stack.parent;
      keys = stack.keys;
      index = stack.index;
      stack = stack.prev;
    } else {
      var node = parent ? parent[keys[index]] : getProgram(ast);
      if (node && typeof node === 'object' && (node.type || node.length)) {
        if (node.type) {
          var visitFn = visitor[node.type];
          if (visitFn && visitFn(context, node, ast, parent) === false) {
            continue;
          }
        }
        stack = {parent: parent, keys: keys, index: index, prev: stack};
        parent = node;
        keys = Object.keys(node);
        index = -1;
      }
    }
  } while (stack);
}

// Given an array of sorted tokens, find the index of the token which contains
// the given offset. Uses binary search for O(log N) performance.
function findTokenIndex(tokens, offset, matchBiasParam) {
  // Token ranges often overlap at the edges e.g {start: 0, end: 100}, {start: 100, end: 300}. The
  // `matchBiasParam` param allows a user to specify what to do in this case, defaulting to matching
  // against the start of a token range.
  var matchBias = matchBiasParam === 'end' ? 'end' : 'start';
  var min = 0;
  var max = tokens.length - 1;
  var ptr;

  while (min <= max) {
    ptr = ((min + max) / 2) | 0;
    var token = tokens[ptr];
    var end = endOf(token);
    var start = startOf(token);
    if (end < offset || (matchBias === 'start' && end === offset)) {
      min = ptr + 1;
    } else if (start > offset || (matchBias === 'end' && start === offset)) {
      max = ptr - 1;
    } else {
      return ptr;
    }
  }

  return ptr;
}

function findTokenIndexAtStartOfNode(tokens, node) {
  return findTokenIndex(tokens, startOf(node), 'start');
}
function findTokenIndexAtEndOfNode(tokens, node) {
  return findTokenIndex(tokens, endOf(node), 'end');
}

// True if the provided token is a comment.
function isComment(token) {
  return token.type === 'Block' || token.type === 'Line';
}

// Produce a string full of space characters of a given size.
function space(size) {
  var sp = ' ';
  var result = '';

  for (;;) {
    if ((size & 1) === 1) {
      result += sp;
    }
    size >>>= 1;
    if (size === 0) {
      break;
    }
    sp += sp;
  }
  return result;
}

// Generate a source map when *removing* nodes rather than replacing them
// with spaces.
function generateSourceMappings(removedNodes) {
  var mappings = '';
  if (!removedNodes || removedNodes.length === '') {
    return mappings;
  }

  var end = {line: 1, column: 0};

  for (var i = 0; i < removedNodes.length; i++) {
    var start = removedNodes[i].loc.start;
    var lineDiff = start.line - end.line;
    var columnDiff = start.column - end.column;
    if (lineDiff) {
      for (var l = 0; l !== lineDiff; l++) {
        mappings += ';';
      }
      mappings += vlq.encode([start.column, 0, lineDiff, columnDiff]);
    } else if (columnDiff) {
      if (i) {
        mappings += ',';
      }
      mappings += vlq.encode([columnDiff, 0, lineDiff, columnDiff]);
    }

    end = removedNodes[i].loc.end;
    mappings += ',';
    mappings += vlq.encode([
      0,
      0,
      end.line - start.line,
      end.column - start.column,
    ]);
  }

  return mappings;
}

/**
 * A lightweight layer to abstract over the slightly different ASTs returned by
 * Flow vs Babylon.
 */

function startOf(token) {
  return token.range[0];
}

function endOf(token) {
  return token.range[1];
}

function getComments(ast) {
  return ast.comments;
}

function createNode(data) {
  return {
    range: [data.start, data.end],
    loc: data.loc,
    __spliceValue: data.__spliceValue,
  };
}

function getLabel(token) {
  return token.value;
}

function getProgram(ast) {
  return ast;
}
