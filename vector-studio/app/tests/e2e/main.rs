//! End-to-end tests: the whole app, driven through its window with pointer and keyboard
//! input. One module per feature area; see `TESTING.md` for the map from features to tests.

mod support;

mod drawing;
mod editing;
mod groups;
mod nodes;
mod style;
mod units;
mod arrange;
mod transform;
mod edit;
mod objects;
mod files;
mod pages;
mod view;
