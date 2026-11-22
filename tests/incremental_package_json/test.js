import {x} from 'dep';
import {y} from 'pkg';

x as string; // ERROR while dep's main is a.js
y as string; // ERROR while pkg/package.json names the package `pkg`
