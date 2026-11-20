// Type references inside a `declare namespace` block see the classes and values declared in the
// namespace's other blocks.
declare const box: LibMergedValue.UsesClass;
box.v as string; // ok
box.v as number; // err
declare const value: LibMergedValue.UsesValue;
value as empty; // ok: `any`
