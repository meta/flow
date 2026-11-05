// A member of a union that is an alias of a union counts with its members: the hint picks the
// action whose `type` matches, inside the nested union.
type CreateCmd<T> = {type: 'create', table: keyof T, key: string};
type DeleteCmd<T> = {type: 'delete', table: keyof T};
type Cmd<T> = CreateCmd<T> | DeleteCmd<T>;
type ApplyAction<T> = {type: 'APPLY', payload: {commands: ReadonlyArray<Cmd<T>>, ...}};
type Tables = {readonly a: number, readonly b: string};
type ToolsAction = Readonly<{type: 'TOOLS', payload: {x: number}}>;
type TableAction = ApplyAction<Tables> | Readonly<{type: 'OTHER', payload: {y: string}}>;
type Action = ToolsAction | TableAction;

declare function apply<T>(commands: ReadonlyArray<Cmd<T>>): ApplyAction<T>;
declare function dispatch(a: Action): void;
declare const cmd: CreateCmd<Tables>;

dispatch(apply([cmd])); // ok: T = Tables
