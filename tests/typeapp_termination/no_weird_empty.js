import React from 'react';

type Comp<Props extends {...}> = component(ref: React.RefSetter<string>, ...Props);

function test1() {
  declare const Base: Comp<{}>;
  declare component C(ref: React.RefSetter<React.ElementRef<typeof Base>>);

  declare const ref: React.RefSetter<React.ElementRef<typeof C>> | undefined;
  <C ref={ref} />; // ok
}
