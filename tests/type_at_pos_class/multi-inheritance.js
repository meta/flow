// @flow

function create(Base: Class<A> | Class<B>) {
  return class extends Base {
    constructor() {
      super();
//    ^
    }
  };
}

class A {}
class B {}
create(A);
create(B);
