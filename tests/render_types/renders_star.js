import * as React from 'react';
component MenuItem() { return null }
null as renders* MenuItem; // OK
false as renders* MenuItem; // OK
false as false as renders* MenuItem; // OK
undefined as renders* MenuItem; // OK
[null, [false, [undefined]]] as renders* MenuItem; // OK
[null, false, undefined] as renders* MenuItem; // OK
true as renders* MenuItem; // ERROR

<MenuItem /> as renders* MenuItem; // OK
declare const menuItemChildrenArray: React.ChildrenArray<renders MenuItem>;
menuItemChildrenArray as renders* MenuItem; // OK

component BlueMenuItem() renders MenuItem { return <MenuItem />; }
<BlueMenuItem /> as renders* MenuItem; // OK

declare const rendersMaybeBlueMenuItem: renders? MenuItem;
rendersMaybeBlueMenuItem as renders* MenuItem; // OK

declare const rendersChildrenArrayBlueMenuItem: renders? React.ChildrenArray<BlueMenuItem>; // invalid-render
rendersChildrenArrayBlueMenuItem as renders* MenuItem; // OK

component Bad() { return null }
declare const rendersHuhBad: RendersHuh<Bad>;
rendersHuhBad as renders* MenuItem; // ERROR

declare const rendersStarMenuItem: renders* MenuItem;
rendersStarMenuItem as renders (null | false | undefined | MenuItem | ReadonlyArray<renders* MenuItem>); // type checks, but invalid-render

[<MenuItem />, <BlueMenuItem />, <Bad />] as renders* MenuItem; // ERROR

component MenuItems() renders* MenuItem {
  return null; // OK
}

component Menu(children: renders* MenuItem) { return null }
const menu1 = <Menu><MenuItems /></Menu>; // OK
const menu2 = <Menu><BlueMenuItem /><MenuItem /></Menu>; // OK
const menu3 = <Menu><MenuItems /><MenuItem /><BlueMenuItem /></ Menu>; // OK
