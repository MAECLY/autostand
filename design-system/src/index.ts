// Package entry point: token metadata plus the component barrel. Relative paths
// only — the "@/" alias resolves differently in each package that consumes this.
export * from "./tokens";
export * from "../components";
