import {
  DEFAULT_MIME_TYPE_CONFIG,
  DEFAULT_MIME_TYPE_PRIORITY,
  conditionalFirst,
  conditionalLast,
  symbolLastKeys,
  symbolFirstKeys,
} from './reexport';

DEFAULT_MIME_TYPE_CONFIG['text/html'] as ?number; // OK
DEFAULT_MIME_TYPE_CONFIG['application/json'] as ?number; // OK
DEFAULT_MIME_TYPE_PRIORITY['text/plain'] as ?number; // OK
DEFAULT_MIME_TYPE_CONFIG['text/html'] as string; // ERROR

conditionalFirst.zz as ?number; // OK
conditionalLast.zz as ?number; // OK

// tsc: the property reads are errors (TS2339), the symbol reads are not.
declare const s: symbol;
symbolLastKeys[s] as number;
symbolLastKeys.zz as number;
symbolFirstKeys[s] as number;
symbolFirstKeys.zz as number;
