// @flow
//
// Cross-file: `.js` consumer of object types declared in
// `indexer_key_pattern_lib.ts`. Like TypeScript, an index signature only
// constrains the properties whose key it applies to. This is keyed on the
// target's declaration.

import type {
  ButtonProps,
  DataAttributes,
  LabeledData,
} from './indexer_key_pattern_lib';

declare const labeled: {label: string};

// `label` is not a `data-*` key, so the index signature doesn't apply to it.
labeled as DataAttributes; // OK
({label: 'x'}) as LabeledData; // OK
({label: 'x'}) as ButtonProps; // OK
({label: 'x', 'data-testid': 'a'}) as ButtonProps; // OK

// Properties whose key the index signature applies to are still checked.
({'data-testid': 1}) as DataAttributes; // ERROR: number vs string
({label: 'x', 'data-testid': 1}) as ButtonProps; // ERROR: number vs string

// Flow-declared targets keep checking every property against the indexer.
labeled as {[key: 'data-a' | 'data-b']: string, ...}; // ERROR: `label` vs indexer
