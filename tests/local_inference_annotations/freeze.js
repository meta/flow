// @flow

const noHint = Object.freeze({f: x => {}}); // error
const withHint: {readonly f: (x: number) => void} = Object.freeze({f: x => {}});
Object.freeze({f: x => {}}) as {readonly f: (x: number) => void};
