import * as React from 'react';
import * as IP from './include_provider';
import * as XP from './exclude_provider';

function FunctionComponent(): undefined {
    IP.useFoo(); // error
    IP.useBar(); // error: it's assumed to be hook
    XP.useFoo(); // error
    XP.useBar(); // ok
}

component Component() {
    IP.useFoo(); // ok
    IP.useBar(); // ok
    XP.useFoo(); // ok
    XP.useBar(); // error
    return null;
}

React.memo((): undefined => {
    IP.useFoo(); // ok: called in fn contextually typed as component
})
