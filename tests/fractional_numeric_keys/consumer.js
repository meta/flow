// Cross-file inferred signatures expose the same named properties and values.
import {exported, roundtrip} from './signatures';
import type {ExportedFractional, NumericValue, QuotedValue} from './signatures';
exported['0.5'] as string; // OK
exported[1.5] as number; // OK
exported['-1.5'] as boolean; // OK
exported[0.5] as number; // ERROR
exported[-1.5] as string; // ERROR
exported[2.5]; // ERROR

// Explicit imported annotations require all properties with the right values.
roundtrip({0.5: 'half', '1.5': 15}) as ExportedFractional; // OK
roundtrip({'0.5': 'half', [1.5]: 15}); // OK
roundtrip({0.5: 5, 1.5: 15}); // ERROR
roundtrip({1.5: 15}); // ERROR
'half' as NumericValue; // OK
'half' as QuotedValue; // OK
5 as NumericValue; // ERROR

// A shared explicit TypeScript library feeds JS consumers through signatures.
declare const ts: TSFractional;
ts[0.5] as string; // OK
ts['1.5'] as number; // OK
ts[-0.5] as boolean; // OK
ts[0.5] as number; // ERROR
ts['-0.5'] as string; // ERROR
ts[2.5]('value') as number; // OK
ts[2.5](1); // ERROR
