use std::rc::Rc;

use gpui::{App, Window};
use xolhn_core::model::SearchFilter;
use xolhn_core::settings::LineHeightDensity;
use xolhn_core::theme::ThemeId;

pub type ActionCallback = Rc<dyn Fn(&mut Window, &mut App)>;
pub type BoolCallback = Rc<dyn Fn(bool, &mut Window, &mut App)>;
pub type IdCallback = Rc<dyn Fn(u64, &mut Window, &mut App)>;
pub type UrlCallback = Rc<dyn Fn(String, &mut Window, &mut App)>;
pub type OptionalTextCallback = Rc<dyn Fn(Option<String>, &mut Window, &mut App)>;
pub type ThemeCallback = Rc<dyn Fn(ThemeId, &mut Window, &mut App)>;
pub type FontSizeCallback = Rc<dyn Fn(f32, &mut Window, &mut App)>;
pub type LineHeightCallback = Rc<dyn Fn(LineHeightDensity, &mut Window, &mut App)>;
pub type SearchFilterCallback = Rc<dyn Fn(SearchFilter, &mut Window, &mut App)>;

pub mod app_view;
pub mod comment_tree;
pub mod reader_pane;
pub mod search_view;
pub mod settings_view;
pub mod story_list;
