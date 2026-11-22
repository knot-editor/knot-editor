// The fake host's "knot/contract": the host's schema builder, whose contracts
// serve and bind through the fake host. Loaded in place of the host's module
// by `register.mjs`.
import { defineContract as define, s } from "../../../crates/knot-host/src/js/contract.js";
import { bindContract } from "./host.js";

export { s };

export const defineContract: typeof define = (name, methods) => {
  const contract = define(name, methods);
  return Object.freeze({ ...contract, ...bindContract(contract.name, Object.keys(contract.methods)) }) as never;
};
