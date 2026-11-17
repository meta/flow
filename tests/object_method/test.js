const id = require('./id');

export type ObjectType = {
  readonly m: () => void,
 ...};

function methodCaller(x: ObjectType) {
  x.m();
};

module.exports = (id(
  methodCaller
) as (obj: ObjectType) => void);
