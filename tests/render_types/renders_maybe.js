//@flow
import * as React from 'react';

component MenuItem() {
  return null;
}
component BlueMenuItem() renders MenuItem {
  return <MenuItem />;
}
component MaybeMenuItem() renders (renders? MenuItem) { // type checks, but invalid-render
  return null;
}
component Bar() {
  return true;
}
component Baz() renders null { // type checks, but invalid-render
  return null;
}

(<MenuItem />) as renders? MenuItem; // OK
(<BlueMenuItem />) as renders? MenuItem; // OK
(<MaybeMenuItem />) as renders? MenuItem; // OK
(<Bar />) as renders? MenuItem; // ERROR
(<Baz />) as renders? MenuItem; // OK

declare const AllMenuItems: renders? (MenuItem | BlueMenuItem | MaybeMenuItem);
AllMenuItems as renders? MenuItem; // OK

declare const Bad: renders? (MenuItem | BlueMenuItem | MaybeMenuItem | Bar);
Bad as renders? MenuItem; // ERROR

declare const rendersMenuItem: renders MenuItem;
rendersMenuItem as renders (true | renders? MenuItem); // type checks, but invalid-render

declare const rendersMaybeMenuItem: renders? MenuItem;
rendersMaybeMenuItem as renders MenuItem; // ERROR

declare const rendersMaybeUnion: renders? (MenuItem | Bar);
rendersMaybeUnion as renders MenuItem; // ERROR

component RendersMaybeMenuItem() renders MaybeMenuItem {
  return <MaybeMenuItem />;
}
(<RendersMaybeMenuItem />) as renders? MenuItem; // OK

rendersMaybeMenuItem as renders (null | false | undefined | MenuItem); // invalid-render turns RHS any

rendersMaybeMenuItem as renders (null | false | undefined | Bar); // invalid-render turns RHS any

component MightRenderMenuItem() renders? MenuItem {
  return Math.random() > 0.5 ? null : <MenuItem />; // OK
}

(<Bar />) as (renders MenuItem) | (renders BlueMenuItem) | null | undefined; // ERROR, `null` and `undefined` not tried
(<Bar />) as ?(renders (MenuItem | BlueMenuItem)); // ERROR
