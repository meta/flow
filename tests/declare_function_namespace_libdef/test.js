mergedFunction(1) as number;
mergedFunction.member(1) as number;
mergedFunction.member('text') as string;
mergedFunction.member(1) as string; // ERROR

const result: mergedFunction.NumberResult = 1;
const badResult: mergedFunction.NumberResult = 'text'; // ERROR

const other: number = mergedFunction.other();
const badOther: string = mergedFunction.other(); // ERROR

declare const b: MergedClass.B;
b as number;
b as string; // ERROR
