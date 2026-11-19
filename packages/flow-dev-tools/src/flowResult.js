/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 * @format
 */

const {format} = require('util');

export type FlowResult = {
  passed: boolean,
  errors: Array<FlowError>,
  flowVersion: string,
  timing?: FlowTiming,
  memory?: FlowMemory,
  ...
};
export type FlowError = {
  kind: string,
  error_codes: Array<string>,
  level: string,
  message: Array<FlowMessage>,
  trace: ?Array<FlowMessage>,
  operation?: FlowMessage,
  extra?: FlowExtra,
  error_codes: Array<string>,
  ...
};
export type FlowMessage = {
  descr: string,
  type: 'Blame' | 'Comment',
  context?: ?string,
  loc?: ?FlowLoc,
  indent?: number,
  ...
};
type FlowExtra = Array<{
  message: Array<FlowMessage>,
  children: FlowExtra,
  ...
}>;
export type FlowLoc = {
  source: ?string,
  type: ?('LibFile' | 'SourceFile' | 'JsonFile' | 'Builtin'),
  start: FlowPos,
  end: FlowPos,
  ...
};
type FlowPos = {
  line: number,
  column: number,
  offset: number,
  ...
};
type FlowTimer = {
  start_age: number,
  duration: number,
  ...
};
type FlowTiming = {
  results: {
    [key: string]: {
      // Legacy fields
      start_wall_age: number,
      wall_duration: number,

      // New hotness
      wall: FlowTimer,
      user: FlowTimer,
      system: FlowTimer,
      worker_user: FlowTimer,
      worker_system: FlowTimer,
      ...
    },
  },
  ...
};
type FlowMemoryStat = {start: number, delta: number, hwm_delta: number};
type FlowMemoryGroup = {[stat: string]: FlowMemoryStat};
type FlowMemory = {
  sub_results: {[key: string]: FlowMemory},
  [group: string]: number | FlowMemoryGroup,
};

const noErrors: FlowResult = {
  passed: true,
  errors: [],
  flowVersion: 'No version',
};

// Returns a result that is a - b
function difference(a: FlowResult, b: FlowResult): FlowResult {
  const oldHashes: {[key: string]: FlowError} = {};
  const errors = [];
  for (let error of b.errors) {
    const hash = JSON.stringify(error.message);
    oldHashes[hash] = error;
  }
  for (let error of a.errors) {
    const hash = JSON.stringify(error.message);
    if (oldHashes[hash] !== undefined) {
      continue;
    }
    errors.push(JSON.parse(JSON.stringify(error)));
  }
  return {
    passed: errors.length === 0,
    errors,
    flowVersion: a.flowVersion,
  };
}

function prettyPrintWithHeader(result: FlowResult): string {
  if (result.passed) {
    return 'No errors';
  }

  return format(
    '%d error%s\n%s',
    result.errors.length,
    result.errors.length === 1 ? '' : 's',
    prettyPrint(result),
  );
}

function prettyPrint(result: FlowResult): string {
  // Copy the result so we can mess with it
  result = JSON.parse(JSON.stringify(result));
  return result.errors.map(prettyPrintError).join('\n\n');
}

function mainLocOfError(error: FlowError): ?FlowLoc {
  const {operation, message} = error;
  return (operation && operation.loc) || message[0].loc;
}

function mergedMessagesOfError(error: FlowError): Array<FlowMessage> {
  const {level, kind, message, operation, trace, extra} = error;
  let mainLoc = mainLocOfError(error);
  let messages = [].concat(
    getHeader(mainLoc, kind, level),
    getOpReason(operation),
    message,
    getExtraMessages(extra),
    getTraceReasons(trace),
  );
  const mainFile = (mainLoc && mainLoc.source) || '[No file]';
  // Merge comments into blames
  return messages.reduce((acc, message) => {
    const {descr, loc, type} = message;
    if (loc != null || acc.length == 0 || type == 'Blame') {
      acc.push(message);
    } else if (descr != 'Error:') {
      const prev = acc[acc.length - 1];
      prev.descr =
        prev.descr == '' ? descr : format('%s. %s', prev.descr, descr);
    }
    return acc;
  }, []);
}

