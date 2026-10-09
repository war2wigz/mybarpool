/**
 * The chain layer of `@mybarpool/shared`: a client object that takes a connection and a program
 * address, PDAs, reads, event decoding, subscriptions, the transaction helper, one helper per
 * user-facing instruction, and the derived views (ARCHITECTURE › Clients; DESIGN §10).
 */
export * from "./client.js";
export * from "./pdas.js";
export * from "./accounts.js";
export * from "./errors.js";
export * from "./events.js";
export * from "./watch.js";
export * from "./fees.js";
export * from "./transaction.js";
export * from "./funds.js";
export * from "./instructions/index.js";
