use std::cell::Cell;

use floem::peniko::Color;
use parking_lot::Mutex;

use crate::tailwind;

#[derive(Clone)]
pub struct Theme {
    pub bg: Color,
    pub bg_plus: Color,
    pub bg_plus2: Color,
    pub bg_minus: Color,
    pub fg_minus: Color,
    pub bd: Color,
    pub fg: Color,
    pub fg_plus: Color,
    pub accent: Color,
    pub fg_on_accent: Color,
    pub accent_dim: Color,
    pub unavailable: Color,
}

impl Theme {
    pub const fn dark() -> Theme {
        Theme {
            bg: Color::rgb8(28, 30, 31),
            bg_plus: Color::rgb8(28 + 5, 30 + 5, 31 + 5),
            bg_plus2: Color::rgb8(28 + 20, 30 + 20, 31 + 20),
            bd: Color::rgb8(75 - 25, 75 - 25, 75 - 25),
            bg_minus: Color::rgb8(28 - 5, 30 - 5, 31 - 5),
            fg_on_accent: Color::rgb8(245, 245, 245),
            fg_minus: Color::rgb8(225, 225, 225),
            fg: Color::rgb8(230, 230, 230),
            fg_plus: Color::WHITE,
            accent: Color::rgb8(11, 132, 255),
            accent_dim: Color::rgb8(30, 64, 175),
            unavailable: Color::rgb8(100, 116, 139),
        }
    }
}

pub fn theme() -> Theme {
    THEME.lock().clone()
}
pub static THEME: parking_lot::Mutex<Theme> = Mutex::new(Theme::dark());
