//! Knot's editor state and policy that need no UI toolkit, scripting runtime,
//! or extension host. Functions take plain data and the models they change,
//! and return results as data; the application shell owns the runtime that
//! drives them.

pub mod editing;
pub mod history;
pub mod indentation;
pub mod keys;
pub mod style;
pub mod syntax;
pub mod theme;
