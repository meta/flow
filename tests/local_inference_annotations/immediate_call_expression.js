// @flow

const x: (n: number) => void = (function () {
    return (x) => {}; // no annotation needed
})();

const y: (n: number) => void = (function (y: number) {
    return (x) => {}; // error missing annotation
})(1);
