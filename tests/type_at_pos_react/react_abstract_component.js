//@flow

function test(x: component(...any)) {
//            ^
  return;
}

declare component Foo();
type RendersFoo = renders Foo;
//            ^
