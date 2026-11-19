/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 * @format
 */

import type {NormalizeTag} from './assertionTypes';

// Substituted by the `paths` step for the test's sandbox project directory.
const PROJECT_ROOT_PLACEHOLDER = '<ROOT>';
const JSON_INDENT = 2;
// Wide enough to recognize the offending region, narrow enough that a multi-megabyte dump does not
// land in the test log.
const SNIPPET_RADIUS = 120;
const ELISION = '...';

// What a value actually passes through. `json` is sugar the caller writes; it expands to a parse
// where it appears plus a single stringify at the end, so the steps between it and the end operate
// on structured data rather than on text.
type Step = 'paths' | 'json_parse' | 'json_stringify';

function escapeRegExp(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function expandTags(tags: ReadonlyArray<NormalizeTag>): Array<Step> {
  const steps: Array<Step> = [];
  let parses = false;
  for (const tag of tags) {
    const step: Step = match (tag) {
      'paths' => 'paths',
      'json' => 'json_parse',
    };
    steps.push(step);
    parses ||= step === 'json_parse';
  }
  if (parses) {
    steps.push('json_stringify');
  }
  return steps;
}

// Anchor the match so a sibling path sharing `projectDir` as a prefix (e.g. `/tmp/sandbox` vs
// `/tmp/sandbox2`) is not rewritten. Separators after the root are rewritten to `/` so one golden
// serves every platform; parse JSON first (the `json` tag) and this never sees an escaped path.
function normalizePathsInString(value: string, projectDir: string): string {
  const rooted = value.replace(
    new RegExp(`${escapeRegExp(projectDir)}(?![A-Za-z0-9_.-])`, 'g'),
    PROJECT_ROOT_PLACEHOLDER,
  );
  return rooted.replace(
    new RegExp(`${PROJECT_ROOT_PLACEHOLDER}\\S*`, 'g'),
    match => match.replace(/\\/g, '/'),
  );
}

function normalizePaths(value: unknown, projectDir: string): unknown {
  if (typeof value === 'string') {
    return normalizePathsInString(value, projectDir);
  }
  if (Array.isArray(value)) {
    return value.map((item: unknown) => normalizePaths(item, projectDir));
  }
  if (typeof value === 'object' && value != null) {
    const out: {[key: string]: unknown} = {};
    for (const key of Object.keys(value)) {
      // Keys too: a manifest keyed by absolute path is otherwise unsnapshottable.
      out[normalizePathsInString(key, projectDir)] = normalizePaths(
        value[key],
        projectDir,
      );
    }
    return out;
  }
  return value;
}

// No engine exposes the offset as a property of the error, so its message is the only source, and
// a message without one ("Unexpected end of JSON input") is ordinary rather than exceptional.
function positionOfSyntaxError(error: SyntaxError): number | null {
  const found = /at position (\d+)/.exec(error.message);
  if (found == null) {
    return null;
  }
  const position = Number(found[1]);
  return Number.isInteger(position) ? position : null;
}

// `JSON.parse` reports what is wrong but never what it was reading, which is the half you need.
// The subject travels on the error rather than in its message so that the catch site decides how
// much of it to show.
class JsonParseError extends Error {
  value: string;
  position: number | null;

  constructor(cause: SyntaxError, value: string) {
    super(`normalize: 'json' could not parse the value: ${String(cause)}`);
    this.name = 'JsonParseError';
    this.value = value;
    this.position = positionOfSyntaxError(cause);
  }
}

// Control characters collapse to one space each rather than to escapes, so a column in the
// rendered line is still the offset it came from and the caret below it lands.
function toSingleLine(value: string): string {
  return value.replace(/[\n\r\t\v\f]/g, ' ');
}

function snippetAt(value: string, position: number): string {
  const start = Math.max(0, position - SNIPPET_RADIUS);
  const end = Math.min(value.length, position + SNIPPET_RADIUS);
  const opening = start > 0 ? ELISION : '';
  const closing = end < value.length ? ELISION : '';
  const line = opening + toSingleLine(value.slice(start, end)) + closing;
  const caret = ' '.repeat(opening.length + position - start) + '^';
  return `${line}\n${caret}`;
}

// Without a position to center on, the two ends are what separate a truncated payload from an
// empty or wrong-shaped one.
function bothEnds(value: string): string {
  if (value.length <= SNIPPET_RADIUS * 2) {
    return toSingleLine(value);
  }
  return (
    toSingleLine(value.slice(0, SNIPPET_RADIUS)) +
    ELISION +
    toSingleLine(value.slice(value.length - SNIPPET_RADIUS))
  );
}

function describeJsonParseFailure(error: JsonParseError): string {
  const {value, position} = error;
  const subject = `${value.length}-character input`;
  if (position == null) {
    return `${subject}, no position reported:\n${bothEnds(value)}`;
  }
  return `${subject}, failing at position ${position}:\n${snippetAt(value, position)}`;
}

function parseJson(value: unknown): unknown {
  if (typeof value !== 'string') {
    throw new Error("normalize: 'json' can only parse a string");
  }
  try {
    return JSON.parse(value);
  } catch (e) {
    if (e instanceof SyntaxError) {
      throw new JsonParseError(e, value);
    }
    throw e;
  }
}

function rootedPaths(value: unknown, projectDir: string): unknown {
  if (projectDir === '') {
    // A silent no-op here would let a golden be recorded still containing absolute sandbox paths —
    // the exact thing `paths` normalization exists to prevent. Fail loudly instead.
    throw new Error("normalize: 'paths' requested but no project dir is set");
  }
  return normalizePaths(value, projectDir);
}

function applyStep(step: Step, value: unknown, projectDir: string): unknown {
  return match (step) {
    'paths' => rootedPaths(value, projectDir),
    'json_parse' => parseJson(value),
    'json_stringify' => JSON.stringify(value, null, JSON_INDENT),
  };
}

// Apply the requested normalization as an ordered pipeline, and hand back text to snapshot against.
function normalizeOutput(
  tags: ReadonlyArray<NormalizeTag>,
  value: string,
  projectDir: string,
): string {
  let out: unknown = value;
  for (const step of expandTags(tags)) {
    out = applyStep(step, out, projectDir);
  }
  if (typeof out !== 'string') {
    throw new Error(
      "normalize: the pipeline produced a non-string; add 'json_stringify'",
    );
  }
  return out;
}

module.exports = {
  describeJsonParseFailure,
  JsonParseError,
  normalizeOutput,
  PROJECT_ROOT_PLACEHOLDER,
};
