// The sheet's Dependencies section (DPV-04, T-0463): Overview draws it under the description, in place of the old
// Stack list. Its words are Rust's (`src/api/dependencies.rs`).
import type { Screen } from "../../app/registry";
import { DependenciesSection } from "./Dependencies";
import "./dependencies.css";

export const dependencies: Screen = (app) => {
  app.sheetSections.register("dependencies", { Component: DependenciesSection });
};
