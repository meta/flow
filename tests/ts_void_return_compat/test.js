type FlowVoidHandler = (event: string) => void;
declare function takeFlowVoidHandler(handler: FlowVoidHandler): void;

declare const returnsUnknown: (event: string) => unknown;
declare const returnsPromise: (event: string) => Promise<void>;

takeTsVoidHandler(returnsUnknown); // OK: a TypeScript `=> void` accepts any return type.
takeTsVoidHandler(returnsPromise); // OK
takeTsVoidHandler((event: string) => false); // OK
takeTsProps({onEvent: returnsUnknown}); // OK

takeTsVoidHandler((event: number) => {}); // ERROR: params are still checked.
takeTsNumberHandler(returnsUnknown); // ERROR: only `void` returns are relaxed.
takeFlowVoidHandler(returnsUnknown); // ERROR: a Flow `=> void` keeps Flow's rule.
