// The libraries of ts_mixed_builtin_strictness, merged with a Flow library last.
flowDogTsBox as TsBuiltinBox<FlowAnimal>; // OK: TypeScript generic with Flow arguments.
tsDogFlowBox as FlowBuiltinBox<TsAnimal>; // ERROR: Flow generic with TypeScript arguments.

// The key set follows the interface's own file, not this one: TypeScript `keyof`
// sees inherited members, Flow `$Keys` sees only own ones.
'custom' as typeof tsKeys; // OK: own member.
'mousedown' as typeof tsKeys; // OK: inherited through `extends`.
'custom' as typeof flowKeys; // OK: own member.
'mousedown' as typeof flowKeys; // ERROR: inherited members are not Flow keys.

// An inherited key still selects the keyed overload rather than the `string` fallback.
tsListener.on('mousedown', 1); // OK
tsListener.on('custom', 1); // ERROR: the keyed overload types `custom` as `string`.

// Statics and namespace objects follow the language of the library file that
// declares them, whatever the order of the library files.
tsFn as {f: number | string, ...}; // OK: TypeScript fields are covariant.
TsStatics as {s: number | string, ...}; // OK
TsNs as {v: number | string, ...}; // OK
tsIfaceClass as {name: string | number, ...}; // ERROR: a null prototype, but `name` is OK.
FlowStatics as {s: number | string, ...}; // ERROR: Flow fields are invariant.
FlowNs as {v: number | string, ...}; // ERROR: read-only in Flow.

// A declaration merged across both languages: each annotation keeps the language
// of the file that writes it, and the statics of overloads declared in both files
// take the joined language, TypeScript.
declare var mergedIface: MergedIface;
mergedIface.arr as Array<number | string>; // ERROR: a Flow array is invariant.
over as {f: number | string, ...}; // OK
