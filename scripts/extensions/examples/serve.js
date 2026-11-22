// API version 1 contract example: the provider's main module serves the
// contract. The host checks each argument and result against the schemas, so
// a result that breaks them rejects the caller's call.
import { windows } from "knot";
import { greeter } from "./contract.js";

let greeted = 0;

const registration = await greeter.serve({
  // `context.window` is the caller's window, or null outside one.
  async greet({ name, shout }, context) {
    greeted += 1;
    const text = `hello ${name}`;
    return {
      greeting: shout === true ? text.toUpperCase() : text,
      window: context.window !== null && (await windows.get(context.window)) !== null,
    };
  },
  reset() {
    greeted = 0;
  },
});

// A consumer in another package that lists this one in `requires` writes:
//
//   import { greeter } from "@example/greeter";
//   const client = await greeter.client(); // rejects ContractNotFound if unserved
//   const { greeting } = await client.greet({ name: "knot" });
//
// Disposing the registration, or unloading the package, uncovers an earlier
// provider of the contract, and held clients then reject ContractNotFound.
export async function stop() {
  await registration[Symbol.asyncDispose]();
  return greeted;
}
