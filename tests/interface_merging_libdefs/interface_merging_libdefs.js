// Basic property merging: both `a` and `b` should be visible
declare const merged_props: MergedProps;
merged_props.a as string; // ok
merged_props.b as number; // ok
merged_props.a as number; // error: string ~> number
merged_props.b as string; // error: number ~> string

// Method overload merging: both foo signatures should exist, bar from second interface
declare const merged_methods: MergedMethods;
merged_methods.foo("") as string; // ok
merged_methods.foo(0) as number; // ok
merged_methods.bar() as boolean; // ok

// Extends merging: both Base1 and Base2 should be extended
declare const merged_extends: MergedExtends;
merged_extends.a as string; // ok
merged_extends.b as number; // ok
merged_extends.x as string; // ok (from Base1)
merged_extends.y as number; // ok (from Base2)

// Three-way merge
declare const three_way: ThreeWay;
three_way.first as string; // ok
three_way.second as number; // ok
three_way.third as boolean; // ok
three_way.first as number; // error: string ~> number

// A partial declared in a later lib file without variance keeps the base's `out`.
declare const varianceMerged: VarianceMerged<string>;
varianceMerged as VarianceMerged<string | number>; // ok: covariant
varianceMerged.get() as string;
varianceMerged.get() as number; // error: string is not number
declare const varianceDefault: VarianceMerged;
varianceDefault.value as string | number;

declare const varianceUnchecked: VarianceUnchecked<string>;
varianceUnchecked as VarianceUnchecked<string | number>; // ok: covariant

// The overloads of a lib file merged later come first, and in one file, those of the later
// declaration.
declare const mergedAcrossFiles: MergedAcrossFiles;
mergedAcrossFiles.m('') as 'later file'; // ok
mergedAcrossFiles.n('') as 'earlier file, second'; // ok
'a,b'.split('magic') as Array<number>; // ok: before the overloads of the builtin String
