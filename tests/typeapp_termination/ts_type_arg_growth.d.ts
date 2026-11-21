export type M<T extends string> = {v: T} & {next: N<`a${T}`>};
export type N<T extends string> = {v: T} & {next: M<`a${T}`>};
