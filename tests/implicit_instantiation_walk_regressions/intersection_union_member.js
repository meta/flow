// An object meets a member `A & S` of a union, an object type intersected with a bare marker, that
// it fits before the bare marker `S` of another member, as the speculative match of the flow-based
// solve tries the members in order: `I` gets the type of the items of `data`.
type SectionBase<I> = {data: ReadonlyArray<I>, key?: string, ...};
type SectionData<I, S> = (Readonly<SectionBase<I>> & S) | S;
declare function firstItem<I, S>(sections: ReadonlyArray<SectionData<I, S>>): I;
declare const sections: ReadonlyArray<{title: string, data: Array<{id: string}>}>;

const item = firstItem(sections); // was an error
item.id as string; // ok
item.id as number; // error
