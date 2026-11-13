// The version this build was made from.
//
// One number, one source: `package.json`. The build injects it (see
// `vite.config.ts`), so the version on screen, the version in the bundle and the
// version the updater compares against are the same one. A literal written into a
// message table would drift the moment a release was cut, and the drift would
// only show up as an update that never arrives.

declare const __APP_VERSION__: string;

/**
 * The version, or a marker that says the number is not known.
 *
 * The fallback is not a plausible version on purpose: a build that somehow lost
 * the injected value should say so rather than pretend to be `0.0.0`.
 */
export const APP_VERSION: string =
  typeof __APP_VERSION__ === "string" ? __APP_VERSION__ : "unknown";
