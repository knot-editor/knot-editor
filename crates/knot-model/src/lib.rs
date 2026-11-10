//! Knot's editor state and policy that need no UI toolkit, scripting runtime,
//! or extension host. Functions take plain data and the models they change,
//! and return results as data; the application shell owns the runtime that
//! drives them, and its globals wrap this crate's plain state.
//!
//! - Text: [`buffer`], [`editor`], [`history`], [`indentation`], [`editing`],
//!   and [`syntax`].
//! - Resources: [`resource`], [`filesystem`], [`open`], and [`workspace`].
//! - Input: [`keys`], [`commands`], and [`completion`].
//! - Presentation values: [`style`] and [`theme`].
//! - Packages: [`extensions`].
//! - Layout: [`workbench`].

pub mod buffer;
pub mod command_history;
pub mod commands;
pub mod completion;
pub mod documents;
pub mod editing;
pub mod editor;
pub mod extensions;
pub mod files;
pub mod filesystem;
pub mod history;
pub mod icons;
pub mod indentation;
pub mod keys;
pub mod languages;
pub mod observation;
pub mod open;
pub mod resource;
pub mod settings;
pub mod state_database;
pub mod style;
pub mod syntax;
pub mod theme;
pub mod themes;
pub mod workbench;
pub mod workspace;
