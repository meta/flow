import {useHelper} from './exports'; // ERROR

hook useHelper(): number {
  return 2;
}

hook useRead(): number {
  return useHelper();
}
