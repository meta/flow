import {useState} from 'react';

function Component(): null {
  class Inner {
    field: unknown = useState(); // error: react-rule-hook-class
  }
  return null;
}
