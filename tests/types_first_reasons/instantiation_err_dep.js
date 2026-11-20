import type {U} from './instantiation_err';
0 as U; // forces the lazy TypeAppT, whose error is reported where it is written
