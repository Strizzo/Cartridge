//! Native controller-first browser. The full web engine runs locally in a child
//! process; page pixels and input cross a private, inherited CDP pipe.
pub mod engine;
pub mod model;
pub mod ui;
