//! Cartridge UI widget library.
//!
//! All widgets receive `&mut Screen` in `draw()` and render within a given `Rect`.
//! Colors come from `screen.theme`. Widgets do NOT own the screen.

pub mod chart;
pub mod confirm_dialog;
pub mod detail_view;
pub mod footer;
pub mod grid_view;
pub mod list_view;
pub mod loading;
pub mod progress_bar;
pub mod status_bar;
pub mod tab_bar;
pub mod table;
pub mod text_input;
pub mod toast;

pub use chart::{LineChart, SparkLine};
pub use confirm_dialog::ConfirmDialog;
pub use detail_view::DetailView;
pub use footer::{ButtonHint, Footer};
pub use grid_view::{GridItem, GridView};
pub use list_view::{ListItem, ListView};
pub use loading::LoadingIndicator;
pub use progress_bar::ProgressBar;
pub use status_bar::StatusBar;
pub use tab_bar::{Tab, TabBar};
pub use table::{Column, Table};
pub use text_input::{TextInput, TextInputResult};
pub use toast::{Toast, ToastManager};

