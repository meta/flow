var vars = require("./cycle_B.js")

declare function p(x: number | null): Promise<void>;

function f(x: number | null) {
  p(x);
}

const resNull: void = f(null);
const resB: void = f(vars.b);

module.exports = {resA: resNull, resB: resB};
