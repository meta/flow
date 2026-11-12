type TsVoidHandler = (event: string) => void;
type TsUndefinedHandler = (event: string) => undefined;

declare function takeTsVoidHandler(handler: TsVoidHandler): void;
declare function takeTsUndefinedHandler(handler: TsUndefinedHandler): void;
declare function tsReturnsVoid(): void;
