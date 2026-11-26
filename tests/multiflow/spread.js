function fun(x: 'hi', y: 123) {}
fun(...['hi' as const, 123 as const]); // No error
fun(...['hi' as const], ...[123 as const]); // No error
fun(...['hi' as const], ...[], ...[123 as const]); // No error
fun(...['hi' as const], ...[], ...[123 as const], ...[true as const]); // Error - true is unused
fun(...['hi' as const], ...[true as const], ...[123 as const]); // Error: true ~> 123 and 123 is unused

declare const arrOf123: Array<123>;
fun('hi', ...arrOf123); // No error - ignore the fact arrOf123 could be empty


function funWithRestArray(x: 'hi', y: 123, ...rest: Array<number>) {}
funWithRestArray(...['hi' as const, 123 as const]); // No error
funWithRestArray(...['hi' as const], ...[123 as const]); // No error
funWithRestArray(...['hi' as const], ...[], ...[123 as const]); // No error
funWithRestArray(...['hi' as const], ...[], ...[123 as const], ...[456, 789]); // No error
funWithRestArray(...['hi' as const], ...[true as const], ...[123 as const]); // Error: true ~> 123

funWithRestArray('hi', 123, ...arrOf123); // Ok
funWithRestArray('hi', ...arrOf123); // No error - ignore the fact arrOf123 could be empty
funWithRestArray('hi', ...arrOf123, ...arrOf123); // No error - ignore the fact arrOf123 could be empty

// 2 errors
// 1. 'bye' ~> 123 in case the first spread is empty
// 2. 'bye' ~> number in case the first spread is not empty
funWithRestArray('hi', ...arrOf123, 'bye', ...arrOf123);

function funWithReadonlyObjectInRestSpread(...args: [{readonly bar: string, ... }]): void {}
funWithReadonlyObjectInRestSpread({ bar: "" });

// An error about an argument from a spread whose type is written in another file is at the call.
import type {NumberTuple} from './spread_tuple_type';
declare const tupleOfNumber: NumberTuple;
declare function takesString(s: string): void;
takesString(...tupleOfNumber); // Error: number ~> string
