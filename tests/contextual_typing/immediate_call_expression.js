const x: (x: number) => void = (function () {
    return (x) => {
        x as number; // okay
        x as string; // error number ~> string
    };
})();
