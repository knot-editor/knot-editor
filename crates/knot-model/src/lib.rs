//! Knot's editor state and policy that need no UI toolkit, scripting runtime,
//! or extension host. Functions take plain data and the models they change,
//! and return results as data; the application shell owns the runtime that
//! drives them, and its globals wrap this crate's plain state.
//!
//! - Text policy: [`history`], [`indentation`], [`editing`], and [`syntax`].
//! - Resources: [`resource`], [`filesystem`], [`open`], and [`workspace`].
//! - Input: [`keys`] and [`commands`].
//! - Presentation values: [`style`] and [`theme`].
//! - Packages: [`extensions`].

pub mod commands;
pub mod editing;
pub mod extensions;
pub mod filesystem;
pub mod history;
pub mod indentation;
pub mod keys;
pub mod open;
pub mod resource;
pub mod style;
pub mod syntax;
pub mod theme;
pub mod workspace;
