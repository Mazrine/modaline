//! modaline's model half: sprites, cels, colour, palettes, file i/o and the editor
//! state machine.
//!
//! It has no GUI dependency on purpose, so another tool can read, composite or
//! write a `.vpx` without launching the editor.
//! The shell — `app`, `gui`, `input` — stays in the binary.

pub mod ase;
pub mod brush;
pub mod color;
pub mod editor;
pub mod i18n;
pub mod io;
pub mod layout;
pub mod ops;
pub mod palette;
pub mod pen;
pub mod properties;
pub mod script;
pub mod sprite;
pub mod status;
