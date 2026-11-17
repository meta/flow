// Consequences of giving template literals literal-like precision.

import * as React from 'react';

declare const s: string;
declare const n: number;

// Computed keys whose template types differ
function computed_keys() {
  const k1 = `First (${s})`;
  const k2 = `Second (${s})`;
  ({[k1]: 1, [k2]: 2}); // okay: disjoint templates merge into a string dict
  ({[`a_${n}`]: false, [`b_${n}`]: 'x'}); // error string ~> boolean (values still conflict)
  ({[`a_${n}`]: 1, [`a_${n}`]: 2}); // okay: same template shape
  ({[s]: 1, [n]: 2}); // error number ~> string (non-string-like keys stay strict)
}

// Disjoint template keys under a string indexer annotation
function computed_keys_annotated(index: number): {[key: string]: string} {
  return {
    [`token_name_${index}`]: '',
    [`ios_font_family_${index}`]: '',
    [`ios_font_weight_${index}`]: '',
  }; // okay
}

// A template key next to widened keys in an object with an indexer
function computed_keys_indexer() {
  const SHAPE = Object.freeze({LINK: 'link', NODE: 'node'});
  const o: {[key: string]: {...}} = {
    [SHAPE.LINK as string]: {},
    [`${SHAPE.LINK}:hover`]: {},
    [SHAPE.NODE as string]: {}, // TODO error indexer may overwrite explicit keys
  };
}

// Comparing a template to a literal it can't produce
function compare() {
  const key = `${n}:${n}`;
  if (key === '') {} // error
}

// Assigning templates to an annotated variable narrows it
function array_push() {
  let lines: Array<string> = [];
  lines = [`Processed: ${s}`, `Updated: ${s}`];
  lines.push(''); // TODO error '' ~> `Processed: ${string}` | `Updated: ${string}`
}

// Indexing with a template that the object's keys don't cover
function index_with_template() {
  const SIZES = {size48: 30, size64: 41};
  SIZES[`size${n}`]; // error
  declare const metrics: Partial<{views_7d: number, views_28d: number}>;
  declare const period: '7d' | '28d';
  metrics[`views_${period}`]; // okay
  metrics[`favorited_${period}`]; // error property missing
}

// A template resolves like a general-string intrinsic, since both `string`
// and string literals are accepted as components
function jsx_tag() {
  const Tag = `h${n}`;
  <Tag />; // okay
  ({}) as React.ComponentProps<typeof Tag>; // okay
}

// Folded union keys merge into a string dict instead of failing pairwise
function folded_union_keys(prefix: 'original' | 'updated'): {[key: string]: string} {
  return {
    [`${prefix}_hash`]: 'x',
    [`${prefix}_url`]: 'y',
  }; // okay
}

// Plain literal-union keys merge the same way
function literal_union_keys(k1: 'a' | 'b', k2: 'c' | 'd'): {[key: string]: string} {
  return {
    [k1]: 'x',
    [k2]: 'y',
  }; // okay
}

// ...but conflicting values still error
function union_keys_value_conflict(
  prefix: 'original' | 'updated',
): {[key: string]: string} {
  return {
    [`${prefix}_hash`]: 'x',
    [`${prefix}_url`]: 42, // error number ~> string
  };
}

// Relational comparison accepts templates wherever strings are accepted,
// including unions that do not normalize
function ordered_comparison(
  min: string,
  u: `${number}-01-01` | string,
  cond: boolean,
): void {
  let start = '';
  if (cond) {
    start = `${n}-01-01`;
  } else {
    start = min;
  }
  if (min < start) {} // okay
  if (s < u) {} // okay
  if (s < 5) {} // error string ~> number
}

// A template over a mixed general+literal union still matches literals
// producible through either side (cf. MetaCRMGuidanceHomeUtils)
function mixed_union_placeholder(x: number | 1, y: string | 'a') {
  if (`H${x}` !== 'H1') {} // okay: 'H1' matches via x = 1
  if (`H${x}` !== 'Hx') {} // error: neither number text nor '1' is 'x'
  if (`H${y}` !== 'Ha') {} // okay
}

// `===` against a literal refines a template when the literal matches
function literal_refinement(a: string, b: string) {
  const k = `${a}_${b}`;
  if (k === 'www_www') {
    (k as 'www_www'); // okay
  }
  if (k !== 'www_www' && k !== 'distillery_distillery') {
    return;
  }
  if (k === 'distillery_distillery') {} // okay
}
