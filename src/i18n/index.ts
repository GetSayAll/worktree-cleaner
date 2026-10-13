// Single swap point for the UI language.
//
// To add a locale, import its table here and rebind `messages` (and make that
// binding reactive if the app needs to switch languages at runtime). Nothing
// else in the codebase needs to change: every component and helper reads the
// user-visible copy through this one export.
import { en } from "./en";

export type Messages = typeof en;
export const messages: Messages = en;