function prettyPrintError(error: FlowError): string {
  let mainLoc = mainLocOfError(error);
  const mainFile = (mainLoc && mainLoc.source) || '[No file]';
  const messages = mergedMessagesOfError(error);
  return messages.map(prettyPrintMessage.bind(null, mainFile)).join('\n');
}

function mkComment(descr: string): FlowMessage {
  return {descr, type: 'Comment'};
}

function getHeader(
  mainLoc: ?FlowLoc,
  kind: string,
  level: string,
): Array<FlowMessage> {
  let line = -1;
  let filename = '[No file]';
  if (mainLoc != null) {
    const {source, start} = mainLoc;
    line = start.line;
    if (source != null) {
      filename = source;
    }
  }

  let prefix = '';
  if (kind == 'internal' && level == 'error') {
    prefix = 'Internal error (see logs):\n';
  } else if (mainLoc != null && mainLoc.type == 'LibFile') {
    if (kind == 'parse' && level == 'error') {
      prefix = 'Library parse error:\n';
    } else if (kind == 'infer') {
      prefix = 'Library type error:\n';
    }
  }

  return [mkComment(format('%s%s:%d', prefix, filename, line))];
}

function getOpReason(op: ?FlowMessage): Array<FlowMessage> {
  if (op) {
    return [op, mkComment('Error:')];
  }
  return [];
}

function getExtraMessages(extra: ?FlowExtra): Array<FlowMessage> {
  if (extra) {
    const messages = extra.reduce((acc: Array<FlowMessage>, current) => {
      const childrenMessages =
        current.children == null ? [] : getExtraMessages(current.children);
      const messages = acc.concat(current.message, childrenMessages);
      return messages;
    }, []);
    messages.forEach(message => (message.indent = (message.indent || 0) + 2));
    return messages;
  }
  return [];
}

function getTraceReasons(trace: ?Array<FlowMessage>): Array<FlowMessage> {
  if (trace != null && trace.length > 0) {
    return [{descr: 'Trace:', type: 'Blame'} as FlowMessage].concat(trace);
  }
  return [];
}

function prettyPrintMessage(
  mainFile: string,
  {context, descr, loc, indent}: FlowMessage,
): string {
  const indentation = Array((indent || 0) + 1).join(' ');
  if (loc != null) {
    let startCol = loc.start.column - 1;
    let contextStr = indentation;
    if (context != null) {
      // On Windows this might have \r
      context = context.trimRight();
      // Replace tabs with spaces
      context = context.replace(/\t/g, ' ');
      let lineStr = String(loc.start.line);
      if (lineStr.length < 3) {
        lineStr = ('   ' + lineStr).slice(-3);
      }
      lineStr += ': ';
      let padding = Array(lineStr.length + 1).join(' ');
      if (context.length > startCol) {
        padding += context.substr(0, startCol).replace(/[^\t ]/g, ' ');
      }
      const underline_size =
        loc.start.line == loc.end.line
          ? Math.max(1, loc.end.column - startCol)
          : 1;
      const underline = Array(underline_size + 1).join('^');

      contextStr = format(
        '%s%s%s\n%s%s%s ',
        indentation,
        lineStr,
        context,
        indentation,
        padding,
        underline,
      );
    }
    let see_another_file =
      loc.source == mainFile
        ? ''
        : format(
            '. See%s: %s:%d',
            loc.type === 'LibFile' ? ' lib' : '',
            loc.source,
            loc.start.line,
          );
    return format('%s%s%s', contextStr, descr, see_another_file);
  }
  return indentation + descr;
}

module.exports = {
  noErrors,
  difference,
  prettyPrintWithHeader,
  prettyPrint,
};
