declare module 'c' {
    declare module.exports: number; // ok
    declare module.exports: string; // ok
    declare module.exports: boolean; // ok
}

declare module 'd' {
    declare export var a: number;
    declare module.exports: number; // error
}

declare module 'e' {
    declare var a: number;
    declare module.exports: number;
    export default a; // error
}

declare module 'f' {
    declare var a: number;
    export default a;
    declare module.exports: number; // error
}

declare module 'g' {
    declare var a: number;
    declare var b: string;
    declare export default typeof a;
    declare export default typeof b; // error
    export default a; // error
}
