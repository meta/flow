// Comments at member, call, and computed-key boundaries stay attached once.
class Comments {
  @ /* root */ decorators /* dot */ . /* property */ member /* args */ ( /* argument */ value /* end */ ) /* key */ [0]() {} // OK
  @ /* grouped */ ( /* inner */ decorators[0] /* close */ ) /* field */ [1] = value; // OK
}
