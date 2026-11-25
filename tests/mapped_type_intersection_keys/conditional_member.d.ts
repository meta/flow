// tsc accepts. Annotation inference cannot evaluate the conditional member in
// either order.
type C<T> = T extends string ? {} : never;
export type ConditionalFirst = {[K in C<'x'> & string]?: number};
export type ConditionalLast = {[K in string & C<'x'>]?: number};
export declare const conditionalFirst: ConditionalFirst;
export declare const conditionalLast: ConditionalLast;
